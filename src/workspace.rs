//! Lumen 的主界面与全部交互逻辑（GPUI 重写版）。
//!
//! 整体结构：单一根实体 `Workspace` 持有所有标签页与 UI 状态，负责绘制标题栏 /
//! 工具栏 / 侧边栏 / 编辑器 / 状态栏 / Toast / 对话框，并统一处理键鼠输入。
//!
//! 编辑器区域是一个 `canvas` 元素，绘制完全由我们控制：行号槽、软换行
//! （基于 `window.text_system().shape_text` 得到的 `WrappedLine`）、选区与查找匹配
//! 高亮（通过 `TextRun` 的 `background_color`）、光标、命中测试都自行实现，以对齐
//! 原 CodeMirror 版观感。
//!
//! 鼠标选择：按下由编辑区 div 的 on_mouse_down 处理；拖动与松开在 canvas 的
//! paint 阶段用 `window.on_mouse_event` 注册为窗口级监听，鼠标移出编辑区也能
//! 继续拖选，并在任意位置松开时结束。所有弹层（新建对话框、查找面板、右键菜单）
//! 都 `occlude()`，点击不会穿透到下层编辑区或遮罩。
//!
//! 打开 / 另存为使用系统原生文件对话框（`prompt_for_paths` / `prompt_for_new_path`）。
//!
//! 语法高亮见 `highlight.rs`：`line_runs` 把语法着色（前景）与选区 / 查找匹配（背景）
//! 合并成 TextRun；只着色可见行，高亮状态按行增量缓存在每个标签的 `syntax` 里。

use gpui::prelude::FluentBuilder;
use gpui::*;
use std::ops::Range;
use std::path::Path;
use std::time::Duration;

