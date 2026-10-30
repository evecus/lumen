//! Lumen 的主界面与全部交互逻辑（GPUI 重写版）。
//!
//! 整体结构：单一根实体 `Workspace` 持有所有标签页与 UI 状态，负责绘制标题栏 /
//! 工具栏 / 侧边栏 / 编辑器 / 状态栏 / Toast / 对话框，并统一处理键鼠输入。
//!
//! 编辑器区域是一个 `canvas` 元素，绘制完全由我们控制：行号槽、软换行
//! （基于 `window.text_system().shape_text` 得到的 `WrappedLine`）、选区与查找匹配
//! 高亮（通过 `TextRun` 的 `background_color`）、光标、命中测试都自行实现，以对齐
//! 原 CodeMirror 版观感。

use gpui::prelude::FluentBuilder;
use gpui::*;
use std::path::Path;
use std::time::Duration;

use crate::buffer::Buffer;
use crate::file_assoc;
use crate::file_ops::read_text_file;
use crate::file_ops::write_text_file;
use crate::search::{find_matches, replace_all, replace_one, SearchMode, SearchQuery};
use crate::theme::*;

const ENCODING_OPTIONS: &[(&str, &str)] = &[
    ("UTF-8", "UTF-8（推荐，适合绝大多数文件）"),
    ("GBK", "GBK（简体中文，适合 .bat 等旧版 Windows 文件）"),
    ("GB18030", "GB18030（GBK 的超集，兼容性更广）"),
    ("BIG5", "Big5（繁体中文）"),
    ("UTF-16LE", "UTF-16 LE"),
    ("UTF-16BE", "UTF-16 BE"),
    ("SHIFT-JIS", "Shift-JIS（日文）"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

/// 当前键盘输入应当流向哪里。
#[derive(Clone, Copy, PartialEq, Eq)]
enum InputFocus {
    None,
    NewFileName,
    SaveFileName,
    SearchFind,
    SearchReplace,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DragMode {
    None,
    Select,
    Scroll,
}

/// 编辑器区域内每个逻辑行的绘制几何参数（像素，原始 f32 便于算术，绘制时再包 px）。
#[derive(Clone, Copy)]
struct Geom {
    gutter_w: f32,
    content_left: f32,
    content_width: f32,
    content_top: f32,
    content_height: f32,
    wrap_width: f32,
}

/// 一个打开的标签页（含独立的光标 / 选区 / 滚动状态）。
pub struct Tab {
    pub id: usize,
    pub name: String,
    pub path: Option<String>,
    pub buffer: Buffer,
    pub encoding: String,
    pub eol: String,
    pub cursor: usize,
    pub anchor: usize,
    pub scroll: f32,
}

impl Tab {
    fn new(name: String, path: Option<String>, buffer: Buffer, encoding: String, eol: String) -> Self {
        Tab {
            id: 0,
            name,
            path,
            buffer,
            encoding,
            eol,
            cursor: 0,
            anchor: 0,
            scroll: 0.0,
        }
    }

    fn is_dirty(&self) -> bool {
        self.buffer.is_dirty()
    }
}

/// 查找面板状态（全局唯一，匹配针对当前活动标签计算）。
struct SearchPanel {
    visible: bool,
    mode_replace: bool,
    pattern: String,
    replace: String,
    match_case: bool,
    whole_word: bool,
    backward: bool,
    wrap: bool,
    search_mode: SearchMode,
    matches: Vec<(usize, usize)>,
    active: Option<usize>,
}

impl Default for SearchPanel {
    fn default() -> Self {
        Self {
            visible: false,
            mode_replace: false,
            pattern: String::new(),
            replace: String::new(),
            match_case: false,
            whole_word: false,
            backward: false,
            wrap: true,
            search_mode: SearchMode::Normal,
            matches: Vec::new(),
            active: None,
        }
    }
}

/// 文件打开 / 另存为对话框状态。
#[derive(Clone)]
struct FileDialog {
    save: bool,
    dir: String,
    entries: Vec<DirEntry>,
    filename: String,
    error: String,
}

#[derive(Clone)]
struct DirEntry {
    name: String,
    is_dir: bool,
}

/// 新建文件对话框状态。
#[derive(Clone)]
struct NewFileDialog {
    name: String,
    encoding: usize,
    error: String,
}

pub struct Workspace {
    focus_handle: FocusHandle,
    tabs: Vec<Tab>,
    active: Option<usize>,
    next_id: usize,
    untitled: usize,
    sidebar_visible: bool,

    editor_bounds: Bounds<Pixels>,

    search: SearchPanel,
    new_file: Option<NewFileDialog>,
    file_dialog: Option<FileDialog>,

    input_focus: InputFocus,
    drag_mode: DragMode,

    toast: Option<(String, ToastKind)>,
}

impl Workspace {
    pub fn new(cx: &mut App, launch_file: Option<String>) -> Entity<Self> {
        let ws = cx.new(|cx| {
            let focus_handle = cx.focus_handle();
            Workspace {
                focus_handle,
                tabs: Vec::new(),
                active: None,
                next_id: 1,
                untitled: 1,
                sidebar_visible: true,
                editor_bounds: Bounds::default(),
                search: SearchPanel::default(),
                new_file: None,
                file_dialog: None,
                input_focus: InputFocus::None,
                drag_mode: DragMode::None,
                toast: None,
            }
        });
        // 启动时若有通过"打开方式"传入的文件，直接打开；否则给一个空白标签。
        if let Some(path) = launch_file {
            ws.update(cx, |this, cx| {
                this.open_file_by_path(&path, cx);
            });
        } else {
            ws.update(cx, |this, _cx| {
                this.create_untitled();
            });
        }
        ws
    }

    // ===== 标签页管理 =====

    fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|i| self.tabs.get(i))
    }

    fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.active.and_then(|i| self.tabs.get_mut(i))
    }

    fn create_untitled(&mut self) {
        let n = self.untitled;
        self.untitled += 1;
        let name = format!("未命名-{}.txt", n);
        let mut tab = Tab::new(name, None, Buffer::new(""), "UTF-8".to_string(), "LF".to_string());
        tab.id = self.next_id;
        self.next_id += 1;
        self.tabs.push(tab);
        self.active = Some(self.tabs.len() - 1);
        self.sync_search();
    }

    fn open_file_by_path(&mut self, path: &str, cx: &mut Context<Workspace>) {
        let path = path.to_string();
        if let Some(idx) = self.tabs.iter().position(|t| t.path.as_deref() == Some(path.as_str())) {
            self.active = Some(idx);
            self.sync_search();
            return;
        }
        match read_text_file(&path) {
            Ok(res) if res.is_binary => {
                let name = basename(&path);
                self.show_toast(
                    format!("“{}” 看起来是二进制文件，无法以文本方式打开", name),
                    ToastKind::Error,
                    cx,
                );
            }
            Ok(res) => {
                let name = basename(&path);
                let mut tab = Tab::new(
                    name,
                    Some(path.clone()),
                    Buffer::new(&res.content),
                    res.encoding,
                    res.eol,
                );
                tab.id = self.next_id;
                self.next_id += 1;
                self.tabs.push(tab);
                self.active = Some(self.tabs.len() - 1);
                self.sync_search();
            }
            Err(e) => {
                self.show_toast(format!("打开文件失败：{}", e), ToastKind::Error, cx);
            }
        }
    }

    fn close_tab(&mut self, idx: usize, cx: &mut Context<Workspace>) {
        if idx >= self.tabs.len() {
            return;
        }
        if self.tabs[idx].is_dirty() {
            self.show_toast(
                format!("“{}” 尚有未保存的更改，已直接关闭", self.tabs[idx].name),
                ToastKind::Info,
                cx,
            );
        }
        self.tabs.remove(idx);
        if self.tabs.is_empty() {
            self.active = None;
            self.create_untitled();
        } else {
            let new = idx.min(self.tabs.len() - 1);
            self.active = Some(new);
        }
        self.sync_search();
    }

    fn set_active(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.active = Some(idx);
            self.sync_search();
        }
    }

    // ===== 保存 =====

    fn save_active(&mut self, cx: &mut Context<Workspace>) {
        let Some(tab) = self.active_tab_mut() else { return };
        if tab.path.is_none() {
            self.open_save_dialog(cx);
            return;
        }
        let path = tab.path.clone().unwrap();
        let content = tab.buffer.to_string();
        let eol = tab.eol.clone();
        let enc = tab.encoding.clone();
        let name = tab.name.clone();
        match write_text_file(&path, &content, &eol, Some(&enc)) {
            Ok(res) => {
                tab.buffer.mark_saved();
                if res.had_unmappable_chars {
                    self.show_toast(
                        format!("已保存 “{}”，但部分字符无法用 {} 表示，已替换为 ?", name, enc),
                        ToastKind::Info,
                        cx,
                    );
                } else {
                    self.show_toast(format!("已保存 “{}”", name), ToastKind::Success, cx);
                }
            }
            Err(e) => self.show_toast(format!("保存失败：{}", e), ToastKind::Error, cx),
        }
    }

    fn save_active_as(&mut self, cx: &mut Context<Workspace>) {
        self.open_save_dialog(cx);
    }

    // ===== 文件对话框 =====

    fn open_open_dialog(&mut self, cx: &mut Context<Workspace>) {
        let home = home_dir();
        let mut dlg = FileDialog {
            save: false,
            dir: home.clone(),
            entries: Vec::new(),
            filename: String::new(),
            error: String::new(),
        };
        dlg.reload();
        self.file_dialog = Some(dlg);
        self.input_focus = InputFocus::SaveFileName;
        cx.notify();
    }

    fn open_save_dialog(&mut self, cx: &mut Context<Workspace>) {
        let home = home_dir();
        let default = self
            .active_tab()
            .map(|t| t.name.clone())
            .unwrap_or_else(|| "未命名.txt".to_string());
        let mut dlg = FileDialog {
            save: true,
            dir: home,
            entries: Vec::new(),
            filename: default,
            error: String::new(),
        };
        dlg.reload();
        self.file_dialog = Some(dlg);
        self.input_focus = InputFocus::SaveFileName;
        cx.notify();
    }

    fn confirm_file_dialog(&mut self, cx: &mut Context<Workspace>) {
        let Some(dlg) = self.file_dialog.clone() else { return };
        if dlg.save {
            let target = Path::new(&dlg.dir).join(&dlg.filename);
            let target = target.to_string_lossy().to_string();
            if target.is_empty() {
                return;
            }
            let pre = self.active_tab().map(|t| {
                (
                    t.buffer.to_string(),
                    t.eol.clone(),
                    t.encoding.clone(),
                    t.name.clone(),
                )
            });
            if let Some((content, eol, enc, name)) = pre {
                match write_text_file(&target, &content, &eol, Some(&enc)) {
                    Ok(res) => {
                        if let Some(tab) = self.active_tab_mut() {
                            tab.path = Some(target.clone());
                            tab.name = name.clone();
                            tab.buffer.mark_saved();
                        }
                        if res.had_unmappable_chars {
                            self.show_toast(
                                "已保存，但部分字符无法用该编码表示，已替换为 ?".to_string(),
                                ToastKind::Info,
                                cx,
                            );
                        } else {
                            self.show_toast(format!("已保存 “{}”", name), ToastKind::Success, cx);
                        }
                    }
                    Err(e) => {
                        self.show_toast(format!("保存失败：{}", e), ToastKind::Error, cx);
                        return;
                    }
                }
            }
            self.file_dialog = None;
            self.input_focus = InputFocus::None;
            cx.notify();
        } else {
            if !dlg.filename.is_empty() {
                let target = Path::new(&dlg.dir).join(&dlg.filename);
                let target = target.to_string_lossy().to_string();
                self.file_dialog = None;
                self.input_focus = InputFocus::None;
                self.open_file_by_path(&target, cx);
            } else {
                self.file_dialog = None;
                self.input_focus = InputFocus::None;
                cx.notify();
            }
        }
    }

    fn navigate_file_dialog(&mut self, name: String, cx: &mut Context<Workspace>) {
        if let Some(dlg) = self.file_dialog.as_mut() {
            if name == ".." {
                if let Some(parent) = Path::new(&dlg.dir).parent() {
                    dlg.dir = parent.to_string_lossy().to_string();
                }
            } else {
                let joined = Path::new(&dlg.dir).join(&name);
                if joined.is_dir() {
                    dlg.dir = joined.to_string_lossy().to_string();
                } else {
                    dlg.filename = name;
                }
            }
            dlg.error.clear();
            dlg.reload();
            cx.notify();
        }
    }

    // ===== 新建文件对话框 =====

    fn open_new_file_dialog(&mut self, cx: &mut Context<Workspace>) {
        let n = self.untitled;
        let name = format!("未命名-{}.txt", n);
        self.new_file = Some(NewFileDialog {
            name,
            encoding: 0,
            error: String::new(),
        });
        self.input_focus = InputFocus::NewFileName;
        cx.notify();
    }

    fn confirm_new_file(&mut self, cx: &mut Context<Workspace>) {
        if let Some(dlg) = self.new_file.take() {
            let trimmed = dlg.name.trim().to_string();
            if trimmed.is_empty() {
                self.new_file = Some(NewFileDialog {
                    name: dlg.name,
                    encoding: dlg.encoding,
                    error: "文件名不能为空".to_string(),
                });
                cx.notify();
                return;
            }
            if trimmed.contains('\\')
                || trimmed.contains('/')
                || trimmed.contains(':')
                || trimmed.contains('*')
                || trimmed.contains('?')
                || trimmed.contains('"')
                || trimmed.contains('<')
                || trimmed.contains('>')
                || trimmed.contains('|')
            {
                self.new_file = Some(NewFileDialog {
                    name: dlg.name,
                    encoding: dlg.encoding,
                    error: "文件名不能包含 \\ / : * ? \" < > |".to_string(),
                });
                cx.notify();
                return;
            }
            let enc = ENCODING_OPTIONS[dlg.encoding].0.to_string();
            let mut tab = Tab::new(trimmed, None, Buffer::new(""), enc, "LF".to_string());
            tab.id = self.next_id;
            self.next_id += 1;
            self.tabs.push(tab);
            self.active = Some(self.tabs.len() - 1);
            self.input_focus = InputFocus::None;
            self.sync_search();
            cx.notify();
        }
    }

    // ===== 查找 / 替换 =====

    fn open_search(&mut self, replace: bool, cx: &mut Context<Workspace>) {
        self.search.visible = true;
        self.search.mode_replace = replace;
        self.input_focus = InputFocus::SearchFind;
        self.sync_search();
        cx.notify();
    }

    fn close_search(&mut self, cx: &mut Context<Workspace>) {
        self.search.visible = false;
        self.input_focus = InputFocus::None;
        cx.notify();
    }

    fn sync_search(&mut self) {
        let q = self.current_query();
        if let Some(tab) = self.active_tab() {
            let text = tab.buffer.to_string();
            let matches = find_matches(&text, &q);
            self.search.matches = matches;
        } else {
            self.search.matches.clear();
        }
        self.search.active = if self.search.matches.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    fn current_query(&self) -> SearchQuery {
        SearchQuery {
            pattern: self.search.pattern.clone(),
            replace: self.search.replace.clone(),
            mode: self.search.search_mode,
            match_case: self.search.match_case,
            whole_word: self.search.whole_word,
        }
    }

    fn find_next(&mut self, cx: &mut Context<Workspace>) {
        if self.search.matches.is_empty() {
            return;
        }
        let n = self.search.matches.len();
        let cur = self.search.active.unwrap_or(0);
        let next = if self.search.backward {
            if cur == 0 { n - 1 } else { cur - 1 }
        } else if cur + 1 >= n {
            if self.search.wrap {
                0
            } else {
                cur
            }
        } else {
            cur + 1
        };
        self.search.active = Some(next);
        self.apply_active_match(cx);
    }

    fn find_prev(&mut self, cx: &mut Context<Workspace>) {
        let old_back = self.search.backward;
        self.search.backward = !old_back;
        self.find_next(cx);
        self.search.backward = old_back;
    }

    fn apply_active_match(&mut self, cx: &mut Context<Workspace>) {
        let Some(idx) = self.search.active else { return };
        let Some(&(s, e)) = self.search.matches.get(idx) else { return };
        if let Some(tab) = self.active_tab_mut() {
            tab.cursor = e;
            tab.anchor = s;
        }
        cx.notify();
    }

    fn replace_current(&mut self, cx: &mut Context<Workspace>) {
        let Some(idx) = self.search.active else { return };
        let Some(&range) = self.search.matches.get(idx) else { return };
        let q = self.current_query();
        let Some(tab) = self.active_tab_mut() else { return };
        let text = tab.buffer.to_string();
        if let Some(new) = replace_one(&text, &q, range) {
            tab.buffer.set_text(&new);
            self.sync_search();
            cx.notify();
        }
    }

    fn replace_all_matches(&mut self, cx: &mut Context<Workspace>) {
        let q = self.current_query();
        let Some(tab) = self.active_tab_mut() else { return };
        let text = tab.buffer.to_string();
        if let Some((new, count)) = replace_all(&text, &q) {
            if count > 0 {
                tab.buffer.set_text(&new);
                self.show_toast(format!("已替换 {} 处", count), ToastKind::Success, cx);
            } else {
                self.show_toast("没有可替换的匹配".to_string(), ToastKind::Info, cx);
            }
            self.sync_search();
            cx.notify();
        }
    }

    fn count_matches(&mut self, cx: &mut Context<Workspace>) {
        if self.search.pattern.is_empty() {
            self.show_toast("请输入查找内容".to_string(), ToastKind::Info, cx);
            return;
        }
        let n = self.search.matches.len();
        self.show_toast(format!("共找到 {} 处匹配", n), ToastKind::Info, cx);
    }

    // ===== 文件关联 =====

    fn set_default_for_active(&mut self, cx: &mut Context<Workspace>) {
        let ext = self.active_tab().and_then(|t| extension_of(&t.name));
        match ext {
            None => self.show_toast("当前文件没有可识别的扩展名".to_string(), ToastKind::Info, cx),
            Some(ext) => match file_assoc::set_default_file_extension(&ext) {
                Ok(()) => self.show_toast(
                    format!("已将 Lumen 设为 .{} 的默认打开程序", ext),
                    ToastKind::Success,
                    cx,
                ),
                Err(e) => self.show_toast(format!("设置失败：{}", e), ToastKind::Error, cx),
            },
        }
    }

    // ===== Toast =====

    fn show_toast(&mut self, msg: String, kind: ToastKind, cx: &mut Context<Workspace>) {
        self.toast = Some((msg, kind));
        cx.spawn(async move |this, cx| {
            smol::Timer::after(Duration::from_millis(2200)).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.toast = None;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
        cx.notify();
    }

    // ===== 编辑器编辑操作 =====

    fn insert_text_at_cursor(&mut self, text: &str, cx: &mut Context<Workspace>) {
        let (s, e) = match self.active_tab() {
            Some(tab) => {
                let a = tab.anchor;
                let c = tab.cursor;
                (a.min(c), a.max(c))
            }
            None => return,
        };
        let new_pos = if let Some(tab) = self.active_tab_mut() {
            if s < e {
                tab.buffer.replace(s..e, text)
            } else {
                tab.buffer.insert(tab.cursor, text)
            }
        } else {
            return;
        };
        if let Some(tab) = self.active_tab_mut() {
            tab.cursor = new_pos;
            tab.anchor = new_pos;
        }
        cx.notify();
    }

    fn backspace(&mut self, cx: &mut Context<Workspace>) {
        let (s, e) = match self.active_tab() {
            Some(tab) => {
                let a = tab.anchor;
                let c = tab.cursor;
                (a.min(c), a.max(c))
            }
            None => return,
        };
        let Some(tab) = self.active_tab_mut() else { return };
        if s < e {
            tab.buffer.remove(s..e);
            tab.cursor = s;
            tab.anchor = s;
        } else if tab.cursor > 0 {
            let prev = prev_char_boundary(&tab.buffer, tab.cursor);
            tab.buffer.remove(prev..tab.cursor);
            tab.cursor = prev;
            tab.anchor = prev;
        }
        cx.notify();
    }

    fn delete_forward(&mut self, cx: &mut Context<Workspace>) {
        let (s, e) = match self.active_tab() {
            Some(tab) => {
                let a = tab.anchor;
                let c = tab.cursor;
                (a.min(c), a.max(c))
            }
            None => return,
        };
        let Some(tab) = self.active_tab_mut() else { return };
        if s < e {
            tab.buffer.remove(s..e);
            tab.cursor = s;
            tab.anchor = s;
        } else {
            let next = next_char_boundary(&tab.buffer, tab.cursor);
            if next > tab.cursor {
                tab.buffer.remove(tab.cursor..next);
            }
        }
        cx.notify();
    }

    fn select_all(&mut self, cx: &mut Context<Workspace>) {
        let Some(tab) = self.active_tab_mut() else { return };
        tab.anchor = 0;
        tab.cursor = tab.buffer.char_count();
        cx.notify();
    }

    fn undo(&mut self, cx: &mut Context<Workspace>) {
        let Some(tab) = self.active_tab_mut() else { return };
        if tab.buffer.undo() {
            tab.cursor = tab.cursor.min(tab.buffer.char_count());
            tab.anchor = tab.cursor;
            cx.notify();
        }
    }

    fn redo(&mut self, cx: &mut Context<Workspace>) {
        let Some(tab) = self.active_tab_mut() else { return };
        if tab.buffer.redo() {
            tab.cursor = tab.cursor.min(tab.buffer.char_count());
            tab.anchor = tab.cursor;
            cx.notify();
        }
    }

    fn copy_selection(&mut self, cx: &mut Context<Workspace>) {
        let Some(tab) = self.active_tab() else { return };
        let a = tab.anchor;
        let c = tab.cursor;
        let (s, e) = (a.min(c), a.max(c));
        if s < e {
            let text = tab.buffer.to_string();
            let slice: String = text.chars().skip(s).take(e - s).collect();
            cx.write_to_clipboard(ClipboardItem::new_string(slice));
        }
    }

    fn cut_selection(&mut self, cx: &mut Context<Workspace>) {
        self.copy_selection(cx);
        self.backspace(cx);
    }

    fn paste(&mut self, cx: &mut Context<Workspace>) {
        if let Some(item) = cx.read_from_clipboard() {
            if let Some(text) = item.text() {
                self.insert_text_at_cursor(&text, cx);
            }
        }
    }

    // ===== 编辑器几何 / 绘制 / 命中测试 =====

    fn geometry(&self, bounds: &Bounds<Pixels>) -> Geom {
        let w = f32::from(bounds.size.width);
        let h = f32::from(bounds.size.height);
        let line_count = self.active_tab().map(|t| t.buffer.line_count()).unwrap_or(0);
        let digits = (line_count.max(1)).to_string().len().max(3);
        let gutter_w = (digits as f32 * 8.0 + 16.0).max(44.0);
        let content_top = 10.0_f32;
        let content_left = gutter_w + 8.0;
        let content_width = (w - content_left - 8.0).max(20.0);
        let content_height = (h - content_top * 2.0).max(20.0);
        let wrap_width = content_width;
        Geom {
            gutter_w,
            content_left,
            content_width,
            content_top,
            content_height,
            wrap_width,
        }
    }

    /// 计算内容总高度与最大滚动量（第一遍测量）。
    fn content_metrics(&self, geom: &Geom, window: &Window) -> (f32, f32) {
        let Some(tab) = self.active_tab() else { return (0.0, 0.0) };
        let n = tab.buffer.line_count();
        let mut total = 0.0_f32;
        for i in 0..n {
            let lt = tab.buffer.line_content(i);
            if let Ok(wrapped) = window.text_system().shape_text(
                SharedString::from(lt.to_string()),
                FONT_SIZE,
                &[mk_run(lt.len(), INK)],
                Some(px(geom.wrap_width)),
                None,
            ) {
                total += (wrapped.first().map(|l| l.wrap_boundaries().len() + 1).unwrap_or(1)) as f32 * f32::from(LINE_HEIGHT);
            } else {
                total += f32::from(LINE_HEIGHT);
            }
        }
        let max_scroll = (total - geom.content_height).max(0.0);
        (total, max_scroll)
    }

    /// 把内容空间坐标 (x 相对 content_left, y 相对 content_top) 映射到字符索引。
    fn char_at_xy(&self, geom: &Geom, x: f32, y: f32, window: &Window) -> usize {
        let Some(tab) = self.active_tab() else { return 0 };
        let n = tab.buffer.line_count();
        let mut yy = geom.content_top;
        for i in 0..n {
            let lt = tab.buffer.line_content(i);
            let wrapped = window
                .text_system()
                .shape_text(
                    SharedString::from(lt.to_string()),
                    FONT_SIZE,
                    &[mk_run(lt.len(), INK)],
                    Some(px(geom.wrap_width)),
                    None,
                )
                .unwrap_or_default();
            let Some(line0) = wrapped.first() else { continue };
            let vc = line0.wrap_boundaries().len() + 1;
            let h = vc as f32 * f32::from(LINE_HEIGHT);
            if y < yy + h {
                let local_y = (y - yy).max(0.0);
                let wix = (local_y / f32::from(LINE_HEIGHT)).floor() as usize;
                let pos = point(px(x.max(0.0)), px(wix as f32 * f32::from(LINE_HEIGHT)));
                let byte = line0
                    .closest_index_for_position(pos, LINE_HEIGHT)
                    .unwrap_or_else(|e| e);
                let lcs = tab.buffer.line_to_char(i);
                let char_off = lt[..byte.min(lt.len())].chars().count();
                return lcs + char_off;
            }
            yy += h;
        }
        tab.buffer.char_count()
    }

    /// 计算当前光标在内容空间中的 (x, y)（y 相对 content_top）。
    fn caret_xy(&self, geom: &Geom, window: &Window) -> Option<(f32, f32)> {
        let tab = self.active_tab()?;
        let cursor = tab.cursor;
        let line = tab.buffer.char_to_line(cursor);
        let visible = tab.buffer.line_content(line);
        let lcs = tab.buffer.line_to_char(line);
        let local_char = cursor - lcs;
        let byte = visible
            .char_indices()
            .nth(local_char)
            .map(|(b, _)| b)
            .unwrap_or(visible.len());
        let wrapped = window
            .text_system()
            .shape_text(
                SharedString::from(visible.to_string()),
                FONT_SIZE,
                &[mk_run(visible.len(), INK)],
                Some(px(geom.wrap_width)),
                None,
            )
            .unwrap_or_default();
        let line0 = wrapped.first()?;
        let pos = line0.position_for_index(byte, LINE_HEIGHT)?;
        let mut yy = geom.content_top;
        for i in 0..line {
            let l = tab.buffer.line_content(i);
            let w = window
                .text_system()
                .shape_text(
                    SharedString::from(l.to_string()),
                    FONT_SIZE,
                    &[mk_run(l.len(), INK)],
                    Some(px(geom.wrap_width)),
                    None,
                )
                .unwrap_or_default();
            yy += (w.first().map(|l| l.wrap_boundaries().len() + 1).unwrap_or(1)) as f32 * f32::from(LINE_HEIGHT);
        }
        Some((f32::from(pos.x), yy + f32::from(pos.y)))
    }

    /// 为某一逻辑行构造带选区 / 匹配高亮的 `TextRun` 序列。
    fn line_runs(&self, tab: &Tab, line_idx: usize, line_text: &str) -> Vec<TextRun> {
        let n = tab.buffer.line_count();
        let lcs = tab.buffer.line_to_char(line_idx);
        let lce = if line_idx + 1 < n {
            tab.buffer.line_to_char(line_idx + 1)
        } else {
            tab.buffer.char_count()
        };

        let mut intervals: Vec<(usize, usize, Hsla)> = Vec::new();
        let (s, e) = (tab.anchor.min(tab.cursor), tab.anchor.max(tab.cursor));
        if s < e {
            if let Some(seg) = intersect(lcs, lce, s, e) {
                let (bs, be) = byte_range_in_line(line_text, seg.0 - lcs, seg.1 - lcs);
                intervals.push((bs, be, SELECTION));
            }
        }
        for (i, &(ms, me)) in self.search.matches.iter().enumerate() {
            if let Some(seg) = intersect(lcs, lce, ms, me) {
                let (bs, be) = byte_range_in_line(line_text, seg.0 - lcs, seg.1 - lcs);
                let color = if Some(i) == self.search.active {
                    MATCH_HL_SEL
                } else {
                    MATCH_HL
                };
                intervals.push((bs, be, color));
            }
        }

        intervals.sort_by_key(|a| a.0);

        let mut runs = Vec::new();
        let mut pos = 0;
        for (bs, be, color) in intervals {
            let s2 = bs.max(pos);
            let e2 = be.max(pos);
            if s2 > pos {
                runs.push(mk_run(s2 - pos, INK));
            }
            if e2 > pos {
                runs.push(mk_run_bg(e2 - s2, INK, color));
                pos = e2;
            }
        }
        if pos < line_text.len() {
            runs.push(mk_run(line_text.len() - pos, INK));
        }
        if runs.is_empty() {
            runs.push(mk_run(line_text.len(), INK));
        }
        runs
    }

    fn paint_editor(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let geom = self.geometry(&bounds);
        window.paint_quad(fill(bounds, BG_EDITOR));

        let Some(tab) = self.active_tab() else {
            return;
        };
        let (total, max_scroll) = self.content_metrics(&geom, window);
        let scroll = tab.scroll.max(0.0).min(max_scroll);

        let view_top = f32::from(bounds.origin.y) + geom.content_top;
        let view_bottom = f32::from(bounds.origin.y) + f32::from(bounds.size.height) - geom.content_top;

        // 行号槽背景
        window.paint_quad(fill(
            Bounds::new(
                point(bounds.origin.x, bounds.origin.y),
                size(px(geom.gutter_w), bounds.size.height),
            ),
            BG_PANEL,
        ));
        // 分隔线
        window.paint_quad(fill(
            Bounds::new(
                point(bounds.origin.x + px(geom.gutter_w) - px(1.0), bounds.origin.y),
                size(px(1.0), bounds.size.height),
            ),
            BORDER,
        ));

        let caret_line = tab.buffer.char_to_line(tab.cursor);
        let mut y = f32::from(bounds.origin.y) + geom.content_top - scroll;
        let n = tab.buffer.line_count();

        for line_idx in 0..n {
            let lt = tab.buffer.line_content(line_idx);
            let runs = self.line_runs(tab, line_idx, &lt);
            let wrapped = window
                .text_system()
                .shape_text(
                    SharedString::from(lt.to_string()),
                    FONT_SIZE,
                    &runs,
                    Some(px(geom.wrap_width)),
                    None,
                )
                .unwrap_or_default();
            let vc = wrapped.first().map(|l| l.wrap_boundaries().len() + 1).unwrap_or(1);
            let h = vc as f32 * f32::from(LINE_HEIGHT);
            let line_top = y;
            let line_bottom = y + h;

            if line_bottom < view_top - 60.0 || line_top > view_bottom + 60.0 {
                y += h;
                continue;
            }

            // 当前行高亮
            if line_idx == caret_line {
                window.paint_quad(fill(
                    Bounds::new(
                        point(bounds.origin.x + px(geom.content_left), px(line_top)),
                        size(px(geom.content_width), px(h)),
                    ),
                    BG_ACTIVE,
                ));
            }

            let origin = point(bounds.origin.x + px(geom.content_left), px(line_top));
            if let Some(line0) = wrapped.first() {
                let _ = line0.paint_background(origin, LINE_HEIGHT, TextAlign::Left, None, window, cx);
                let _ = line0.paint(origin, LINE_HEIGHT, TextAlign::Left, None, window, cx);
            }

            // 行号（仅逻辑行首显示）
            let num_color = if line_idx == caret_line { ACCENT } else { INK_FAINT };
            let num_str = (line_idx + 1).to_string();
            let num_run = TextRun {
                len: num_str.len(),
                font: font(EDITOR_FONT),
                color: num_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window
                .text_system()
                .shape_line(SharedString::from(num_str.to_string()), FONT_SIZE, &[num_run], None);
            let num_x = f32::from(bounds.origin.x) + geom.gutter_w - 8.0 - f32::from(shaped.width);
            let _ = shaped.paint(point(px(num_x), px(line_top)), LINE_HEIGHT, window, cx);

            // 光标
            if self.focus_handle.is_focused(window)
                && self.input_focus == InputFocus::None
                && line_idx == caret_line
            {
                let local_char = tab.cursor - tab.buffer.line_to_char(line_idx);
                let byte = lt
                    .char_indices()
                    .nth(local_char)
                    .map(|(b, _)| b)
                    .unwrap_or(lt.len());
                if let Some(pos) = wrapped.first().and_then(|l| l.position_for_index(byte, LINE_HEIGHT)) {
                    let caret_x = f32::from(bounds.origin.x) + geom.content_left + f32::from(pos.x);
                    let caret_h = f32::from(LINE_HEIGHT) * 0.8;
                    let caret_y = line_top + f32::from(pos.y) + (f32::from(LINE_HEIGHT) - caret_h) / 2.0;
                    window.paint_quad(fill(
                        Bounds::new(point(px(caret_x), px(caret_y)), size(px(1.6), px(caret_h))),
                        ACCENT,
                    ));
                }
            }

            y += h;
        }

        // 滚动条
        if max_scroll > 0.0 {
            let track_top = f32::from(bounds.origin.y) + geom.content_top;
            let track_h = geom.content_height;
            let thumb_h = (geom.content_height / total.max(1.0)) * track_h;
            let thumb_top = track_top + (scroll / max_scroll) * (track_h - thumb_h);
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.origin.x + bounds.size.width - px(10.0), px(track_top)),
                    size(px(6.0), px(track_h)),
                ),
                BORDER,
            ));
            window.paint_quad(fill(
                Bounds::new(
                    point(bounds.origin.x + bounds.size.width - px(10.0), px(thumb_top)),
                    size(px(6.0), px(thumb_h)),
                ),
                INK_FAINT,
            ));
        }
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.focus_handle.clone();
        let entity = cx.entity();

        let active: Option<&Tab> = self.active.and_then(|i| self.tabs.get(i));
        let tabs_snapshot: Vec<(usize, String, bool, bool)> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, t)| (i, t.name.clone(), t.is_dirty(), t.path.is_some()))
            .collect();
        let active_idx = self.active;
        let sidebar_visible = self.sidebar_visible;
        let search_visible = self.search.visible;
        let new_file = self.new_file.clone();
        let file_dialog = self.file_dialog.clone();
        let _input_focus = self.input_focus;
        let toast = self.toast.clone();
        let status = active.as_ref().map(|t| {
            (
                t.encoding.clone(),
                t.eol.clone(),
                t.buffer.char_count(),
                t.buffer.line_count(),
                t.path.clone().unwrap_or_else(|| "未保存到磁盘".to_string()),
            )
        });

        div()
            .size_full()
            .bg(BG_APP)
            .flex_col()
            .track_focus(&focus_handle)
            .on_key_down(cx.listener(|view, event, window, cx| {
                view.handle_key(event, window, cx);
            }))
            .child(title_bar(active))
            .child(toolbar(
                sidebar_visible,
                active.is_some(),
                &entity,
            ))
            .child({
                let sidebar = if sidebar_visible {
                    Some(sidebar(&tabs_snapshot, active_idx, &entity))
                } else {
                    None
                };
                let editor = div()
                    .flex_1()
                    .relative()
                    .on_mouse_down(MouseButton::Left, cx.listener(|view, event, window, cx| {
                        view.on_editor_mouse_down(event, window, cx);
                    }))
                    .on_mouse_move(cx.listener(|view, event, window, cx| {
                        view.on_editor_mouse_move(event, window, cx);
                    }))
                    .on_scroll_wheel(cx.listener(|view, event, window, cx| {
                        view.on_scroll_wheel(event, window, cx);
                    }))
                    .child({
                        let prepaint_entity = entity.clone();
                        let paint_entity = entity.clone();
                        canvas(
                            move |bounds, _window, cx| {
                                prepaint_entity.update(cx, |view, _cx| {
                                    view.editor_bounds = bounds;
                                });
                            },
                            move |bounds, _pre, window, cx| {
                                paint_entity.update(cx, |view, cx| {
                                    view.paint_editor(bounds, window, cx);
                                });
                            },
                        )
                        .size_full()
                    });
                div()
                    .flex_1()
                    .flex_row()
                    .min_h(px(0.0))
                    .when_some(sidebar, |row, sb| row.child(sb))
                    .child(
                        div()
                            .flex_1()
                            .relative()
                            .child(editor)
                            .when(search_visible, |area| {
                                area.child(search_panel(&*self, &entity))
                            })
                            .when_some(file_dialog, |area, dlg| {
                                area.child(file_dialog_view(&dlg, &entity))
                            }),
                    )
            })
            .child(status_bar(status))
            .when_some(new_file, |root, nf| root.child(new_file_dialog(&nf, &entity)))
            .when_some(toast, |root, (msg, kind)| root.child(toast_view(&msg, kind)))
    }
}

