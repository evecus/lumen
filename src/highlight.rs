//! 语法高亮：基于 syntect + two-face（bat 项目整理的语法包，约 190 种语言 / 文本格式）。
//!
//! - 正则引擎用纯 Rust 的 fancy-regex，不依赖 C 库（Oniguruma），Windows / MSVC 直接编译；
//! - 按文件名 → 扩展名 → 首行（`#!/bin/bash`、`<?xml` 等）识别语言；
//! - 增量高亮：每行缓存「行文本哈希 + 行尾解析状态 + 着色区间」。编辑后从第一处
//!   变化重新解析，一旦某行的行尾状态与旧缓存一致，后面的行直接复用；
//!   且只解析到当前可视区域的最后一行，大文件滚动到中间也不用从头全量解析。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::OnceLock;

use gpui::{Hsla, Rgba};
use two_face::re_exports::syntect;
use two_face::theme::EmbeddedThemeName;

use syntect::highlighting::{HighlightIterator, HighlightState, Highlighter, Theme};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

use crate::buffer::Buffer;

/// 超过这个字符数的文件不做高亮（避免几十 MB 的日志拖慢绘制）。
const MAX_CHARS: usize = 5_000_000;
/// 单行超过这个字节数不解析（压缩后的 JS / JSON 一行可能有几 MB）。
const MAX_LINE_BYTES: usize = 10_000;

/// 一段着色：行内字节区间 [start, end) + 前景色。
pub type Span = (usize, usize, Hsla);

struct Assets {
    syntaxes: SyntaxSet,
    theme: Theme,
}

static ASSETS: OnceLock<Assets> = OnceLock::new();

fn assets() -> &'static Assets {
    ASSETS.get_or_init(|| Assets {
        syntaxes: two_face::syntax::extra_newlines(),
        // 白底配色，与 Lumen 的明亮主题一致。
        theme: two_face::theme::extra()
            .get(EmbeddedThemeName::InspiredGithub)
            .clone(),
    })
}

/// 启动时在后台线程预加载语法包（反序列化约 30~80ms），避免打开第一个文件时卡一下。
pub fn preload() {
    std::thread::spawn(|| {
        let _ = assets();
    });
}

/// 扩展名别名：语法包里没有登记、但格式上等价的常见扩展名。
const EXT_ALIASES: &[(&str, &str)] = &[
    ("jsonc", "json"),
    ("json5", "json"),
    ("webmanifest", "json"),
    ("cfg", "ini"),
    ("conf", "ini"),
    ("reg", "ini"),
    ("inf", "ini"),
    ("props", "xml"),
    ("targets", "xml"),
    ("csproj", "xml"),
    ("vbproj", "xml"),
    ("vcxproj", "xml"),
    ("fsproj", "xml"),
    ("xaml", "xml"),
    ("svg", "xml"),
    ("plist", "xml"),
    ("manifest", "xml"),
    ("resx", "xml"),
    ("rc", "c"),
    ("mts", "ts"),
    ("cts", "ts"),
    ("htm", "html"),
    ("xhtml", "html"),
    ("cmd", "bat"),
    ("markdown", "md"),
];

/// 文件名别名（无扩展名或扩展名有歧义的文件）。
const NAME_ALIASES: &[(&str, &str)] = &[
    ("cargo.lock", "toml"),
    ("pipfile", "toml"),
    ("poetry.lock", "toml"),
    ("go.mod", "go"),
    ("go.sum", "go"),
    ("cmakelists.txt", "cmake"),
    ("dockerfile", "dockerfile"),
    ("containerfile", "dockerfile"),
    ("makefile", "makefile"),
    ("gnumakefile", "makefile"),
    ("justfile", "makefile"),
    (".bashrc", "bash"),
    (".zshrc", "bash"),
    (".profile", "bash"),
    (".env", "env"),
    (".editorconfig", "ini"),
    (".npmrc", "ini"),
    (".gitconfig", "ini"),
];

