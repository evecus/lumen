//! 文本缓冲区：基于 `ropey` 的可编辑文本 + 每标签独立的撤销/重做历史。
//!
//! 设计要点：
//! - 光标/选区位置统一用「字符索引」（`usize`，指 rope 中的 unicode 标量值序号）表示；
//! - 行号由 ropey 的 `line_to_char` / `char_to_line` 推导，因此对大文件也能高效定位；
//! - 连续的纯插入（正常打字）会被合并成同一个撤销单元，退格/删除各自独立，
//!   撤销粒度贴近 CodeMirror 的体验。

use ropey::Rope;
use std::ops::Range;

#[derive(Clone)]
struct Edit {
    /// 变更起始处的字符索引。
    start: usize,
    /// 被删除的文本（撤销时插回）。
    old: String,
    /// 被插入的文本（撤销时移除）。
    new: String,
}

pub struct Buffer {
    rope: Rope,
    /// 上次保存时的内容快照，用于脏标记判定。
    saved: String,
    dirty: bool,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
}

impl Buffer {
    pub fn new(text: &str) -> Self {
        let rope = Rope::from_str(text);
        Buffer {
            saved: rope.to_string(),
            rope,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// 标记为已保存：把当前内容记为「干净」基准，并清空重做栈。
    pub fn mark_saved(&mut self) {
        self.saved = self.rope.to_string();
        self.dirty = false;
        self.redo.clear();
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// 整段替换内容（用于"全部替换"后写回）。会清空撤销/重做栈。
    pub fn set_text(&mut self, text: &str) {
        self.rope = Rope::from_str(text);
        self.saved = self.rope.to_string();
        self.dirty = false;
        self.undo.clear();
        self.redo.clear();
    }

    pub fn char_count(&self) -> usize {
        self.rope.len_chars()
    }

    #[allow(dead_code)]
    pub fn byte_count(&self) -> usize {
        self.rope.len_bytes()
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn to_string(&self) -> String {
        self.rope.to_string()
    }

    // ===== 字符索引 ↔ 行列 转换 =====

    pub fn char_to_line(&self, char_idx: usize) -> usize {
        let idx = char_idx.min(self.rope.len_chars());
        self.rope.char_to_line(idx)
    }

    pub fn line_to_char(&self, line_idx: usize) -> usize {
        let lines = self.rope.len_lines();
        if lines == 0 {
            return 0;
        }
        self.rope.line_to_char(line_idx.min(lines - 1))
    }

    /// 取某一逻辑行的文本（不含行尾换行符）。
    #[allow(dead_code)]
    pub fn line(&self, line_idx: usize) -> String {
        if line_idx >= self.rope.len_lines() {
            return String::new();
        }
        self.rope.line(line_idx).to_string()
    }

    /// 取某一逻辑行的字符长度。
    #[allow(dead_code)]
    pub fn line_len_chars(&self, line_idx: usize) -> usize {
        if line_idx >= self.rope.len_lines() {
            return 0;
        }
        self.rope.line(line_idx).len_chars()
    }

    /// 取某一逻辑行的「可见」文本（去掉行尾换行符，避免软换行时把 \n 当成新行）。
    /// 字符索引与 `to_string()` 整体一致，只是行尾终止符被剥离。
    pub fn line_content(&self, line_idx: usize) -> String {
        if line_idx >= self.rope.len_lines() {
            return String::new();
        }
        let s: String = self.rope.line(line_idx).to_string();
        s.strip_suffix("\r\n")
            .or_else(|| s.strip_suffix('\n'))
            .or_else(|| s.strip_suffix('\r'))
            .unwrap_or(&s)
            .to_string()
    }

    // ===== 编辑操作 =====

    pub fn insert(&mut self, pos: usize, text: &str) -> usize {
        let pos = pos.min(self.rope.len_chars());
        if text.is_empty() {
            return pos;
        }
        let new_chars = text.chars().count();
        self.rope.insert(pos, text);
        self.push_insert_edit(pos, text);
        self.dirty = true;
        self.redo.clear();
        pos + new_chars
    }

    pub fn remove(&mut self, range: Range<usize>) {
        let start = range.start.min(self.rope.len_chars());
        let end = range.end.min(self.rope.len_chars());
        if start >= end {
            return;
        }
        let removed: String = self.rope.slice(start..end).chars().collect();
        self.rope.remove(start..end);
        self.undo.push(Edit {
            start,
            old: removed,
            new: String::new(),
        });
        if self.undo.len() > 500 {
            self.undo.remove(0);
        }
        self.dirty = true;
        self.redo.clear();
    }

    /// 替换一段区间：等价于 remove + insert，但作为同一撤销单元处理更自然。
    /// 返回插入后的光标位置（区间起点 + 新文本字符数）。
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> usize {
        let start = range.start.min(self.rope.len_chars());
        let end = range.end.min(self.rope.len_chars());
        if start < end {
            let removed: String = self.rope.slice(start..end).chars().collect();
            self.rope.remove(start..end);
            self.push_edit(Edit {
                start,
                old: removed,
                new: String::new(),
            });
        }
        let new_chars = text.chars().count();
        if new_chars > 0 {
            let insert_pos = start.min(self.rope.len_chars());
            self.rope.insert(insert_pos, text);
            // 把刚刚的删除与插入合并成一个 edit（替换）
            // 思路：pop 出刚 push 的删除 edit，构造一个 old=removed new=text 的合并 edit 再 push。
            // 这里直接在上面 push 的删除 edit 基础上改写更简单——
            // 但为简洁，用 insert 逻辑：先 pop 删除 edit，再 push 合并 edit。
            if let Some(mut last) = self.undo.pop() {
                last.new = text.to_string();
                self.undo.push(last);
            }
        }
        self.dirty = true;
        self.redo.clear();
        start + new_chars
    }

    fn push_insert_edit(&mut self, pos: usize, text: &str) {
        if let Some(last) = self.undo.last_mut() {
            let last_end = last.start + last.new.chars().count();
            // 合并条件：上一次是纯插入、且本次紧接其后（连续打字）
            if last.old.is_empty() && last_end == pos {
                last.new.push_str(text);
                return;
            }
        }
        self.undo.push(Edit {
            start: pos,
            old: String::new(),
            new: text.to_string(),
        });
        if self.undo.len() > 500 {
            self.undo.remove(0);
        }
    }

    fn push_edit(&mut self, edit: Edit) {
        self.undo.push(edit);
        if self.undo.len() > 500 {
            self.undo.remove(0);
        }
    }

    pub fn undo(&mut self) -> bool {
        if let Some(edit) = self.undo.pop() {
            let new_start = edit.start;
            let new_end = edit.start + edit.new.chars().count();
            if new_end > new_start {
                self.rope.remove(new_start..new_end);
            }
            if !edit.old.is_empty() {
                self.rope.insert(new_start, &edit.old);
            }
            self.redo.push(edit);
            self.recompute_dirty();
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(edit) = self.redo.pop() {
            let old_start = edit.start;
            let old_end = edit.start + edit.old.chars().count();
            if old_end > old_start {
                self.rope.remove(old_start..old_end);
            }
            if !edit.new.is_empty() {
                self.rope.insert(old_start, &edit.new);
            }
            self.undo.push(edit);
            self.recompute_dirty();
            true
        } else {
            false
        }
    }

    fn recompute_dirty(&mut self) {
        self.dirty = self.rope.to_string() != self.saved;
    }
}