// ===== 输入处理 =====

impl Workspace {
    fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Workspace>) {
        // 文件对话框 / 新建 / 查找输入优先
        match self.input_focus {
            InputFocus::NewFileName => {
                if let Some(dlg) = self.new_file.as_mut() {
                    edit_text_field(&mut dlg.name, event);
                    cx.notify();
                }
                return;
            }
            InputFocus::SaveFileName => {
                if let Some(dlg) = self.file_dialog.as_mut() {
                    edit_text_field(&mut dlg.filename, event);
                    cx.notify();
                }
                return;
            }
            InputFocus::SearchFind => {
                match event.keystroke.key.as_str() {
                    "Enter" => {
                        if event.keystroke.modifiers.shift {
                            self.find_prev(cx);
                        } else {
                            self.find_next(cx);
                        }
                    }
                    "Escape" => self.close_search(cx),
                    "F3" => {
                        if event.keystroke.modifiers.shift {
                            self.find_prev(cx);
                        } else {
                            self.find_next(cx);
                        }
                    }
                    _ => {
                        edit_text_field(&mut self.search.pattern, event);
                        self.sync_search();
                        cx.notify();
                    }
                }
                return;
            }
            InputFocus::SearchReplace => {
                match event.keystroke.key.as_str() {
                    "Enter" => self.replace_current(cx),
                    "Escape" => self.close_search(cx),
                    _ => {
                        edit_text_field(&mut self.search.replace, event);
                        cx.notify();
                    }
                }
                return;
            }
            InputFocus::None => {}
        }

        let mods = &event.keystroke.modifiers;
        let ctrl = mods.control || mods.platform;
        let key = event.keystroke.key.clone();

        if ctrl {
            let shift = mods.shift;
            match key.to_lowercase().as_str() {
                "n" => {
                    self.open_new_file_dialog(cx);
                    return;
                }
                "o" => {
                    self.open_open_dialog(cx);
                    return;
                }
                "s" => {
                    if shift {
                        self.save_active_as(cx);
                    } else {
                        self.save_active(cx);
                    }
                    return;
                }
                "f" => {
                    self.open_search(false, cx);
                    return;
                }
                "h" => {
                    self.open_search(true, cx);
                    return;
                }
                "a" => self.select_all(cx),
                "c" => self.copy_selection(cx),
                "x" => self.cut_selection(cx),
                "v" => self.paste(cx),
                "z" => self.undo(cx),
                "y" => self.redo(cx),
                _ => {}
            }
            return;
        }

        match key.as_str() {
            "F3" => {
                if mods.shift {
                    self.find_prev(cx);
                } else {
                    self.find_next(cx);
                }
            }
            "Escape" => {
                if self.search.visible {
                    self.close_search(cx);
                } else if let Some(tab) = self.active_tab_mut() {
                    tab.anchor = tab.cursor;
                    cx.notify();
                }
            }
            "Backspace" => self.backspace(cx),
            "Delete" => self.delete_forward(cx),
            "Enter" => self.insert_text_at_cursor("\n", cx),
            "Tab" => self.insert_text_at_cursor("\t", cx),
            "ArrowLeft" => self.move_cursor(-1, mods, cx),
            "ArrowRight" => self.move_cursor(1, mods, cx),
            "ArrowUp" => self.move_vertical(-1, mods, window, cx),
            "ArrowDown" => self.move_vertical(1, mods, window, cx),
            "Home" => self.cursor_line_start(cx),
            "End" => self.cursor_line_end(cx),
            "PageUp" => self.move_page(-1, mods, window, cx),
            "PageDown" => self.move_page(1, mods, window, cx),
            " " => self.insert_text_at_cursor(" ", cx),
            _ => {
                let ch = if key.is_empty() {
                    event.keystroke.key_char.clone().unwrap_or_default()
                } else {
                    key
                };
                if !ch.is_empty() && !mods.alt {
                    self.insert_text_at_cursor(&ch, cx);
                }
            }
        }
    }

    fn move_cursor(&mut self, dir: i32, mods: &Modifiers, cx: &mut Context<Workspace>) {
        let (anchor, cursor) = match self.active_tab() {
            Some(tab) => (tab.anchor, tab.cursor),
            None => return,
        };
        let new = if mods.control {
            if dir < 0 {
                prev_word(&self.active_tab().unwrap().buffer, cursor)
            } else {
                next_word(&self.active_tab().unwrap().buffer, cursor)
            }
        } else if dir < 0 {
            prev_char_boundary(&self.active_tab().unwrap().buffer, cursor)
        } else {
            next_char_boundary(&self.active_tab().unwrap().buffer, cursor)
        };
        let _ = (anchor,);
        let Some(tab) = self.active_tab_mut() else { return };
        if !mods.shift {
            tab.anchor = new;
        }
        tab.cursor = new;
        cx.notify();
    }

    fn move_vertical(&mut self, dir: i32, mods: &Modifiers, window: &Window, cx: &mut Context<Workspace>) {
        if self.active_tab().is_none() {
            return;
        }
        let bounds = self.editor_bounds;
        let geom = self.geometry(&bounds);
        let (cx_x, cy) = match self.caret_xy(&geom, window) {
            Some(v) => v,
            None => return,
        };
        let new_y = cy + dir as f32 * f32::from(LINE_HEIGHT);
        let char_idx = self.char_at_xy(&geom, cx_x, new_y, window);
        let Some(tab) = self.active_tab_mut() else { return };
        if !mods.shift {
            tab.anchor = char_idx;
        }
        tab.cursor = char_idx;
        cx.notify();
    }

    fn move_page(&mut self, dir: i32, mods: &Modifiers, window: &Window, cx: &mut Context<Workspace>) {
        if self.active_tab().is_none() {
            return;
        }
        let bounds = self.editor_bounds;
        let geom = self.geometry(&bounds);
        let (cx_x, cy) = match self.caret_xy(&geom, window) {
            Some(v) => v,
            None => return,
        };
        let new_y = cy + dir as f32 * geom.content_height * 0.9;
        let char_idx = self.char_at_xy(&geom, cx_x, new_y, window);
        let Some(tab) = self.active_tab_mut() else { return };
        if !mods.shift {
            tab.anchor = char_idx;
        }
        tab.cursor = char_idx;
        cx.notify();
    }

    fn cursor_line_start(&mut self, cx: &mut Context<Workspace>) {
        let Some(tab) = self.active_tab_mut() else { return };
        let line = tab.buffer.char_to_line(tab.cursor);
        tab.cursor = tab.buffer.line_to_char(line);
        tab.anchor = tab.cursor;
        cx.notify();
    }

    fn cursor_line_end(&mut self, cx: &mut Context<Workspace>) {
        let Some(tab) = self.active_tab_mut() else { return };
        let line = tab.buffer.char_to_line(tab.cursor);
        let line_end = if line + 1 < tab.buffer.line_count() {
            tab.buffer.line_to_char(line + 1)
        } else {
            tab.buffer.char_count()
        };
        tab.cursor = line_end;
        tab.anchor = tab.cursor;
        cx.notify();
    }

    fn on_editor_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        window.focus(&self.focus_handle);
        self.input_focus = InputFocus::None;
        let bounds = self.editor_bounds;
        let geom = self.geometry(&bounds);
        let local_x = f32::from(event.position.x) - f32::from(bounds.origin.x);
        let local_y = f32::from(event.position.y) - f32::from(bounds.origin.y);

        if local_x > f32::from(bounds.size.width) - 14.0 {
            self.drag_mode = DragMode::Scroll;
            self.scroll_to_mouse(local_y, &geom, window, cx);
            return;
        }

        let scroll = self.active_tab().map(|t| t.scroll).unwrap_or(0.0);
        let x = (local_x - geom.content_left).max(0.0);
        let y = local_y - geom.content_top + scroll;
        let char_idx = self.char_at_xy(&geom, x, y, window);

        let Some(tab) = self.active_tab_mut() else { return };
        if event.modifiers.shift {
            tab.cursor = char_idx;
        } else if event.click_count >= 2 {
            let (s, e) = word_bounds(&tab.buffer, char_idx);
            tab.anchor = s;
            tab.cursor = e;
        } else {
            tab.anchor = char_idx;
            tab.cursor = char_idx;
        }
        self.drag_mode = DragMode::Select;
        cx.notify();
    }

    fn on_editor_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        match self.drag_mode {
            DragMode::None => {}
            DragMode::Scroll => {
                let bounds = self.editor_bounds;
                let geom = self.geometry(&bounds);
                let local_y = f32::from(event.position.y) - f32::from(bounds.origin.y);
                self.scroll_to_mouse(local_y, &geom, _window, cx);
            }
            DragMode::Select => {
                let bounds = self.editor_bounds;
                let geom = self.geometry(&bounds);
                let local_x = f32::from(event.position.x) - f32::from(bounds.origin.x);
                let local_y = f32::from(event.position.y) - f32::from(bounds.origin.y);
                let scroll = self.active_tab().map(|t| t.scroll).unwrap_or(0.0);
                let x = (local_x - geom.content_left).max(0.0);
                let y = local_y - geom.content_top + scroll;
                let char_idx = self.char_at_xy(&geom, x, y, _window);
                let Some(tab) = self.active_tab_mut() else { return };
                tab.cursor = char_idx;
                cx.notify();
            }
        }
    }

    fn on_scroll_wheel(&mut self, event: &ScrollWheelEvent, window: &Window, cx: &mut Context<Workspace>) {
        let delta = match event.delta {
            ScrollDelta::Pixels(p) => f32::from(p.y),
            ScrollDelta::Lines(l) => l.y * f32::from(LINE_HEIGHT) * 3.0,
        };
        let bounds = self.editor_bounds;
        let geom = self.geometry(&bounds);
        let (_total, max_scroll) = self.content_metrics(&geom, window);
        let Some(tab) = self.active_tab_mut() else { return };
        tab.scroll = (tab.scroll - delta).max(0.0).min(max_scroll);
        cx.notify();
    }

    fn scroll_to_mouse(&mut self, local_y: f32, geom: &Geom, window: &Window, cx: &mut Context<Workspace>) {
        let (_total, max_scroll) = self.content_metrics(geom, window);
        let Some(tab) = self.active_tab_mut() else { return };
        if max_scroll <= 0.0 {
            return;
        }
        let track_top = geom.content_top;
        let track_h = geom.content_height;
        let frac = ((local_y - track_top) / track_h).clamp(0.0, 1.0);
        tab.scroll = frac * max_scroll;
        cx.notify();
    }
}