fn usable(s: &'static SyntaxReference) -> Option<&'static SyntaxReference> {
    (s.name != "Plain Text").then_some(s)
}

/// 识别语言。返回 None 表示按纯文本显示。
pub fn detect_syntax(file_name: &str, first_line: &str) -> Option<&'static SyntaxReference> {
    let ss = &assets().syntaxes;
    let path = Path::new(file_name);
    let base = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(file_name);
    let lower = base.to_ascii_lowercase();

    // 1. 特定文件名（Dockerfile、Makefile、CMakeLists.txt、Cargo.lock …）
    if let Some((_, ext)) = NAME_ALIASES.iter().find(|(n, _)| *n == lower) {
        if let Some(s) = ss.find_syntax_by_extension(ext) {
            return usable(s);
        }
    }
    // 语法包自己登记的完整文件名（.gitignore、.gitattributes、PKGBUILD …）
    if let Some(s) = ss.find_syntax_by_extension(base) {
        if let Some(s) = usable(s) {
            return Some(s);
        }
    }

    // 2. 扩展名（原样 → 小写 → 别名）
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_ascii_lowercase();
        let found = ss
            .find_syntax_by_extension(ext)
            .or_else(|| ss.find_syntax_by_extension(&ext_lower))
            .or_else(|| {
                EXT_ALIASES
                    .iter()
                    .find(|(a, _)| *a == ext_lower)
                    .and_then(|(_, t)| ss.find_syntax_by_extension(t))
            });
        if let Some(s) = found {
            // .txt / .log 等明确是纯文本的扩展名，不再看首行
            return usable(s);
        }
    }

    // 3. 首行：#!/usr/bin/env python、<?xml、<?php …
    ss.find_syntax_by_first_line(first_line).and_then(usable)
}