use crate::buffer::Buffer;
use crate::file_assoc;
use crate::file_ops::read_text_file;
use crate::file_ops::write_text_file;
use crate::highlight::{Span, SyntaxCache};
use std::cell::RefCell;
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
    /// 语法高亮缓存。绘制函数只拿到 `&self`，所以用 RefCell 做内部可变。
    pub syntax: RefCell<SyntaxCache>,
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
            syntax: RefCell::new(SyntaxCache::new()),
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

    input_focus: InputFocus,
    /// 当前获得焦点的输入框（文件名 / 查找 / 替换）里的光标与选区锚点（字符索引）。
    field_cursor: usize,
    field_anchor: usize,
    /// 各输入框文字区域在窗口中的位置，用于鼠标点选定位和摆放输入法候选窗。
    field_rects: [Bounds<Pixels>; 3],
    drag_mode: DragMode,

    /// 输入法正在组合的文本区间（字符索引；编辑器时指缓冲区，输入框时指该输入框字符串）。
    marked: Option<Range<usize>>,

    /// 编辑区右键菜单的弹出位置（窗口坐标）；None 表示未显示。
    context_menu: Option<Point<Pixels>>,

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
                input_focus: InputFocus::None,
                field_cursor: 0,
                field_anchor: 0,
                field_rects: [Bounds::default(); 3],
                drag_mode: DragMode::None,
                marked: None,
                context_menu: None,
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

    // ===== 文件对话框（系统原生）=====
    //
    // 旧版自绘了一个目录浏览弹窗：列表不能滚动、内容溢出卡片、点选还会穿透到
    // 编辑区。这里改用 GPUI 封装的系统对话框（Windows 上是 IFileOpenDialog /
    // IFileSaveDialog），外观与资源管理器一致。对话框是异步的：结果通过 oneshot
    // 通道回来，再回到主线程更新工作区。

    fn open_open_dialog(&mut self, cx: &mut Context<Workspace>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("打开".into()),
        });
        cx.spawn(async move |this, cx| {
            // 外层 Err = 通道被丢弃；内层 Err = 系统对话框打不开；Ok(None) = 用户取消。
            let result = match rx.await {
                Ok(r) => r,
                Err(_) => return,
            };
            let Some(this) = this.upgrade() else { return };
            this.update(cx, |ws, cx| match result {
                Ok(Some(paths)) => {
                    for p in paths {
                        ws.open_file_by_path(&p.to_string_lossy(), cx);
                    }
                    cx.notify();
                }
                Ok(None) => {}
                Err(e) => ws.show_toast(format!("无法打开文件对话框：{}", e), ToastKind::Error, cx),
            })
            .ok();
        })
        .detach();
    }

    fn open_save_dialog(&mut self, cx: &mut Context<Workspace>) {
        let Some(tab) = self.active_tab() else { return };
        let tab_id = tab.id;
        let suggested = tab.name.clone();
        // 已保存过的文件从它所在目录开始，否则从用户目录开始。
        let dir = tab
            .path
            .as_deref()
            .and_then(|p| Path::new(p).parent())
            .map(|d| d.to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from(home_dir()));
        let rx = cx.prompt_for_new_path(&dir, Some(&suggested));
        cx.spawn(async move |this, cx| {
            let result = match rx.await {
                Ok(r) => r,
                Err(_) => return,
            };
            let Some(this) = this.upgrade() else { return };
            this.update(cx, |ws, cx| match result {
                Ok(Some(path)) => ws.save_tab_to(tab_id, &path.to_string_lossy(), cx),
                Ok(None) => {}
                Err(e) => ws.show_toast(format!("无法打开保存对话框：{}", e), ToastKind::Error, cx),
            })
            .ok();
        })
        .detach();
    }

    /// 另存为：按标签 id 定位（对话框打开期间用户可能切换了标签），
    /// 写盘成功后更新路径与标签名。
    fn save_tab_to(&mut self, tab_id: usize, target: &str, cx: &mut Context<Workspace>) {
        let Some(idx) = self.tabs.iter().position(|t| t.id == tab_id) else {
            self.show_toast("要保存的标签已关闭".to_string(), ToastKind::Info, cx);
            return;
        };
        let (content, eol, enc) = {
            let t = &self.tabs[idx];
            (t.buffer.to_string(), t.eol.clone(), t.encoding.clone())
        };
        match write_text_file(target, &content, &eol, Some(&enc)) {
            Ok(res) => {
                let name = basename(target);
                let tab = &mut self.tabs[idx];
                tab.path = Some(target.to_string());
                tab.name = name.clone();
                tab.buffer.mark_saved();
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
            Err(e) => self.show_toast(format!("保存失败：{}", e), ToastKind::Error, cx),
        }
    }

    // ===== 新建文件对话框 =====

    fn open_new_file_dialog(&mut self, cx: &mut Context<Workspace>) {
        let n = self.untitled;
        let name = format!("未命名-{}.txt", n);
        // 像资源管理器重命名一样：默认选中扩展名之前的部分，直接打字就能替换。
        let base_len = match name.rfind('.') {
            Some(i) if i > 0 => name[..i].chars().count(),
            _ => name.chars().count(),
        };
        self.new_file = Some(NewFileDialog {
            name,
            encoding: 0,
            error: String::new(),
        });
        self.focus_field(InputFocus::NewFileName);
        self.field_anchor = 0;
        self.field_cursor = base_len;
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
        self.focus_field(InputFocus::SearchFind);
        // 再次打开查找时全选上次的内容，方便直接输入新关键字
        self.field_anchor = 0;
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

    /// 为某一逻辑行构造 `TextRun` 序列：语法着色决定前景色，选区 / 查找匹配
    /// 决定背景色，输入法组合文本加下划线。三者可以任意重叠，这里按所有区间的
    /// 端点切段，每段分别取前景、背景和下划线。
    fn line_runs(&self, tab: &Tab, line_idx: usize, line_text: &str, syntax: Option<&[Span]>) -> Vec<TextRun> {
        let len = line_text.len();
        let n = tab.buffer.line_count();
        let lcs = tab.buffer.line_to_char(line_idx);
        let lce = if line_idx + 1 < n {
            tab.buffer.line_to_char(line_idx + 1)
        } else {
            tab.buffer.char_count()
        };

        // 背景区间，越靠后优先级越高：普通匹配 < 选区 < 当前匹配
        let mut bgs: Vec<(usize, usize, Hsla)> = Vec::new();
        for (i, &(ms, me)) in self.search.matches.iter().enumerate() {
            if Some(i) == self.search.active {
                continue;
            }
            if let Some(seg) = intersect(lcs, lce, ms, me) {
                let (bs, be) = byte_range_in_line(line_text, seg.0 - lcs, seg.1 - lcs);
                bgs.push((bs, be, MATCH_HL));
            }
        }
        let (s, e) = (tab.anchor.min(tab.cursor), tab.anchor.max(tab.cursor));
        if s < e {
            if let Some(seg) = intersect(lcs, lce, s, e) {
                let (bs, be) = byte_range_in_line(line_text, seg.0 - lcs, seg.1 - lcs);
                bgs.push((bs, be, SELECTION));
            }
        }
        if let Some(&(ms, me)) = self.search.active.and_then(|i| self.search.matches.get(i)) {
            if let Some(seg) = intersect(lcs, lce, ms, me) {
                let (bs, be) = byte_range_in_line(line_text, seg.0 - lcs, seg.1 - lcs);
                bgs.push((bs, be, MATCH_HL_SEL));
            }
        }

        // 输入法组合中的文本加下划线，和系统输入框的观感一致。
        let mut underline: Option<(usize, usize)> = None;
        if self.input_focus == InputFocus::None {
            if let Some(m) = &self.marked {
                if let Some(seg) = intersect(lcs, lce, m.start, m.end) {
                    underline = Some(byte_range_in_line(line_text, seg.0 - lcs, seg.1 - lcs));
                }
            }
        }

        let spans: &[Span] = syntax.unwrap_or(&[]);

        // 所有端点
        let mut cuts: Vec<usize> = vec![0, len];
        for &(a, b, _) in spans {
            cuts.push(a.min(len));
            cuts.push(b.min(len));
        }
        for &(a, b, _) in &bgs {
            cuts.push(a);
            cuts.push(b);
        }
        if let Some((a, b)) = underline {
            cuts.push(a);
            cuts.push(b);
        }
        cuts.sort_unstable();
        cuts.dedup();

        let mut runs: Vec<TextRun> = Vec::new();
        let mut si = 0usize; // spans 有序且不重叠，用游标顺序扫描
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a >= b {
                continue;
            }
            while si < spans.len() && spans[si].1 <= a {
                si += 1;
            }
            let fg = match spans.get(si) {
                Some(&(ss, se, c)) if ss <= a && a < se => c,
                _ => INK,
            };
            let bg = bgs
                .iter()
                .rev()
                .find(|&&(bs, be, _)| bs <= a && a < be)
                .map(|&(_, _, c)| c);
            let ul = underline.is_some_and(|(us, ue)| us <= a && a < ue);

            // 与上一段样式完全相同就合并，减少 TextRun 数量
            if let Some(last) = runs.last_mut() {
                if last.color == fg && last.background_color == bg && last.underline.is_some() == ul {
                    last.len += b - a;
                    continue;
                }
            }
            let mut run = mk_run(b - a, fg);
            run.background_color = bg;
            if ul {
                run.underline = Some(UnderlineStyle {
                    thickness: px(1.0),
                    color: Some(fg),
                    wavy: false,
                });
            }
            runs.push(run);
        }
        if runs.is_empty() {
            runs.push(mk_run(len, INK));
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
        let (sel_s, sel_e) = (tab.anchor.min(tab.cursor), tab.anchor.max(tab.cursor));
        let mut y = f32::from(bounds.origin.y) + geom.content_top - scroll;
        let n = tab.buffer.line_count();

        // 语法高亮：遇到第一条可见行时，一次性把高亮推进到可视区域末尾。
        // 每条逻辑行至少占一个视觉行，所以可见行数不超过 视口高度 / 行高。
        let mut syntax = tab.syntax.borrow_mut();
        syntax.refresh(&tab.name, &tab.buffer);
        let visible_rows = (geom.content_height / f32::from(LINE_HEIGHT)).ceil() as usize + 2;
        let mut highlighted = false;

        for line_idx in 0..n {
            let lt = tab.buffer.line_content(line_idx);
            // 先用无样式的排版算出本行高度，判断是否可见；不可见的行不着色。
            let h_probe = window
                .text_system()
                .shape_text(
                    SharedString::from(lt.to_string()),
                    FONT_SIZE,
                    &[mk_run(lt.len(), INK)],
                    Some(px(geom.wrap_width)),
                    None,
                )
                .ok()
                .and_then(|w| w.first().map(|l| l.wrap_boundaries().len() + 1))
                .unwrap_or(1) as f32
                * f32::from(LINE_HEIGHT);
            if y + h_probe < view_top - 60.0 || y > view_bottom + 60.0 {
                y += h_probe;
                continue;
            }
            if !highlighted {
                syntax.ensure_upto(&tab.buffer, line_idx + visible_rows);
                highlighted = true;
            }
            let runs = self.line_runs(tab, line_idx, &lt, syntax.spans(line_idx));
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

                // 选区包含本行的换行符：在行尾补一小块选区色。TextRun 的背景色
                // 只覆盖字形，空行或行尾换行被选中时原本没有任何视觉反馈。
                if sel_s < sel_e && line_idx + 1 < n {
                    let nl = tab.buffer.line_to_char(line_idx) + lt.chars().count();
                    if sel_s <= nl && nl < sel_e {
                        if let Some(p) = line0.position_for_index(lt.len(), LINE_HEIGHT) {
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(origin.x + p.x, px(line_top) + p.y),
                                    size(px(7.0), LINE_HEIGHT),
                                ),
                                SELECTION,
                            ));
                        }
                    }
                }
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_handle = self.focus_handle.clone();
        let viewport = window.viewport_size();
        let context_menu = self.context_menu.map(|pos| {
            let has_sel = self
                .active_tab()
                .map(|t| t.anchor != t.cursor)
                .unwrap_or(false);
            let can_paste = cx
                .read_from_clipboard()
                .and_then(|i| i.text())
                .map(|t| !t.is_empty())
                .unwrap_or(false);
            (pos, has_sel, can_paste)
        });
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
        let _input_focus = self.input_focus;
        let toast = self.toast.clone();
        let status = active.as_ref().map(|t| {
            let language = {
                let mut syn = t.syntax.borrow_mut();
                syn.refresh(&t.name, &t.buffer);
                syn.language().to_string()
            };
            (
                language,
                t.encoding.clone(),
                t.eol.clone(),
                t.buffer.char_count(),
                t.buffer.line_count(),
                t.path.clone().unwrap_or_else(|| "未保存到磁盘".to_string()),
            )
        });

        div()
            // GPUI 的 Div 默认 display:Block；flex_col()/flex_1() 只有在显式
            // .flex()（display:Flex）之后才生效。漏掉会让整页按内容高度堆叠，
            // 编辑区塌成 0 高、状态栏跑到窗口中间，下方留一大片空白。
            .flex()
            .flex_col()
            .size_full()
            .bg(BG_APP)
            .track_focus(&focus_handle)
            .on_key_down(cx.listener(|view, event, window, cx| {
                view.handle_key(event, window, cx);
                // 可输入的字符不在这里处理，放行给输入法接口（EntityInputHandler）
                // 写入——这样拼音等输入法才能走"组合 → 候选 → 上屏"流程。
                // 其余按键（功能键、快捷键、已在上面处理的空格/回车…）到此为止，
                // 避免平台再把它们当作文本重复送一遍。
                if !is_text_input(event) {
                    cx.stop_propagation();
                }
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
                    .flex()
                    .flex_col()
                    .flex_1()
                    .relative()
                    .on_mouse_down(MouseButton::Left, cx.listener(|view, event, window, cx| {
                        view.on_editor_mouse_down(event, window, cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(|view, event, window, cx| {
                        view.on_editor_right_click(event, window, cx);
                    }))
                    .on_scroll_wheel(cx.listener(|view, event, window, cx| {
                        view.on_scroll_wheel(event, window, cx);
                    }))
                    .child({
                        let prepaint_entity = entity.clone();
                        let paint_entity = entity.clone();
                        let move_entity = entity.clone();
                        let up_entity = entity.clone();
                        let ime_focus = focus_handle.clone();
                        canvas(
                            move |bounds, _window, cx| {
                                prepaint_entity.update(cx, |view, _cx| {
                                    view.editor_bounds = bounds;
                                });
                            },
                            move |bounds, _pre, window, cx| {
                                // 注册输入法处理器：没有它，系统输入法无法把组合文本
                                // 交给我们，拼音的字母就会被当成普通按键直接写进文档。
                                window.handle_input(
                                    &ime_focus,
                                    ElementInputHandler::new(bounds, paint_entity.clone()),
                                    cx,
                                );
                                // 窗口级鼠标监听：不要求鼠标悬停在编辑区上，
                                // 拖选移出编辑区、在别处松开都能正确处理。
                                let move_entity = move_entity.clone();
                                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| {
                                    if phase == DispatchPhase::Bubble {
                                        move_entity.update(cx, |view, cx| view.on_editor_drag(ev, window, cx));
                                    }
                                });
                                let up_entity = up_entity.clone();
                                window.on_mouse_event(move |ev: &MouseUpEvent, phase, _window, cx| {
                                    if phase == DispatchPhase::Bubble && ev.button == MouseButton::Left {
                                        up_entity.update(cx, |view, cx| view.on_editor_mouse_up(cx));
                                    }
                                });
                                paint_entity.update(cx, |view, cx| {
                                    view.paint_editor(bounds, window, cx);
                                });
                            },
                        )
                        .size_full()
                    });
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h(px(0.0))
                    .when_some(sidebar, |row, sb| row.child(sb))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .relative()
                            .child(editor)
                            .when(search_visible, |area| {
                                area.child(search_panel(&*self, &entity))
                            }),
                    )
            })
            .child(status_bar(status))
            .when_some(new_file, |root, nf| root.child(new_file_dialog(&*self, &nf, &entity)))
            .when_some(context_menu, |root, (pos, has_sel, can_paste)| {
                root.child(context_menu_view(pos, viewport, has_sel, can_paste, &entity))
            })
            .when_some(toast, |root, (msg, kind)| root.child(toast_view(&msg, kind)))
    }
}

// ===== 输入处理 =====

impl Workspace {
    fn handle_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Workspace>) {
        // 右键菜单打开时，Esc 只负责关闭菜单。
        if self.context_menu.is_some() {
            if event.keystroke.key.eq_ignore_ascii_case("escape") {
                self.context_menu = None;
                cx.notify();
            }
            return;
        }
        // 文件对话框 / 新建 / 查找输入优先
        match self.input_focus {
            InputFocus::NewFileName => {
                // 对话框里的 Enter / Escape 同样要按 GPUI 小写按键名匹配，
                // 否则会被 edit_text_field 当成普通字符追加到文件名后面。
                match event.keystroke.key.to_ascii_lowercase().as_str() {
                    "enter" => {
                        self.confirm_new_file(cx);
                        return;
                    }
                    "escape" => {
                        self.new_file = None;
                        self.input_focus = InputFocus::None;
                        cx.notify();
                        return;
                    }
                    _ => {}
                }
                if self.field_key(event, cx) {
                    if let Some(d) = self.new_file.as_mut() {
                        d.error.clear();
                    }
                }
                return;
            }
            InputFocus::SearchFind => {
                let key = event.keystroke.key.to_ascii_lowercase();
                match key.as_str() {
                    "enter" => {
                        if event.keystroke.modifiers.shift {
                            self.find_prev(cx);
                        } else {
                            self.find_next(cx);
                        }
                    }
                    "escape" => self.close_search(cx),
                    "f3" => {
                        if event.keystroke.modifiers.shift {
                            self.find_prev(cx);
                        } else {
                            self.find_next(cx);
                        }
                    }
                    _ => {
                        if self.field_key(event, cx) {
                            self.sync_search();
                        }
                    }
                }
                return;
            }
            InputFocus::SearchReplace => {
                let key = event.keystroke.key.to_ascii_lowercase();
                match key.as_str() {
                    "enter" => self.replace_current(cx),
                    "escape" => self.close_search(cx),
                    "f3" => {
                        if event.keystroke.modifiers.shift {
                            self.find_prev(cx);
                        } else {
                            self.find_next(cx);
                        }
                    }
                    _ => {
                        self.field_key(event, cx);
                    }
                }
                return;
            }
            InputFocus::None => {}
        }

        let mods = &event.keystroke.modifiers;
        let ctrl = mods.control || mods.platform;
        // GPUI 的 `Keystroke.key` 一律是小写具名键（"enter" / "backspace" /
        // "space" / "up" / "pageup" / "f3" …），与浏览器 KeyboardEvent.key 的
        // "Enter" / "Backspace" / "ArrowUp" 命名不同。早期代码按浏览器命名
        // 匹配，全部落空后把按键名当文本插入，于是按 Enter 会打出 "enter"、
        // 按退格会打出 "backspace"。这里统一归一化后再匹配。
        let key = event.keystroke.key.to_ascii_lowercase();

        if ctrl {
            let shift = mods.shift;
            match key.as_str() {
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
                // Ctrl+Shift+Z 与 Ctrl+Y 都是重做，对齐常见编辑器习惯。
                "z" => {
                    if shift {
                        self.redo(cx);
                    } else {
                        self.undo(cx);
                    }
                }
                "y" => self.redo(cx),
                // Ctrl+Home / Ctrl+End：文首 / 文末（Shift 扩展选区）
                "home" | "end" => {
                    if let Some(tab) = self.active_tab_mut() {
                        tab.cursor = if key == "home" { 0 } else { tab.buffer.char_count() };
                        if !shift {
                            tab.anchor = tab.cursor;
                        }
                    }
                    cx.notify();
                }
                // Ctrl+←/→ 按词移动（move_cursor 内部识别 control）
                "left" => self.move_cursor(-1, mods, cx),
                "right" => self.move_cursor(1, mods, cx),
                _ => return,
            }
            if matches!(key.as_str(), "x" | "v" | "z" | "y" | "home" | "end" | "left" | "right") {
                self.ensure_cursor_visible(window);
            }
            return;
        }

        match key.as_str() {
            "f3" => {
                if mods.shift {
                    self.find_prev(cx);
                } else {
                    self.find_next(cx);
                }
            }
            "escape" => {
                if self.search.visible {
                    self.close_search(cx);
                } else if let Some(tab) = self.active_tab_mut() {
                    tab.anchor = tab.cursor;
                    cx.notify();
                }
            }
            "backspace" => self.backspace(cx),
            "delete" => self.delete_forward(cx),
            "enter" => self.insert_text_at_cursor("\n", cx),
            "tab" => self.insert_text_at_cursor("\t", cx),
            "space" => self.insert_text_at_cursor(" ", cx),
            "left" => self.move_cursor(-1, mods, cx),
            "right" => self.move_cursor(1, mods, cx),
            "up" => self.move_vertical(-1, mods, window, cx),
            "down" => self.move_vertical(1, mods, window, cx),
            "home" => self.cursor_line_start(mods.shift, cx),
            "end" => self.cursor_line_end(mods.shift, cx),
            "pageup" => self.move_page(-1, mods, window, cx),
            "pagedown" => self.move_page(1, mods, window, cx),
            // 普通字符不在这里插入：由 `EntityInputHandler`（见文件末尾的 IME 部分）
            // 统一接收，才能正确支持拼音/五笔等输入法的组合输入。
            _ => return,
        }
        // Shift+方向键选到可视区域外时跟着滚动，否则选区「看不见地」扩展。
        if key != "escape" {
            self.ensure_cursor_visible(window);
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

    fn cursor_line_start(&mut self, extend: bool, cx: &mut Context<Workspace>) {
        let Some(tab) = self.active_tab_mut() else { return };
        let line = tab.buffer.char_to_line(tab.cursor);
        tab.cursor = tab.buffer.line_to_char(line);
        if !extend {
            tab.anchor = tab.cursor;
        }
        cx.notify();
    }

    /// 行尾 = 行首 + 可见字符数（不含换行符）。旧版用「下一行行首」，
    /// 光标会落到换行符之后，也就是跑到下一行开头。
    fn cursor_line_end(&mut self, extend: bool, cx: &mut Context<Workspace>) {
        let Some(tab) = self.active_tab_mut() else { return };
        let line = tab.buffer.char_to_line(tab.cursor);
        let line_end = tab.buffer.line_to_char(line) + tab.buffer.line_content(line).chars().count();
        tab.cursor = line_end;
        if !extend {
            tab.anchor = tab.cursor;
        }
        cx.notify();
    }

    /// 右键：弹出编辑菜单。点在选区之外时，先把光标移到点击处（与常见编辑器一致）。
    /// 窗口坐标 → 字符索引。
    ///
    /// `char_at_xy` / `caret_xy` 的 y 以编辑区 canvas 顶部为 0（已经包含上边距
    /// `content_top`）。旧代码在这里又减了一次 `content_top`，点击位置整体偏上
    /// 10px：点在行的下半部分会落到下一行之前，拖选时选区起止和鼠标对不上。
    fn hit_test(&self, pos: Point<Pixels>, window: &Window) -> usize {
        let bounds = self.editor_bounds;
        let geom = self.geometry(&bounds);
        let local_x = f32::from(pos.x) - f32::from(bounds.origin.x);
        let local_y = f32::from(pos.y) - f32::from(bounds.origin.y);
        let scroll = self.active_tab().map(|t| t.scroll).unwrap_or(0.0);
        let x = (local_x - geom.content_left).max(0.0);
        let y = (local_y + scroll).max(geom.content_top);
        self.char_at_xy(&geom, x, y, window)
    }

    fn on_editor_right_click(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        if self.active_tab().is_none() {
            return;
        }
        window.focus(&self.focus_handle);
        self.input_focus = InputFocus::None;
        self.marked = None;
        self.drag_mode = DragMode::None;

        let char_idx = self.hit_test(event.position, window);
        if let Some(tab) = self.active_tab_mut() {
            let (s, e) = (tab.anchor.min(tab.cursor), tab.anchor.max(tab.cursor));
            if !(s < e && char_idx >= s && char_idx <= e) {
                tab.cursor = char_idx;
                tab.anchor = char_idx;
            }
        }
        self.context_menu = Some(event.position);
        cx.notify();
    }

    fn on_editor_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        if self.active_tab().is_none() {
            return;
        }
        window.focus(&self.focus_handle);
        self.input_focus = InputFocus::None;
        self.marked = None;
        let bounds = self.editor_bounds;
        let geom = self.geometry(&bounds);
        let local_x = f32::from(event.position.x) - f32::from(bounds.origin.x);
        let local_y = f32::from(event.position.y) - f32::from(bounds.origin.y);

        if local_x > f32::from(bounds.size.width) - 14.0 {
            self.drag_mode = DragMode::Scroll;
            self.scroll_to_mouse(local_y, &geom, window, cx);
            return;
        }

        let char_idx = self.hit_test(event.position, window);
        let Some(tab) = self.active_tab_mut() else { return };
        if event.modifiers.shift {
            // Shift+点击：保留锚点，扩展选区
            tab.cursor = char_idx;
        } else if event.click_count >= 3 {
            // 三击：选中整行（含换行符，与常见编辑器一致）
            let line = tab.buffer.char_to_line(char_idx);
            tab.anchor = tab.buffer.line_to_char(line);
            tab.cursor = if line + 1 < tab.buffer.line_count() {
                tab.buffer.line_to_char(line + 1)
            } else {
                tab.buffer.char_count()
            };
        } else if event.click_count == 2 {
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

    /// 拖动中的鼠标移动。由 canvas 在 paint 阶段注册的「窗口级」监听调用：
    /// 鼠标移出编辑区（甚至移到侧边栏、状态栏上）也能继续拖选/拖滚动条。
    /// 旧版挂在编辑区 div 的 on_mouse_move 上，只有鼠标悬停在编辑区内才触发。
    fn on_editor_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Workspace>,
    ) {
        if self.drag_mode == DragMode::None {
            return;
        }
        // 左键已不在按下状态（例如在窗口外松开，没收到 MouseUp）：结束拖动。
        if event.pressed_button != Some(MouseButton::Left) {
            self.drag_mode = DragMode::None;
            return;
        }
        let bounds = self.editor_bounds;
        let geom = self.geometry(&bounds);
        let local_y = f32::from(event.position.y) - f32::from(bounds.origin.y);
        match self.drag_mode {
            DragMode::None => {}
            DragMode::Scroll => self.scroll_to_mouse(local_y, &geom, window, cx),
            DragMode::Select => {
                // 拖出上/下边缘时自动滚动，距离越远滚得越快。
                let top = geom.content_top;
                let bottom = f32::from(bounds.size.height) - geom.content_top;
                let step = if local_y < top {
                    -(top - local_y).min(120.0)
                } else if local_y > bottom {
                    (local_y - bottom).min(120.0)
                } else {
                    0.0
                };
                if step != 0.0 {
                    let (_total, max_scroll) = self.content_metrics(&geom, window);
                    if let Some(tab) = self.active_tab_mut() {
                        tab.scroll = (tab.scroll + step).clamp(0.0, max_scroll);
                    }
                }
                let char_idx = self.hit_test(event.position, window);
                let Some(tab) = self.active_tab_mut() else { return };
                if tab.cursor != char_idx || step != 0.0 {
                    tab.cursor = char_idx;
                    cx.notify();
                }
            }
        }
    }

    /// 左键松开：结束拖选。旧版完全没有这个处理，drag_mode 一直停在 Select，
    /// 于是单击之后不按键、只移动鼠标，选区也会跟着鼠标变化。
    fn on_editor_mouse_up(&mut self, cx: &mut Context<Workspace>) {
        if self.drag_mode != DragMode::None {
            self.drag_mode = DragMode::None;
            cx.notify();
        }
    }

    /// 让光标保持在可视区域内（键盘移动、输入、粘贴、撤销后调用）。
    fn ensure_cursor_visible(&mut self, window: &Window) {
        let bounds = self.editor_bounds;
        if f32::from(bounds.size.height) <= 0.0 {
            return;
        }
        let geom = self.geometry(&bounds);
        let Some((_x, y)) = self.caret_xy(&geom, window) else { return };
        let (_total, max_scroll) = self.content_metrics(&geom, window);
        let line_h = f32::from(LINE_HEIGHT);
        let Some(tab) = self.active_tab_mut() else { return };
        // caret_xy 的 y 含上边距，换算成内容坐标
        let caret_top = y - geom.content_top;
        if caret_top < tab.scroll {
            tab.scroll = caret_top;
        } else if caret_top + line_h > tab.scroll + geom.content_height {
            tab.scroll = caret_top + line_h - geom.content_height;
        }
        tab.scroll = tab.scroll.clamp(0.0, max_scroll);
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

// ===== 单行输入框（新建文件名 / 查找 / 替换）=====
//
// 旧版输入框只是一段静态文字：没有光标、不能移动、不能选择，空格键也被吞掉
// （空格是具名键，既不走输入法，也没在输入框分支里处理），所以默认文件名根本
// 改不动，只能从末尾退格。这里给它补上完整的单行编辑：光标 / 选区、方向键、
// Home/End、Shift 选择、Ctrl+A/C/X/V、Delete、空格、鼠标点击定位、双击全选。
// 文字输入仍然走输入法接口（支持中文文件名）。

/// 输入框文字的字体与字号：绘制与鼠标命中测试必须用同一套，才能点得准。
const FIELD_FONT: &str = ".SystemUIFont";
const FIELD_FONT_SIZE: Pixels = px(14.0);

fn field_slot(f: InputFocus) -> Option<usize> {
    match f {
        InputFocus::NewFileName => Some(0),
        InputFocus::SearchFind => Some(1),
        InputFocus::SearchReplace => Some(2),
        InputFocus::None => None,
    }
}

fn splice_chars(text: &str, s: usize, e: usize, ins: &str) -> String {
    let bs = char_to_byte(text, s);
    let be = char_to_byte(text, e);
    let mut out = String::with_capacity(text.len() + ins.len());
    out.push_str(&text[..bs]);
    out.push_str(ins);
    out.push_str(&text[be..]);
    out
}

impl Workspace {
    /// 把键盘焦点切到某个输入框，光标放到末尾。
    fn focus_field(&mut self, f: InputFocus) {
        self.input_focus = f;
        self.marked = None;
        let n = self.ime_field().map(|t| t.chars().count()).unwrap_or(0);
        self.field_cursor = n;
        self.field_anchor = n;
    }

    /// 输入框里的编辑键。返回文本是否被修改。
    fn field_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Workspace>) -> bool {
        let Some(text) = self.ime_field().cloned() else { return false };
        let key = event.keystroke.key.to_ascii_lowercase();
        let m = &event.keystroke.modifiers;
        let ctrl = m.control || m.platform;
        let shift = m.shift;
        let n = text.chars().count();
        let mut cur = self.field_cursor.min(n);
        let mut anc = self.field_anchor.min(n);
        let (s, e) = (cur.min(anc), cur.max(anc));
        let mut new_text: Option<String> = None;

        match (ctrl, key.as_str()) {
            (true, "a") => {
                anc = 0;
                cur = n;
            }
            (true, "c") => {
                if s < e {
                    let sel: String = text.chars().skip(s).take(e - s).collect();
                    cx.write_to_clipboard(ClipboardItem::new_string(sel));
                }
            }
            (true, "x") => {
                if s < e {
                    let sel: String = text.chars().skip(s).take(e - s).collect();
                    cx.write_to_clipboard(ClipboardItem::new_string(sel));
                    new_text = Some(splice_chars(&text, s, e, ""));
                    cur = s;
                    anc = s;
                }
            }
            (true, "v") => {
                if let Some(clip) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    // 单行输入框：换行替换成空格
                    let clip: String = clip
                        .chars()
                        .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
                        .collect();
                    new_text = Some(splice_chars(&text, s, e, &clip));
                    cur = s + clip.chars().count();
                    anc = cur;
                }
            }
            (_, "left") => {
                cur = if s < e && !shift { s } else { cur.saturating_sub(1) };
                if !shift {
                    anc = cur;
                }
            }
            (_, "right") => {
                cur = if s < e && !shift { e } else { (cur + 1).min(n) };
                if !shift {
                    anc = cur;
                }
            }
            (_, "home") | (_, "up") => {
                cur = 0;
                if !shift {
                    anc = 0;
                }
            }
            (_, "end") | (_, "down") => {
                cur = n;
                if !shift {
                    anc = n;
                }
            }
            (false, "backspace") => {
                if s < e {
                    new_text = Some(splice_chars(&text, s, e, ""));
                    cur = s;
                } else if cur > 0 {
                    new_text = Some(splice_chars(&text, cur - 1, cur, ""));
                    cur -= 1;
                }
                anc = cur;
            }
            (true, "backspace") => {
                // Ctrl+Backspace：删到开头（输入框够短，按词删意义不大）
                new_text = Some(splice_chars(&text, 0, e.max(cur), ""));
                cur = 0;
                anc = 0;
            }
            (false, "delete") => {
                if s < e {
                    new_text = Some(splice_chars(&text, s, e, ""));
                    cur = s;
                } else if cur < n {
                    new_text = Some(splice_chars(&text, cur, cur + 1, ""));
                }
                anc = cur;
            }
            (false, "space") => {
                new_text = Some(splice_chars(&text, s, e, " "));
                cur = s + 1;
                anc = cur;
            }
            _ => return false,
        }

        let changed = new_text.is_some();
        if let Some(t) = new_text {
            if let Some(f) = self.ime_field_mut() {
                *f = t;
            }
            self.marked = None;
        }
        self.field_cursor = cur;
        self.field_anchor = anc;
        cx.notify();
        changed
    }

    /// 鼠标点击输入框：聚焦并把光标放到点击处；Shift 扩展选区，双击全选。
    fn click_field(
        &mut self,
        f: InputFocus,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        window.focus(&self.focus_handle);
        if self.input_focus != f {
            self.focus_field(f);
        }
        let Some(slot) = field_slot(f) else { return };
        let text = self.ime_field().cloned().unwrap_or_default();
        let n = text.chars().count();
        if event.click_count >= 2 {
            self.field_anchor = 0;
            self.field_cursor = n;
            cx.notify();
            return;
        }
        let rect = self.field_rects[slot];
        let x = px((f32::from(event.position.x) - f32::from(rect.origin.x)).max(0.0));
        let idx = if text.is_empty() {
            0
        } else {
            let run = TextRun {
                len: text.len(),
                font: font(FIELD_FONT),
                color: INK,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let shaped = window.text_system().shape_line(
                SharedString::from(text.clone()),
                FIELD_FONT_SIZE,
                &[run],
                None,
            );
            let byte = shaped.closest_index_for_x(x).min(text.len());
            text[..byte].chars().count()
        };
        self.field_cursor = idx;
        if !event.modifiers.shift {
            self.field_anchor = idx;
        }
        self.marked = None;
        cx.notify();
    }

    /// 输入框光标的 x 偏移（相对文字起点），用于摆放输入法候选窗。
    fn field_caret_x(&self, window: &Window) -> Pixels {
        let Some(text) = self.ime_field() else { return px(0.0) };
        let prefix: String = text.chars().take(self.field_cursor).collect();
        if prefix.is_empty() {
            return px(0.0);
        }
        let run = TextRun {
            len: prefix.len(),
            font: font(FIELD_FONT),
            color: INK,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window
            .text_system()
            .shape_line(SharedString::from(prefix), FIELD_FONT_SIZE, &[run], None)
            .width
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

/// 把一个按键转换成"应该插入到文本里的字符串"。
///
/// GPUI 的 `Keystroke.key` 是**按键名**（小写，例如 `enter`、`backspace`、
/// `up`、`space`、`f3`），并不是可输入文本；`key_char` 才是平台解析出来的
/// 实际字符（包含 Shift 产生的大写、非美式键盘布局的字符、IME 结果）。
/// 特殊键的 `key_char` 恒为 `None`。因此这里只接受可打印字符，任何具名
/// 功能键都返回 `None`，从根上防止按键名被当成正文写入。
fn typed_char(event: &KeyDownEvent) -> Option<String> {
    let ks = &event.keystroke;

    // 空格键：GPUI 里 key = "space" 且 key_char = None，需要单独还原成空格。
    if ks.key.eq_ignore_ascii_case("space") {
        return Some(" ".to_string());
    }
    if let Some(ch) = ks.key_char.as_deref() {
        return (!ch.is_empty() && !ch.chars().all(|c| c.is_control()))
            .then(|| ch.to_string());
    }
    // 兜底：部分平台 / 模拟事件不带 key_char，此时只接受单个可打印字符，
    // 杜绝把 "enter"、"backspace"、"f12" 这类按键名当成文本插入。
    let mut chars = ks.key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_control() => Some(c.to_string()),
        _ => None,
    }
}

/// 旧的"按键直接写入输入框"逻辑：运行时已改由输入法接口接收文本，仅测试保留。
#[allow(dead_code)]
fn edit_text_field(field: &mut String, event: &KeyDownEvent) {
    let key = event.keystroke.key.to_ascii_lowercase();
    let mods = &event.keystroke.modifiers;
    let ctrl = mods.control || mods.platform;
    match key.as_str() {
        "backspace" => {
            field.pop();
        }
        // 功能键不产生文本；Enter / Escape / F3 由上层焦点分支处理。
        "enter" | "escape" | "f3" | "tab" | "delete" | "insert"
        | "left" | "right" | "up" | "down" | "home" | "end"
        | "pageup" | "pagedown" => {}
        _ => {
            if ctrl || mods.alt {
                return;
            }
            if let Some(ch) = typed_char(event) {
                field.push_str(&ch);
            }
        }
    }
}

/// 这个按键是否是"要写进文本的字符"（交给输入法接口处理）。
fn is_text_input(event: &KeyDownEvent) -> bool {
    let ks = &event.keystroke;
    let m = &ks.modifiers;
    // Ctrl/Cmd 组合是快捷键；单独的 Alt 组合不产生文本。
    // Windows 的 AltGr 表现为 Ctrl+Alt，仍然可能产生文本，因此放行。
    if m.platform || (m.control && !m.alt) || (m.alt && !m.control) {
        return false;
    }
    // 这些具名键由 handle_key 显式处理，不能再让平台当文本送一遍。
    if matches!(
        ks.key.to_ascii_lowercase().as_str(),
        "space" | "enter" | "tab" | "backspace" | "delete" | "escape"
    ) {
        return false;
    }
    matches!(ks.key_char.as_deref(), Some(c) if !c.is_empty() && !c.chars().all(|ch| ch.is_control()))
}

fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(s.len())
}

fn str_char_to_utf16(s: &str, char_idx: usize) -> usize {
    s.chars().take(char_idx).map(char::len_utf16).sum()
}

fn str_utf16_to_char(s: &str, utf16_idx: usize) -> usize {
    let mut acc = 0;
    for (i, c) in s.chars().enumerate() {
        if acc >= utf16_idx {
            return i;
        }
        acc += c.len_utf16();
    }
    s.chars().count()
}

// ===== 输入法（IME）=====
//
// 系统输入法通过 `EntityInputHandler` 与我们交互：拼音输入时先"标记"一段组合文本
// （`replace_and_mark_text_in_range`，显示为带下划线的拼音/候选），选定后再用
// `replace_text_in_range` 把最终汉字上屏。所有区间都是 UTF-16 码元偏移，
// 这里与内部的字符索引互转。
//
// 输入目标取决于 `input_focus`：None 时写入当前标签的缓冲区，否则写入对应输入框
// （输入框的光标恒在末尾）。组合中的临时文本不进撤销栈，上屏时才作为一次正常编辑记录。

impl Workspace {
    fn ime_field(&self) -> Option<&String> {
        match self.input_focus {
            InputFocus::None => None,
            InputFocus::NewFileName => self.new_file.as_ref().map(|d| &d.name),
            InputFocus::SearchFind => Some(&self.search.pattern),
            InputFocus::SearchReplace => Some(&self.search.replace),
        }
    }

    fn ime_field_mut(&mut self) -> Option<&mut String> {
        match self.input_focus {
            InputFocus::None => None,
            InputFocus::NewFileName => self.new_file.as_mut().map(|d| &mut d.name),
            InputFocus::SearchFind => Some(&mut self.search.pattern),
            InputFocus::SearchReplace => Some(&mut self.search.replace),
        }
    }

    /// 当前输入目标的字符总数；没有可输入目标时为 None。
    fn ime_len(&self) -> Option<usize> {
        if self.input_focus == InputFocus::None {
            self.active_tab().map(|t| t.buffer.char_count())
        } else {
            self.ime_field().map(|f| f.chars().count())
        }
    }

    fn ime_char_to_u16(&self, c: usize) -> usize {
        if self.input_focus == InputFocus::None {
            self.active_tab().map(|t| t.buffer.char_to_utf16(c)).unwrap_or(0)
        } else {
            self.ime_field().map(|f| str_char_to_utf16(f, c)).unwrap_or(0)
        }
    }

    fn ime_u16_to_char(&self, u: usize) -> usize {
        if self.input_focus == InputFocus::None {
            self.active_tab().map(|t| t.buffer.utf16_to_char(u)).unwrap_or(0)
        } else {
            self.ime_field().map(|f| str_utf16_to_char(f, u)).unwrap_or(0)
        }
    }

    /// 当前选区（字符索引，已排序）。输入框的光标恒在末尾。
    fn ime_selection(&self) -> Range<usize> {
        if self.input_focus == InputFocus::None {
            self.active_tab()
                .map(|t| t.anchor.min(t.cursor)..t.anchor.max(t.cursor))
                .unwrap_or(0..0)
        } else {
            let n = self.ime_len().unwrap_or(0);
            let c = self.field_cursor.min(n);
            let a = self.field_anchor.min(n);
            c.min(a)..c.max(a)
        }
    }

    /// 组合区间（夹紧到当前文本长度，防止切换标签/输入框后残留的区间越界）。
    fn ime_marked(&self) -> Option<Range<usize>> {
        let n = self.ime_len()?;
        self.marked.clone().map(|m| m.start.min(n)..m.end.min(n))
    }

    /// 本次替换作用的区间：平台显式给出 > 正在组合的区间 > 当前选区。
    fn ime_target_range(&self, range_utf16: Option<Range<usize>>) -> Range<usize> {
        if let Some(r) = range_utf16 {
            let s = self.ime_u16_to_char(r.start);
            let e = self.ime_u16_to_char(r.end).max(s);
            return s..e;
        }
        self.ime_marked().unwrap_or_else(|| self.ime_selection())
    }

    /// 把 `r` 替换成 `text`。`mark` 为真表示 `text` 是组合中的临时文本。
    /// 返回替换后文本的起点（字符索引）。
    fn ime_replace(&mut self, mut r: Range<usize>, text: &str, mark: bool) -> usize {
        let new_chars = text.chars().count();
        let was_marked = self.ime_marked();

        if self.input_focus != InputFocus::None {
            let Some(field) = self.ime_field_mut() else { return r.start };
            // 先清掉旧的组合文本（如果区间不是它），并修正偏移。
            if let Some(m) = &was_marked {
                if r != *m {
                    let (bs, be) = (char_to_byte(field, m.start), char_to_byte(field, m.end));
                    field.replace_range(bs..be, "");
                    if r.start >= m.end {
                        let len = m.end - m.start;
                        r = r.start - len..r.end - len;
                    }
                }
            }
            let (bs, be) = (char_to_byte(field, r.start), char_to_byte(field, r.end));
            field.replace_range(bs..be, text);
            self.marked = (mark && new_chars > 0).then(|| r.start..r.start + new_chars);
            // 光标放到插入文本之后（组合中则在组合文本末尾）
            self.field_cursor = r.start + new_chars;
            self.field_anchor = self.field_cursor;
            if !mark && self.input_focus == InputFocus::NewFileName {
                if let Some(d) = self.new_file.as_mut() {
                    d.error.clear();
                }
            }
            return r.start;
        }

        // 编辑器缓冲区
        let Some(tab) = self.active_tab_mut() else { return r.start };
        let mut r_is_marked = false;
        if let Some(m) = &was_marked {
            if r == *m {
                r_is_marked = true;
            } else {
                tab.buffer.replace_silent(m.clone(), "");
                if r.start >= m.end {
                    let len = m.end - m.start;
                    r = r.start - len..r.end - len;
                }
            }
        }
        let start = r.start;
        let end_pos;
        if mark {
            if r_is_marked {
                end_pos = tab.buffer.replace_silent(r.clone(), text);
            } else {
                if r.start < r.end {
                    tab.buffer.remove(r.clone()); // 被选中的文字作为一次正常编辑删除（可撤销）
                }
                end_pos = tab.buffer.replace_silent(start..start, text);
            }
            self.marked = (new_chars > 0).then(|| start..end_pos);
        } else {
            if r_is_marked {
                // 组合文本只是临时的：先静默移除，再把最终文本作为一次正常编辑插入。
                tab.buffer.replace_silent(r.clone(), "");
                end_pos = tab.buffer.insert(start, text);
            } else if r.start < r.end {
                end_pos = tab.buffer.replace(r.clone(), text);
            } else {
                end_pos = tab.buffer.insert(start, text);
            }
            tab.cursor = end_pos;
            tab.anchor = end_pos;
            self.marked = None;
        }
        start
    }
}

impl EntityInputHandler for Workspace {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let s = self.ime_u16_to_char(range.start);
        let e = self.ime_u16_to_char(range.end).max(s);
        *adjusted_range = Some(self.ime_char_to_u16(s)..self.ime_char_to_u16(e));
        if self.input_focus == InputFocus::None {
            Some(self.active_tab()?.buffer.slice_string(s..e))
        } else {
            Some(self.ime_field()?.chars().skip(s).take(e - s).collect())
        }
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.ime_len()?;
        let sel = self.ime_selection();
        let reversed = self.input_focus == InputFocus::None
            && self.active_tab().map(|t| t.anchor > t.cursor).unwrap_or(false);
        Some(UTF16Selection {
            range: self.ime_char_to_u16(sel.start)..self.ime_char_to_u16(sel.end),
            reversed,
        })
    }

    fn marked_text_range(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        let m = self.ime_marked()?;
        Some(self.ime_char_to_u16(m.start)..self.ime_char_to_u16(m.end))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ime_len().is_none() {
            return;
        }
        let r = self.ime_target_range(range);
        self.ime_replace(r, text, false);
        if self.input_focus == InputFocus::SearchFind {
            self.sync_search();
        }
        if self.input_focus == InputFocus::None {
            self.ensure_cursor_visible(window);
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.ime_len().is_none() {
            return;
        }
        let r = self.ime_target_range(range);
        let start = self.ime_replace(r, new_text, true);
        if self.input_focus == InputFocus::None {
            // 组合期间光标放在输入法指定的位置（默认在组合文本末尾）。
            let caret = match new_selected_range {
                Some(sel) => start + str_utf16_to_char(new_text, sel.end),
                None => start + new_text.chars().count(),
            };
            if let Some(tab) = self.active_tab_mut() {
                tab.cursor = caret;
                tab.anchor = caret;
            }
        } else if let Some(sel) = new_selected_range {
            self.field_cursor = start + str_utf16_to_char(new_text, sel.end);
            self.field_anchor = self.field_cursor;
        }
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        // 用来摆放输入法候选窗：编辑器里贴着光标；输入框没有逐字符几何，
        // 就放在编辑区上方偏左的位置。
        if self.input_focus == InputFocus::None {
            let geom = self.geometry(&element_bounds);
            let (x, y) = self.caret_xy(&geom, window)?;
            let scroll = self.active_tab()?.scroll.max(0.0);
            Some(Bounds::new(
                point(
                    element_bounds.origin.x + px(geom.content_left + x),
                    element_bounds.origin.y + px(y - scroll),
                ),
                size(px(2.0), LINE_HEIGHT),
            ))
        } else {
            // 输入框：贴着输入框里的光标
            let rect = field_slot(self.input_focus).map(|i| self.field_rects[i])?;
            let x = self.field_caret_x(window);
            Some(Bounds::new(
                point(rect.origin.x + x, rect.origin.y),
                size(px(2.0), px(f32::from(rect.size.height).max(f32::from(LINE_HEIGHT)))),
            ))
        }
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

// ===== UI 组件（返回元素的自由函数） =====

const MENU_W: f32 = 232.0;
const MENU_ITEM_H: f32 = 32.0;

/// 编辑区右键菜单：整窗透明遮罩（点任意处关闭）+ 菜单本体。
fn context_menu_view(
    pos: Point<Pixels>,
    viewport: Size<Pixels>,
    has_sel: bool,
    can_paste: bool,
    entity: &Entity<Workspace>,
) -> Div {
    // (标签, 快捷键, 是否可用, 动作)；None 表示分隔线。
    type Act = fn(&mut Workspace, &mut Context<Workspace>);
    let items: Vec<Option<(&str, &str, bool, Act)>> = vec![
        Some(("剪切", "Ctrl+X", has_sel, |ws, cx| ws.cut_selection(cx))),
        Some(("复制", "Ctrl+C", has_sel, |ws, cx| ws.copy_selection(cx))),
        Some(("粘贴", "Ctrl+V", can_paste, |ws, cx| ws.paste(cx))),
        Some(("粘贴为纯文本", "Ctrl+Shift+V", can_paste, |ws, cx| ws.paste(cx))),
        None,
        Some(("全选", "Ctrl+A", true, |ws, cx| ws.select_all(cx))),
    ];
    let n_items = items.iter().filter(|i| i.is_some()).count() as f32;
    let n_seps = items.iter().filter(|i| i.is_none()).count() as f32;
    let menu_h = n_items * MENU_ITEM_H + n_seps * 9.0 + 12.0;

    // 不超出窗口：靠近右/下边缘时向左/上翻转。
    let mut x = f32::from(pos.x);
    let mut y = f32::from(pos.y);
    if x + MENU_W > f32::from(viewport.width) {
        x = (x - MENU_W).max(4.0);
    }
    if y + menu_h > f32::from(viewport.height) {
        y = (y - menu_h).max(4.0);
    }

    let mut menu = div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(MENU_W))
        .py(px(6.0))
        .flex()
        .flex_col()
        .bg(white())
        .border(px(1.0))
        .border_color(BORDER)
        .rounded(px(10.0))
        .shadow_lg()
        // 点菜单空白处不应穿透到遮罩而关闭
        .on_mouse_down(MouseButton::Left, |_e, _w, cx| cx.stop_propagation());

    for item in items {
        match item {
            None => {
                menu = menu.child(div().my(px(4.0)).h(px(1.0)).bg(BORDER));
            }
            Some((label, shortcut, enabled, act)) => {
                let e = entity.clone();
                let row = div()
                    .h(px(MENU_ITEM_H))
                    .mx(px(6.0))
                    .px(px(10.0))
                    .rounded(px(6.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(13.5))
                    .child(div().child(label.to_string()))
                    .child(div().text_size(px(12.5)).child(shortcut.to_string()));
                menu = menu.child(if enabled {
                    row.text_color(INK)
                        .cursor(CursorStyle::PointingHand)
                        .hover(|s| s.bg(BG_HOVER))
                        .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
                            cx.stop_propagation();
                            e.update(cx, |ws, cx| {
                                ws.context_menu = None;
                                act(ws, cx);
                                cx.notify();
                            });
                        })
                } else {
                    row.text_color(INK_FAINT)
                        .on_mouse_down(MouseButton::Left, |_e, _w, cx| cx.stop_propagation())
                });
            }
        }
    }

    let e_l = entity.clone();
    let e_r = entity.clone();
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .size_full()
        .occlude()
        .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
            e_l.update(cx, |ws, cx| {
                ws.context_menu = None;
                cx.notify();
            });
        })
        .on_mouse_down(MouseButton::Right, move |_e, _w, cx| {
            e_r.update(cx, |ws, cx| {
                ws.context_menu = None;
                cx.notify();
            });
        })
        .child(menu)
}

fn title_bar(active: Option<&Tab>) -> Div {
    // 程序名已经显示在系统标题栏里，这里只显示当前文件，避免重复。
    let title = match active {
        Some(t) => {
            let dirty = if t.is_dirty() { "● " } else { "" };
            if let Some(p) = &t.path {
                format!("{}{} — {}", dirty, t.name, p)
            } else {
                format!("{}{}", dirty, t.name)
            }
        }
        None => "未打开文件".to_string(),
    };
    div()
        .h(px(36.0))
        .flex_shrink_0()
        .bg(BG_PANEL)
        .border_b_1()
        .border_color(BORDER)
        .flex()
        .items_center()
        .px(px(12.0))
        .text_color(INK_DIM)
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
        .flex_shrink_0()
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
    let mut list = div().flex().flex_col().flex_1().p(px(6.0));
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
        .flex()
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

fn status_bar(status: Option<(String, String, String, usize, usize, String)>) -> Div {
    match status {
        Some((language, enc, eol, chars, lines, path)) => div()
            .h(px(26.0))
            .flex_shrink_0()
            .bg(ACCENT)
            .text_color(white())
            .flex()
            .items_center()
            .px(px(14.0))
            .gap(px(18.0))
            .child(div().child(language))
            .child(div().child(enc))
            .child(div().child(eol))
            .child(div().child(format!("{} 字符 · {} 行", chars, lines)))
            .child(div().flex_1())
            .child(div().child(path)),
        None => div()
            .h(px(26.0))
            .flex_shrink_0()
            .bg(ACCENT)
            .text_color(white())
            .flex()
            .items_center()
            .px(px(14.0))
            .child("没有打开的文件"),
    }
}

/// 单行输入框：显示文字、选区与光标；点击定位光标。
///
/// 文字区域里放一个透明 canvas，在 prepaint 时把文字起点记到 `field_rects`，
/// 鼠标点击时据此换算点击处的字符位置。
fn text_field(ws: &Workspace, kind: InputFocus, text: &str, placeholder: &str, entity: &Entity<Workspace>) -> Div {
    let focused = ws.input_focus == kind;
    let n = text.chars().count();
    let (cur, anc) = (ws.field_cursor.min(n), ws.field_anchor.min(n));
    let (s, e) = if focused { (cur.min(anc), cur.max(anc)) } else { (n, n) };
    let slot = field_slot(kind).unwrap_or(0);

    let caret = || {
        div()
            .flex_none()
            .w(px(1.5))
            .h(px(18.0))
            .bg(ACCENT)
    };

    let e_rect = entity.clone();
    let mut row = div()
        .relative()
        .flex()
        .flex_row()
        .items_center()
        .h(px(20.0))
        .whitespace_nowrap()
        .overflow_hidden()
        .font_family(FIELD_FONT)
        .text_size(FIELD_FONT_SIZE)
        .child(
            canvas(
                move |bounds, _window, cx| {
                    e_rect.update(cx, |ws, _| ws.field_rects[slot] = bounds);
                },
                |_, _, _, _| {},
            )
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full(),
        );

    if text.is_empty() {
        if focused {
            row = row.child(caret());
        }
        row = row.child(div().text_color(INK_FAINT).child(placeholder.to_string()));
    } else {
        let pre: String = text.chars().take(s).collect();
        let sel: String = text.chars().skip(s).take(e - s).collect();
        let post: String = text.chars().skip(e).collect();
        if !pre.is_empty() {
            row = row.child(div().text_color(INK).child(pre));
        }
        if focused && cur == s {
            row = row.child(caret());
        }
        if !sel.is_empty() {
            row = row.child(div().text_color(INK).bg(SELECTION).child(sel));
        }
        if focused && cur != s {
            row = row.child(caret());
        }
        if !post.is_empty() {
            row = row.child(div().text_color(INK).child(post));
        }
    }

    let e_click = entity.clone();
    div()
        .mt(px(4.0))
        .border(px(1.0))
        .border_color(if focused { ACCENT } else { BORDER_STRONG })
        .rounded(px(4.0))
        .px(px(8.0))
        .py(px(6.0))
        .bg(BG_APP)
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            e_click.update(cx, |ws, cx| ws.click_field(kind, ev, window, cx));
        })
        .child(row)
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
        .occlude()
        .on_mouse_down(MouseButton::Left, |_e, _w, cx| cx.stop_propagation())
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
                .child(text_field(ws, InputFocus::SearchFind, &ws.search.pattern, "输入要查找的内容…", entity)),
        )
        .when(replace_mode, |area| {
            area.child(
                div()
                    .mt(px(10.0))
                    .child(field_label("替换为："))
                    .child(text_field(ws, InputFocus::SearchReplace, &ws.search.replace, "输入替换内容…", entity)),
            )
        })
        .child(
            div()
                .mt(px(10.0))
                .flex()
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
                .flex()
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

fn new_file_dialog(ws: &Workspace, nf: &NewFileDialog, entity: &Entity<Workspace>) -> Div {
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
            // 只有点在卡片之外（真正点到遮罩）才会走到这里：卡片自身 occlude 并
            // stop_propagation，点选项不会再穿透到遮罩把弹窗关掉。
            e_bg.update(cx, |ws, cx| {
                ws.new_file = None;
                ws.input_focus = InputFocus::None;
                cx.notify();
            });
        });

    let mut encoding_rows = div().flex().flex_col().gap(px(4.0));
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
                .hover(|s| s.bg(BG_HOVER))
                .on_mouse_down(MouseButton::Left, move |_e, _w, cx| {
                    e.update(cx, |ws, cx| {
                        if let Some(d) = ws.new_file.as_mut() {
                            d.encoding = idx;
                        }
                        cx.notify();
                    });
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
                        .flex()
                        .flex_col()
                        .child(div().text_size(px(12.0)).text_color(INK).child(*code))
                        .child(div().text_size(px(10.5)).text_color(INK_FAINT).child(*desc)),
                ),
        );
    }

    // 弹窗为何「一点选项就消失」：遮罩和卡片是兄弟元素，卡片原来的空
    // on_mouse_down 既不 occlude 也不 stop_propagation，GPUI 会把同一次按下
    // 分发给鼠标下所有未被遮挡的元素——遮罩的「点空白关闭」也一起触发了，
    // 下层编辑区还会把输入焦点抢回去。这里两层都 occlude：
    //   外层容器 → 挡住编辑区 / 侧边栏；卡片 → 挡住遮罩。
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .child(overlay)
        .child(
            div()
                .w(px(420.0))
                .max_h(relative(0.9))
                .flex()
                .flex_col()
                .bg(BG_APP)
                .border(px(1.0))
                .border_color(BORDER_STRONG)
                .rounded(px(8.0))
                .shadow_lg()
                .p(px(16.0))
                .text_color(INK)
                .occlude()
                .on_mouse_down(MouseButton::Left, |_e, _w, cx| cx.stop_propagation())
                .child(div().text_size(px(14.0)).font_weight(FontWeight::BOLD).child("新建文件"))
                .child(
                    div()
                        .mt(px(12.0))
                        .child(field_label("文件名："))
                        .child(text_field(ws, InputFocus::NewFileName, &nf.name, "输入文件名…", entity)),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .child(field_label("编码："))
                        .child(
                            div()
                                .id("new-file-encodings")
                                .mt(px(4.0))
                                .max_h(px(260.0))
                                .overflow_y_scroll()
                                .child(encoding_rows),
                        ),
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
                            ws.input_focus = InputFocus::None;
                            cx.notify();
                        }))
                        .child(sp_button_primary(e_ok, "确定", |ws, cx| ws.confirm_new_file(cx))),
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