// ===== 自由函数：文本 / 索引辅助 =====

fn intersect(a0: usize, a1: usize, b0: usize, b1: usize) -> Option<(usize, usize)> {
    let s = a0.max(b0);
    let e = a1.min(b1);
    if s < e {
        Some((s, e))
    } else {
        None
    }
}

fn byte_range_in_line(line_text: &str, char_s: usize, char_e: usize) -> (usize, usize) {
    let mut start = line_text.len();
    let mut end = line_text.len();
    for (ci, (b, _)) in line_text.char_indices().enumerate() {
        if ci == char_s {
            start = b;
        }
        if ci == char_e {
            end = b;
            break;
        }
    }
    (start, end)
}

fn prev_char_boundary(_buf: &Buffer, idx: usize) -> usize {
    idx.saturating_sub(1)
}

fn next_char_boundary(buf: &Buffer, idx: usize) -> usize {
    (idx + 1).min(buf.char_count())
}

fn prev_word(buf: &Buffer, idx: usize) -> usize {
    let text: String = buf.to_string();
    let chars: Vec<char> = text.chars().collect();
    if idx == 0 {
        return 0;
    }
    let mut i = idx.min(chars.len());
    while i > 0 && chars[i - 1].is_whitespace() {
        i -= 1;
    }
    while i > 0 && !chars[i - 1].is_whitespace() {
        i -= 1;
    }
    i
}