fn hash_line(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

fn to_hsla(c: syntect::highlighting::Color) -> Hsla {
    Rgba {
        r: c.r as f32 / 255.0,
        g: c.g as f32 / 255.0,
        b: c.b as f32 / 255.0,
        a: c.a as f32 / 255.0,
    }
    .into()
}

#[derive(Clone)]
struct LineState {
    parse: ParseState,
    highlight: HighlightState,
}

struct LineEntry {
    hash: u64,
    /// 本行结束时的解析状态（= 下一行的起始状态）。
    end: LineState,
    spans: Vec<Span>,
}

/// 每个标签页一份的高亮缓存。
pub struct SyntaxCache {
    /// 用于识别语言的文件名；标签改名（另存为）后会重新识别。
    key: Option<String>,
    syntax: Option<&'static SyntaxReference>,
    lines: Vec<LineEntry>,
}

impl Default for SyntaxCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxCache {
    pub fn new() -> Self {
        Self {
            key: None,
            syntax: None,
            lines: Vec::new(),
        }
    }

    /// 状态栏显示的语言名。
    pub fn language(&self) -> &str {
        self.syntax.map(|s| s.name.as_str()).unwrap_or("纯文本")
    }

    /// 文件名变化时重新识别语言并清空缓存。每帧调用开销只是一次字符串比较。
    pub fn refresh(&mut self, file_name: &str, buffer: &Buffer) {
        if self.key.as_deref() == Some(file_name) {
            return;
        }
        let first = if buffer.line_count() > 0 {
            buffer.line_content(0)
        } else {
            String::new()
        };
        self.syntax = detect_syntax(file_name, &first);
        self.key = Some(file_name.to_string());
        self.lines.clear();
    }

    fn initial_state(syntax: &SyntaxReference, hl: &Highlighter) -> LineState {
        LineState {
            parse: ParseState::new(syntax),
            highlight: HighlightState::new(hl, ScopeStack::new()),
        }
    }

    /// 确保第 0..=last 行的高亮结果有效（只重新解析发生变化的行）。
    pub fn ensure_upto(&mut self, buffer: &Buffer, last: usize) {
        let Some(syntax) = self.syntax else {
            return;
        };
        let n = buffer.line_count();
        if n == 0 || buffer.char_count() > MAX_CHARS {
            self.lines.clear();
            return;
        }
        self.lines.truncate(n);
        let last = last.min(n - 1);

        let a = assets();
        let hl = Highlighter::new(&a.theme);
        // 上一行的行尾状态是否和旧缓存不同：不同则后面的行都必须重新解析。
        let mut dirty = false;

        for i in 0..=last {
            let text = buffer.line_content(i);
            let h = hash_line(&text);
            if !dirty {
                if let Some(e) = self.lines.get(i) {
                    if e.hash == h {
                        continue;
                    }
                }
            }

            let start = if i == 0 {
                Self::initial_state(syntax, &hl)
            } else {
                self.lines[i - 1].end.clone()
            };
            let (spans, end) = highlight_line(&text, start, &hl, &a.syntaxes);
            dirty = self.lines.get(i).map(|e| e.end.parse != end.parse || e.end.highlight != end.highlight).unwrap_or(true);
            let entry = LineEntry { hash: h, end, spans };
            if i < self.lines.len() {
                self.lines[i] = entry;
            } else {
                self.lines.push(entry);
            }
        }

        // 停在可视区域末尾时状态仍在变化：后面的旧缓存起始状态已失效，丢掉，
        // 等滚动到那里再重新解析。
        if dirty {
            self.lines.truncate(last + 1);
        }
    }

    /// 某行的着色区间；未启用高亮或该行尚未解析时返回 None。
    pub fn spans(&self, line: usize) -> Option<&[Span]> {
        self.syntax?;
        self.lines.get(line).map(|e| e.spans.as_slice())
    }
}

fn highlight_line(
    text: &str,
    mut state: LineState,
    hl: &Highlighter,
    ss: &SyntaxSet,
) -> (Vec<Span>, LineState) {
    if text.len() > MAX_LINE_BYTES {
        return (Vec::new(), state);
    }
    // extra_newlines 语法集要求每行带换行符（行注释、字符串跨行等规则依赖它）。
    let mut line = String::with_capacity(text.len() + 1);
    line.push_str(text);
    line.push('\n');

    let ops = match state.parse.parse_line(&line, ss) {
        Ok(ops) => ops,
        Err(_) => return (Vec::new(), state),
    };

    let mut spans = Vec::new();
    let mut pos = 0usize;
    for (style, piece) in HighlightIterator::new(&mut state.highlight, &ops, &line, hl) {
        let end = (pos + piece.len()).min(text.len());
        if pos < end {
            spans.push((pos, end, to_hsla(style.foreground)));
        }
        pos += piece.len();
    }
    (spans, state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_languages() {
        for name in [
            "main.rs", "a.py", "index.ts", "App.tsx", "Cargo.toml", "Cargo.lock", "Dockerfile",
            "build.bat", "README.md", "a.go", "a.java", "a.cpp", "a.h", "a.cs", "a.kt", "a.swift",
            "a.sql", "a.yaml", "a.json", "a.xml", "a.html", "a.css", "a.sh", "a.lua", "a.ini",
            "CMakeLists.txt", "Makefile",
        ] {
            assert!(detect_syntax(name, "").is_some(), "{name} 没有识别出语言");
        }
        assert!(detect_syntax("notes.txt", "").is_none());
        assert!(detect_syntax("run", "#!/usr/bin/env python3").is_some());
    }

    #[test]
    fn incremental_matches_full() {
        let src = "fn main() {\n    /* a\n b */ let x = \"s\";\n}\n";
        let mut buf = Buffer::new(src);
        let mut cache = SyntaxCache::new();
        cache.refresh("t.rs", &buf);
        cache.ensure_upto(&buf, 100);
        assert_eq!(cache.lines.len(), buf.line_count());

        // 改动第 1 行（打开块注释的那行），后面的行必须重新着色
        buf.replace(12..16, "//");
        cache.ensure_upto(&buf, 100);
        let mut fresh = SyntaxCache::new();
        fresh.refresh("t.rs", &buf);
        fresh.ensure_upto(&buf, 100);
        for i in 0..buf.line_count() {
            assert_eq!(cache.spans(i), fresh.spans(i), "第 {i} 行增量结果与全量不一致");
        }
    }
}
