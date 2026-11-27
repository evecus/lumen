//! Lumen 的视觉主题，复刻原 Tauri 版的明亮简洁白底配色。
//!
//! GPUI 的颜色类型是 `Hsla`（h/s/l/a 均为 0..1）。由于 GPUI 0.2 的
//! `rgb()`/`hsla()` 都不是 const fn，这里直接用 `Hsla` 结构体字面量定义
//! 常量（分量由对应 sRGB 值换算而来）。

use gpui::*;

const fn hsl(h: f32, s: f32, l: f32) -> Hsla {
    Hsla { h, s, l, a: 1.0 }
}
const fn hsla(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    Hsla { h, s, l, a }
}

pub const FONT_SIZE: Pixels = px(13.0);
pub const LINE_HEIGHT: Pixels = px(20.0);

pub const BG_APP: Hsla = hsl(0.0, 0.0, 1.0);
pub const BG_PANEL: Hsla = hsl(0.6111, 0.2308, 0.9745);
pub const BG_EDITOR: Hsla = hsl(0.0, 0.0, 1.0);
pub const BG_HOVER: Hsla = hsl(0.6, 0.2, 0.951);
pub const BG_ACTIVE: Hsla = hsl(0.6042, 0.3077, 0.949);

pub const INK: Hsla = hsl(0.6176, 0.2152, 0.1549);
pub const INK_DIM: Hsla = hsl(0.6111, 0.0894, 0.4608);
pub const INK_FAINT: Hsla = hsl(0.6019, 0.1059, 0.6667);

pub const BORDER: Hsla = hsl(0.6042, 0.1739, 0.9098);
pub const BORDER_STRONG: Hsla = hsl(0.6026, 0.1733, 0.8529);

pub const ACCENT: Hsla = hsl(0.6105, 0.8407, 0.5569);
pub const ACCENT_HOVER: Hsla = hsl(0.6149, 0.7569, 0.5);
pub const WARN: Hsla = hsl(0.0931, 0.6869, 0.6118);
pub const DANGER: Hsla = hsl(0.9947, 0.7512, 0.5902);
pub const SUCCESS: Hsla = hsl(0.4356, 0.5841, 0.4431);

/// 选区背景（原 CSS: rgba(47,111,237,0.25)）
pub const SELECTION: Hsla = hsla(0.60, 0.83, 0.56, 0.28);
/// 查找匹配高亮（原 CSS: rgba(224,164,88,0.35)）
pub const MATCH_HL: Hsla = hsla(0.09, 0.72, 0.62, 0.35);
pub const MATCH_HL_SEL: Hsla = hsla(0.60, 0.83, 0.56, 0.42);
/// 与选中词 / 光标处单词相同的其它位置
pub const OCCURRENCE_HL: Hsla = hsla(0.60, 0.30, 0.50, 0.16);
/// 光标旁的括号与它的配对括号
pub const BRACKET_HL: Hsla = hsla(0.36, 0.55, 0.45, 0.30);
/// 缩进参考线
pub const INDENT_GUIDE: Hsla = hsl(0.60, 0.16, 0.89);

/// 编辑器等宽字体族（Linux 下 fontconfig 会把 "monospace" 解析为系统等宽字体）
pub const EDITOR_FONT: &str = "monospace";