fn next_word(buf: &Buffer, idx: usize) -> usize {
    let text: String = buf.to_string();
    let chars: Vec<char> = text.chars().collect();
    let mut i = idx;
    while i < chars.len() && !chars[i].is_whitespace() {
        i += 1;
    }
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

fn word_bounds(buf: &Buffer, idx: usize) -> (usize, usize) {
    let text: String = buf.to_string();
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return (0, 0);
    }
    let i = idx.min(chars.len());
    let mut s = i;
    let mut e = i;
    while s > 0 && !chars[s - 1].is_whitespace() {
        s -= 1;
    }
    while e < chars.len() && !chars[e].is_whitespace() {
        e += 1;
    }
    (s, e)
}

fn mk_run(len: usize, color: Hsla) -> TextRun {
    TextRun {
        len,
        font: font(EDITOR_FONT),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

fn mk_run_bg(len: usize, color: Hsla, bg: Hsla) -> TextRun {
    TextRun {
        len,
        font: font(EDITOR_FONT),
        color,
        background_color: Some(bg),
        underline: None,
        strikethrough: None,
    }
}

fn edit_text_field(field: &mut String, event: &KeyDownEvent) {
    let key = event.keystroke.key.clone();
    let mods = &event.keystroke.modifiers;
    let ctrl = mods.control || mods.platform;
    match key.as_str() {
        "Backspace" => {
            field.pop();
        }
        "Enter" => {}
        "Escape" => {}
        _ => {
            let ch = if key.is_empty() {
                event.keystroke.key_char.clone().unwrap_or_default()
            } else {
                key
            };
            if !ch.is_empty() && !ctrl && !mods.alt {
                field.push_str(&ch);
            }
        }
    }
}

// ===== UI 组件（返回元素的自由函数） =====

fn title_bar(active: Option<&Tab>) -> Div {
    let title = match active {
        Some(t) => {
            let dirty = if t.is_dirty() { "● " } else { "" };
            if let Some(p) = &t.path {
                format!("{}{} — {}", dirty, t.name, p)
            } else {
                format!("{}{}", dirty, t.name)
            }
        }
        None => "Lumen 文本编辑器".to_string(),
    };
    div()
        .h(px(36.0))
        .bg(BG_PANEL)
        .border_b_1()
        .border_color(BORDER)
        .flex()
        .items_center()
        .px(px(12.0))
        .text_color(INK_DIM)
        .child(div().size(px(15.0)).rounded(px(4.0)).bg(ACCENT))
        .child(div().text_color(INK).font_weight(FontWeight::BOLD).px(px(6.0)).child("Lumen"))
        .child(div().px(px(6.0)).child("·"))
        .child(div().child(title))
}

fn toolbar(sidebar_visible: bool, has_active: bool, entity: &Entity<Workspace>) -> Div {
    let entity_c = entity.clone();
    let entity_o = entity.clone();
    let entity_s = entity.clone();
    let entity_f = entity.clone();
    let entity_d = entity.clone();
    let entity_n = entity.clone();
    div()
        .h(px(44.0))
        .bg(BG_APP)
        .border_b_1()
        .border_color(BORDER)
        .flex()
        .items_center()
        .px(px(10.0))
        .gap(px(4.0))
        .child(tool_button("新建", entity_c, |ws, cx| ws.open_new_file_dialog(cx)))
        .child(tool_button("打开", entity_o, |ws, cx| ws.open_open_dialog(cx)))
        .child(tool_button("保存", entity_s, move |ws, cx| {
            if has_active {
                ws.save_active(cx);
            }
        }))
        .child(div().w(px(1.0)).h(px(20.0)).bg(BORDER).mx(px(6.0)))
        .child(tool_button("查找/替换", entity_f, |ws, cx| ws.open_search(false, cx)))
        .child(div().w(px(1.0)).h(px(20.0)).bg(BORDER).mx(px(6.0)))
        .child(tool_button("设为默认", entity_d, |ws, cx| ws.set_default_for_active(cx)))
        .child(div().flex_1())
        .child(
            tool_button("侧边栏", entity_n, move |ws, cx| {
                ws.sidebar_visible = !ws.sidebar_visible;
                cx.notify();
            })
            .when(sidebar_visible, |b| b.text_color(ACCENT)),
        )
}

fn tool_button(
    label: &str,
    entity: Entity<Workspace>,
    action: impl Fn(&mut Workspace, &mut Context<Workspace>) + 'static,
) -> Div {
    let label = label.to_string();
    div()
        .h(px(30.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .rounded(px(4.0))
        .text_color(INK_DIM)
        .cursor(CursorStyle::PointingHand)
        .hover(|s| s.bg(BG_HOVER).text_color(INK))
        .border(px(1.0))
        .border_color(transparent())
        .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
            entity.update(cx, |ws, cx| action(ws, cx));
        })
        .child(label)
}

fn sidebar(tabs: &[(usize, String, bool, bool)], active_idx: Option<usize>, entity: &Entity<Workspace>) -> Div {
    let entity = entity.clone();
    let mut list = div().flex_1().p(px(6.0));
    for (idx, name, dirty, saved) in tabs {
        let idx = *idx;
        let name = name.clone();
        let active = active_idx == Some(idx);
        let entity_c = entity.clone();
        let entity_x = entity.clone();
        let icon = if *saved { "📄" } else { "📝" };
        list = list.child(
            div()
                .px(px(8.0))
                .py(px(7.0))
                .rounded(px(4.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .cursor(CursorStyle::PointingHand)
                .when(active, |d| d.bg(BG_ACTIVE).text_color(INK))
                .when(!active, |d| d.text_color(INK_DIM).hover(|s| s.bg(BG_HOVER).text_color(INK)))
                .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
                    entity_c.update(cx, |ws, _cx| ws.set_active(idx));
                })
                .child(div().text_color(INK_FAINT).child(icon))
                .child(div().flex_1().truncate().child(name.clone()))
                .when(*dirty, |d| d.child(div().size(px(6.0)).rounded(px(3.0)).bg(WARN)))
                .child(
                    div()
                        .w(px(16.0))
                        .h(px(16.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(3.0))
                        .text_color(INK_FAINT)
                        .hover(|s| s.bg(DANGER).text_color(white()))
                        .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
                            entity_x.update(cx, |ws, cx| ws.close_tab(idx, cx));
                        })
                        .child("✕"),
                ),
        );
    }
    div()
        .w(px(200.0))
        .flex_shrink_0()
        .bg(BG_PANEL)
        .border_r_1()
        .border_color(BORDER)
        .flex_col()
        .child(
            div()
                .px(px(12.0))
                .py(px(10.0))
                .text_color(INK_FAINT)
                .text_size(px(10.5))
                
                .child("打开的文件"),
        )
        .child(list)
        .child(
            div()
                .p(px(8.0))
                .border_t_1()
                .border_color(BORDER)
                .child(
                    div()
                        .w_full()
                        .p(px(8.0))
                        .border(px(1.0))
                        .border_color(BORDER_STRONG)
                        .rounded(px(4.0))
                        .text_color(INK_DIM)
                        .cursor(CursorStyle::PointingHand)
                        .hover(|s| s.border_color(ACCENT).text_color(ACCENT))
                        .flex()
                        .items_center()
                        .justify_center()
                        .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
                            entity.clone().update(cx, |ws, cx| ws.open_new_file_dialog(cx));
                        })
                        .child("+ 新建文件"),
                ),
        )
}