#[cfg(test)]
mod key_input_tests {
    // 不能用 `use super::*`：`super` 通过 `use gpui::*` 引入了
    // `gpui::test` 属性宏，会遮蔽内置的 `#[test]` 并导致宏递归展开。
    use super::{edit_text_field, typed_char};
    use gpui::{KeyDownEvent, Keystroke, Modifiers};

    /// 构造真实运行时的按键事件：GPUI 对特殊键的 `key_char` 恒为 `None`。
    fn key_event(key: &str, key_char: Option<&str>) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: Keystroke {
                modifiers: Modifiers::none(),
                key: key.to_string(),
                key_char: key_char.map(String::from),
            },
            is_held: false,
        }
    }

    /// `handle_key` 主分支使用的按键名，必须保持 GPUI 的小写命名。
    const HANDLED_KEYS: &[&str] = &[
        "f3", "escape", "backspace", "delete", "enter", "tab", "space", "left", "right", "up",
        "down", "home", "end", "pageup", "pagedown",
    ];

    #[test]
    fn handled_keys_are_lowercase_gpui_names() {
        for k in HANDLED_KEYS {
            assert_eq!(k.to_ascii_lowercase(), *k, "{k} 不是 GPUI 的小写按键名");
        }
    }

    #[test]
    fn every_gpui_named_key_is_handled_or_filtered() {
        // 取自 gpui 0.2.2 platform/windows/events.rs::parse_immutable 的具名键。
        let named = [
            "space", "backspace", "enter", "tab", "up", "down", "right", "left", "home", "end",
            "pageup", "pagedown", "back", "forward", "escape", "insert", "delete", "menu", "f1",
            "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12",
        ];
        for k in named {
            let e = key_event(k, None);
            let normalized = e.keystroke.key.to_ascii_lowercase();
            if HANDLED_KEYS.contains(&normalized.as_str()) {
                continue; // 由 handle_key 显式处理
            }
            assert_eq!(
                typed_char(&e),
                None,
                "{k} 未被显式处理却会产生文本，按键名会被写进正文"
            );
        }
    }

    #[test]
    fn typed_char_returns_only_real_text() {
        assert_eq!(typed_char(&key_event("space", None)), Some(" ".into()));
        assert_eq!(typed_char(&key_event("a", None)), Some("a".into()));
        assert_eq!(typed_char(&key_event("中", None)), Some("中".into()));
        // key_char 优先：Shift 大写与 IME 结果
        assert_eq!(typed_char(&key_event("a", Some("A"))), Some("A".into()));
        assert_eq!(typed_char(&key_event("a", Some("你好"))), Some("你好".into()));
        // 控制字符、空串与未匹配的功能键都不进正文
        assert_eq!(typed_char(&key_event("a", Some("\n"))), None);
        assert_eq!(typed_char(&key_event("a", Some(""))), None);
        assert_eq!(typed_char(&key_event("f12", None)), None);
    }

    #[test]
    fn edit_text_field_uses_gpui_key_names() {
        let mut s = String::from("abc");
        edit_text_field(&mut s, &key_event("backspace", None));
        assert_eq!(s, "ab");

        // 功能键不再把 "enter" / "escape" 等按键名写进输入框
        for k in ["enter", "escape", "f3", "tab", "delete", "up", "home"] {
            edit_text_field(&mut s, &key_event(k, None));
        }
        assert_eq!(s, "ab");

        edit_text_field(&mut s, &key_event("a", Some("A")));
        assert_eq!(s, "abA");
        edit_text_field(&mut s, &key_event("space", None));
        assert_eq!(s, "abA ");
    }
}