fn status_bar(status: Option<(String, String, usize, usize, String)>) -> Div {
    match status {
        Some((enc, eol, chars, lines, path)) => div()
            .h(px(26.0))
            .bg(ACCENT)
            .text_color(white())
            .flex()
            .items_center()
            .px(px(14.0))
            .gap(px(18.0))
            .child(div().child(enc))
            .child(div().child(eol))
            .child(div().child(format!("{} 字符 · {} 行", chars, lines)))
            .child(div().flex_1())
            .child(div().child(path)),
        None => div()
            .h(px(26.0))
            .bg(ACCENT)
            .text_color(white())
            .flex()
            .items_center()
            .px(px(14.0))
            .child("没有打开的文件"),
    }
}

fn field_label(text: &str) -> Div {
    div()
        .text_size(px(11.5))
        .text_color(INK_DIM)
        .child(text.to_string())
}

fn checkbox(
    entity: Entity<Workspace>,
    label: &str,
    checked: bool,
    on_toggle: impl Fn(&mut Workspace, &mut Context<Workspace>) + 'static,
) -> Div {
    let label = label.to_string();
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(12.0))
        .cursor(CursorStyle::PointingHand)
        .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
            entity.update(cx, |ws, cx| on_toggle(ws, cx));
        })
        .child(
            div()
                .size(px(14.0))
                .border(px(1.0))
                .border_color(BORDER_STRONG)
                .rounded(px(3.0))
                .when(checked, |d| d.bg(ACCENT)),
        )
        .child(label)
}

fn radio(
    entity: Entity<Workspace>,
    label: &str,
    value: SearchMode,
    current: SearchMode,
    on_select: impl Fn(&mut Workspace, &mut Context<Workspace>) + 'static,
) -> Div {
    let label = label.to_string();
    let checked = value == current;
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .text_size(px(12.0))
        .cursor(CursorStyle::PointingHand)
        .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
            entity.update(cx, |ws, cx| on_select(ws, cx));
        })
        .child(
            div()
                .size(px(14.0))
                .border(px(1.0))
                .border_color(BORDER_STRONG)
                .rounded(px(7.0))
                .when(checked, |d| d.bg(ACCENT)),
        )
        .child(label)
}

fn search_tab(entity: &Entity<Workspace>, replace: bool, label: &str, active_replace: bool) -> Div {
    let label = label.to_string();
    let e = entity.clone();
    div()
        .px(px(12.0))
        .py(px(5.0))
        .rounded(px(4.0))
        .text_size(px(12.5))
        .text_color(if replace == active_replace { ACCENT } else { INK_DIM })
        .cursor(CursorStyle::PointingHand)
        .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
            e.update(cx, |ws, cx| {
                ws.search.mode_replace = replace;
                ws.search.visible = true;
                cx.notify();
            })
            ;
        })
        .child(label)
}

fn sp_button(
    entity: Entity<Workspace>,
    label: &str,
    action: impl Fn(&mut Workspace, &mut Context<Workspace>) + 'static,
) -> Div {
    let label = label.to_string();
    div()
        .px(px(10.0))
        .py(px(5.0))
        .rounded(px(4.0))
        .text_size(px(12.0))
        .bg(BG_HOVER)
        .text_color(INK)
        .cursor(CursorStyle::PointingHand)
        .hover(|s| s.bg(BORDER))
        .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
            entity.update(cx, |ws, cx| action(ws, cx));
        })
        .child(label)
}

fn sp_button_primary(
    entity: Entity<Workspace>,
    label: &str,
    action: impl Fn(&mut Workspace, &mut Context<Workspace>) + 'static,
) -> Div {
    let label = label.to_string();
    div()
        .px(px(10.0))
        .py(px(5.0))
        .rounded(px(4.0))
        .text_size(px(12.0))
        .bg(ACCENT)
        .text_color(white())
        .cursor(CursorStyle::PointingHand)
        .hover(|s| s.bg(ACCENT_HOVER))
        .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
            entity.update(cx, |ws, cx| action(ws, cx));
        })
        .child(label)
}

fn search_panel(ws: &Workspace, entity: &Entity<Workspace>) -> Div {
    let e_find = entity.clone();
    let e_repl = entity.clone();
    let e_prev = entity.clone();
    let e_next = entity.clone();
    let e_count = entity.clone();
    let e_close_btn = entity.clone();
    let e_close = entity.clone();
    let e_mode_n = entity.clone();
    let e_mode_x = entity.clone();
    let e_mode_r = entity.clone();
    let e_rep = entity.clone();
    let e_all = entity.clone();
    let replace_mode = ws.search.mode_replace;

    div()
        .absolute()
        .top(px(10.0))
        .right(px(10.0))
        .w(px(330.0))
        .bg(BG_APP)
        .border(px(1.0))
        .border_color(BORDER_STRONG)
        .rounded(px(8.0))
        .shadow_lg()
        .p(px(14.0))
        .text_color(INK)
        .child(
            div()
                .flex()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .gap(px(4.0))
                        .child(search_tab(&e_find, false, "查找", replace_mode))
                        .child(search_tab(&e_find, true, "替换", replace_mode)),
                )
                .child(
                    div()
                        .w(px(22.0))
                        .h(px(22.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.0))
                        .text_color(INK_FAINT)
                        .cursor(CursorStyle::PointingHand)
                        .hover(|s| s.bg(BG_HOVER).text_color(INK))
                        .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
                            e_close_btn.update(cx, |ws, cx| ws.close_search(cx));
                        })
                        .child("✕"),
                ),
        )
        .child(
            div()
                .mt(px(12.0))
                .child(field_label("查找目标："))
                .child(
                    div()
                        .mt(px(4.0))
                        .border(px(1.0))
                        .border_color(BORDER_STRONG)
                        .rounded(px(4.0))
                        .p(px(7.0))
                        .text_color(INK)
                        .cursor(CursorStyle::IBeam)
                        .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
                            e_find
                                .update(cx, |ws, _cx| {
                                    ws.input_focus = InputFocus::SearchFind;
                                })
                                ;
                        })
                        .child(if ws.search.pattern.is_empty() {
                            "输入要查找的内容…".to_string()
                        } else {
                            ws.search.pattern.clone()
                        }),
                ),
        )
        .when(replace_mode, |area| {
            area.child(
                div()
                    .mt(px(10.0))
                    .child(field_label("替换为："))
                    .child(
                        div()
                            .mt(px(4.0))
                            .border(px(1.0))
                            .border_color(BORDER_STRONG)
                            .rounded(px(4.0))
                            .p(px(7.0))
                            .text_color(INK)
                            .cursor(CursorStyle::IBeam)
                            .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
                                e_repl
                                    .update(cx, |ws, _cx| {
                                        ws.input_focus = InputFocus::SearchReplace;
                                    })
                                    ;
                            })
                            .child(if ws.search.replace.is_empty() {
                                "输入替换内容…".to_string()
                            } else {
                                ws.search.replace.clone()
                            }),
                    ),
            )
        })
        .child(
            div()
                .mt(px(10.0))
                .flex_col()
                .gap(px(6.0))
                .child(checkbox(
                    e_prev.clone(),
                    "匹配大小写",
                    ws.search.match_case,
                    |ws, cx| {
                        ws.search.match_case = !ws.search.match_case;
                        ws.sync_search();
                        cx.notify();
                    },
                ))
                .child(checkbox(
                    e_next.clone(),
                    "全词匹配",
                    ws.search.whole_word,
                    |ws, cx| {
                        ws.search.whole_word = !ws.search.whole_word;
                        ws.sync_search();
                        cx.notify();
                    },
                ))
                .child(checkbox(
                    e_count.clone(),
                    "反向查找",
                    ws.search.backward,
                    |ws, _cx| {
                        ws.search.backward = !ws.search.backward;
                    },
                ))
                .child(checkbox(
                    e_close.clone(),
                    "循环查找",
                    ws.search.wrap,
                    |ws, _cx| {
                        ws.search.wrap = !ws.search.wrap;
                    },
                )),
        )
        .child(
            div()
                .mt(px(10.0))
                .flex_col()
                .gap(px(6.0))
                .child(div().text_size(px(10.5)).text_color(INK_FAINT).child("查找模式"))
                .child(radio(
                    e_mode_n,
                    "普通",
                    SearchMode::Normal,
                    ws.search.search_mode,
                    |ws, cx| {
                        ws.search.search_mode = SearchMode::Normal;
                        ws.sync_search();
                        cx.notify();
                    },
                ))
                .child(radio(
                    e_mode_x,
                    "扩展 (\\n \\r \\t \\0 \\x…)",
                    SearchMode::Extended,
                    ws.search.search_mode,
                    |ws, cx| {
                        ws.search.search_mode = SearchMode::Extended;
                        ws.sync_search();
                        cx.notify();
                    },
                ))
                .child(radio(
                    e_mode_r,
                    "正则表达式",
                    SearchMode::Regex,
                    ws.search.search_mode,
                    |ws, cx| {
                        ws.search.search_mode = SearchMode::Regex;
                        ws.sync_search();
                        cx.notify();
                    },
                )),
        )
        .child(
            div()
                .mt(px(10.0))
                .text_size(px(11.0))
                .text_color(INK_DIM)
                .child(format!("共 {} 处匹配", ws.search.matches.len())),
        )
        .child(
            div()
                .mt(px(8.0))
                .flex()
                .gap(px(6.0))
                .child(sp_button(e_prev, "↑ 查找上一个", |ws, cx| ws.find_prev(cx)))
                .child(sp_button(e_next, "↓ 查找下一个", |ws, cx| ws.find_next(cx)))
                .child(sp_button(e_count, "计数", |ws, cx| ws.count_matches(cx))),
        )
        .when(replace_mode, |area| {
            area.child(
                div()
                    .mt(px(8.0))
                    .flex()
                    .gap(px(6.0))
                    .child(sp_button(e_rep, "替换", |ws, cx| ws.replace_current(cx)))
                    .child(sp_button_primary(
                        e_all,
                        "全部替换",
                        |ws, cx| ws.replace_all_matches(cx),
                    )),
            )
        })
}

fn new_file_dialog(nf: &NewFileDialog, entity: &Entity<Workspace>) -> Div {
    let e_name = entity.clone();
    let e_ok = entity.clone();
    let e_cancel = entity.clone();
    let e_enc: Vec<Entity<Workspace>> = ENCODING_OPTIONS.iter().map(|_| entity.clone()).collect();

    let e_bg = e_cancel.clone();
    let overlay = div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .bg(hsla(0.0, 0.0, 0.0, 0.35))
        .flex()
        .items_center()
        .justify_center()
        .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
            e_bg.update(cx, |ws, _cx| ws.new_file = None);
        });

    let mut encoding_rows = div().flex_col().gap(px(4.0));
    for (idx, (code, desc)) in ENCODING_OPTIONS.iter().enumerate() {
        let selected = idx == nf.encoding;
        let e = e_enc[idx].clone();
        encoding_rows = encoding_rows.child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(6.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .cursor(CursorStyle::PointingHand)
                .when(selected, |d| d.bg(BG_ACTIVE))
                .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
                    e.update(cx, |ws, cx| {
                        if let Some(d) = ws.new_file.as_mut() {
                            d.encoding = idx;
                        }
                        cx.notify();
                    })
                    ;
                })
                .child(
                    div()
                        .size(px(12.0))
                        .rounded(px(6.0))
                        .border(px(1.0))
                        .border_color(BORDER_STRONG)
                        .when(selected, |d| d.bg(ACCENT)),
                )
                .child(
                    div()
                        .flex_col()
                        .child(div().text_size(px(12.0)).text_color(INK).child(*code))
                        .child(div().text_size(px(10.5)).text_color(INK_FAINT).child(*desc)),
                ),
        );
    }

    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .flex()
        .items_center()
        .justify_center()
        .child(overlay)
        .child(
            div()
                .w(px(420.0))
                .bg(BG_APP)
                .border(px(1.0))
                .border_color(BORDER_STRONG)
                .rounded(px(8.0))
                .shadow_lg()
                .p(px(16.0))
                .text_color(INK)
                .on_mouse_down(MouseButton::Left, |_e, _w, _cx| {})
                .child(div().text_size(px(14.0)).font_weight(FontWeight::BOLD).child("新建文件"))
                .child(
                    div()
                        .mt(px(12.0))
                        .child(field_label("文件名："))
                        .child(
                            div()
                                .mt(px(4.0))
                                .border(px(1.0))
                                .border_color(BORDER_STRONG)
                                .rounded(px(4.0))
                                .p(px(7.0))
                                .text_color(INK)
                                .cursor(CursorStyle::IBeam)
                                .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
                                    e_name.update(cx, |ws, _cx| ws.input_focus = InputFocus::NewFileName);
                                })
                                .child(if nf.name.is_empty() {
                                    "输入文件名…".to_string()
                                } else {
                                    nf.name.clone()
                                }),
                        ),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .child(field_label("编码："))
                        .child(div().mt(px(4.0)).max_h(px(160.0)).child(encoding_rows)),
                )
                .when(!nf.error.is_empty(), |d| {
                    d.child(div().mt(px(8.0)).text_color(DANGER).text_size(px(11.5)).child(nf.error.clone()))
                })
                .child(
                    div()
                        .mt(px(14.0))
                        .flex()
                        .justify_end()
                        .gap(px(8.0))
                        .child(sp_button(e_cancel, "取消", |ws, cx| {
                            ws.new_file = None;
                            cx.notify();
                        }))
                        .child(sp_button_primary(e_ok, "确定", |ws, cx| ws.confirm_new_file(cx))),
                ),
        )
}

fn file_dialog_view(dlg: &FileDialog, entity: &Entity<Workspace>) -> Div {
    let e_name = entity.clone();
    let e_ok = entity.clone();
    let e_cancel = entity.clone();
    let e_up = entity.clone();
    let e_entries: Vec<Entity<Workspace>> = dlg.entries.iter().map(|_| entity.clone()).collect();


    let mut rows = div().flex_col().gap(px(2.0));
    // 上级目录
    let eu = e_up.clone();
    rows = rows.child(
        div()
            .px(px(8.0))
            .py(px(6.0))
            .rounded(px(4.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .cursor(CursorStyle::PointingHand)
            .hover(|s| s.bg(BG_HOVER))
            .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
                eu.update(cx, |ws, cx| ws.navigate_file_dialog("..".to_string(), cx));
            })
            .child(div().text_color(INK_FAINT).child("📁"))
            .child(div().text_color(INK_DIM).child("..")),
    );
    for (i, entry) in dlg.entries.iter().enumerate() {
        let e = e_entries[i].clone();
        let name = entry.name.clone();
        let nm = name.clone();
        let is_dir = entry.is_dir;
        rows = rows.child(
            div()
                .px(px(8.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .cursor(CursorStyle::PointingHand)
                .hover(|s| s.bg(BG_HOVER))
                .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
                    e.update(cx, |ws, cx| ws.navigate_file_dialog(nm.clone(), cx));
                })
                .child(div().text_color(INK_FAINT).child(if is_dir { "📁" } else { "📄" }))
                .child(div().text_color(INK).child(name)),
        );
    }

    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .w(px(480.0))
                .h(px(420.0))
                .bg(BG_APP)
                .border(px(1.0))
                .border_color(BORDER_STRONG)
                .rounded(px(8.0))
                .shadow_lg()
                .flex_col()
                .p(px(16.0))
                .text_color(INK)
                .on_mouse_down(MouseButton::Left, |_e, _w, _cx| {})
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(FontWeight::BOLD)
                        .child(if dlg.save { "另存为" } else { "打开文件" }),
                )
                .child(div().mt(px(6.0)).text_color(INK_DIM).text_size(px(11.0)).child(dlg.dir.clone()))
                .child(
                    div()
                        .mt(px(8.0))
                        .flex_1()
                        .border(px(1.0))
                        .border_color(BORDER)
                        .rounded(px(4.0))
                        
                        .p(px(4.0))
                        .child(rows),
                )
                .child(
                    div()
                        .mt(px(8.0))
                        .child(field_label("文件名："))
                        .child(
                            div()
                                .mt(px(4.0))
                                .border(px(1.0))
                                .border_color(BORDER_STRONG)
                                .rounded(px(4.0))
                                .p(px(7.0))
                                .text_color(INK)
                                .cursor(CursorStyle::IBeam)
                                .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
                                    e_name.update(cx, |ws, _cx| ws.input_focus = InputFocus::SaveFileName);
                                })
                                .child(if dlg.filename.is_empty() {
                                    "输入文件名…".to_string()
                                } else {
                                    dlg.filename.clone()
                                }),
                        ),
                )
                .when(!dlg.error.is_empty(), |d| {
                    d.child(div().mt(px(6.0)).text_color(DANGER).text_size(px(11.5)).child(dlg.error.clone()))
                })
                .child(
                    div()
                        .mt(px(12.0))
                        .flex()
                        .justify_end()
                        .gap(px(8.0))
                        .child(sp_button(e_cancel, "取消", |ws, cx| {
                            ws.file_dialog = None;
                            cx.notify();
                        }))
                        .child(sp_button_primary(e_ok, if dlg.save { "保存" } else { "打开" }, |ws, cx| {
                            ws.confirm_file_dialog(cx);
                        })),
                ),
        )
}

fn toast_view(msg: &str, kind: ToastKind) -> Div {
    let bg = match kind {
        ToastKind::Info => INK_DIM,
        ToastKind::Success => SUCCESS,
        ToastKind::Error => DANGER,
    };
    div()
        .absolute()
        .bottom(px(40.0))
        .left(px(0.0))
        .right(px(0.0))
        .flex()
        .justify_center()
        .child(
            div()
                .bg(bg)
                .text_color(white())
                .rounded(px(6.0))
                .px(px(14.0))
                .py(px(8.0))
                .text_size(px(12.5))
                .shadow_lg()
                .child(msg.to_string()),
        )
}

// ===== 平台 / 路径辅助 =====

impl FileDialog {
    fn reload(&mut self) {
        self.entries.clear();
        if let Ok(rd) = std::fs::read_dir(&self.dir) {
            let mut dirs: Vec<DirEntry> = Vec::new();
            let mut files: Vec<DirEntry> = Vec::new();
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.path().is_dir();
                if is_dir {
                    dirs.push(DirEntry { name, is_dir });
                } else {
                    files.push(DirEntry { name, is_dir });
                }
            }
            dirs.sort_by(|a, b| a.name.cmp(&b.name));
            files.sort_by(|a, b| a.name.cmp(&b.name));
            self.entries.extend(dirs);
            self.entries.extend(files);
        }
    }
}

fn basename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn extension_of(name: &str) -> Option<String> {
    Path::new(name)
        .extension()
        .map(|s| s.to_string_lossy().to_lowercase().to_string())
}

fn home_dir() -> String {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string())
}

fn transparent() -> Hsla {
    hsla(0.0, 0.0, 0.0, 0.0)
}

fn white() -> Hsla {
    rgb(0xffffff).into()
}
