//! 自动缩进：回车保持 / 增加 / 减少缩进，Tab / Shift+Tab 缩进整块，
//! 退格一次删一级缩进，输入 `}` `]` `)` 时自动回退一级。
//!
//! 这里只放与 UI 无关的纯函数，便于单元测试；`workspace.rs` 负责把结果写进缓冲区。

use std::path::Path;

use crate::buffer::Buffer;

/// 根据文件名判断的语言类别（只区分缩进规则不同的几类）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// C / Rust / Java / JS / Go … 大括号类语言（默认）
    Brace,
    /// Python：行尾 `:` 增加缩进，`return` / `pass` … 之后减少
    Python,
    /// YAML：行尾 `:` 增加缩进
    Yaml,
    /// HTML / XML / Vue …：开始标签后增加缩进
    Markup,
    /// Makefile：目标行（行尾 `:`）后用 Tab 缩进
    Makefile,
    /// Shell：`then` / `do` 之后增加缩进
    Shell,
    /// Lua / Ruby：`then` / `do` / `function` 之后增加缩进
    Keyword,
    /// Markdown：列表自动续写
    Markdown,
    /// Haskell / OCaml / F# / Erlang：行尾 `=` `->` `do` `where` … 之后增加缩进
    Offside,
    /// 纯文本：只保持上一行缩进
    Plain,
}

fn ext_and_name(file_name: &str) -> (String, String) {
    let p = Path::new(file_name);
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(file_name)
        .to_ascii_lowercase();
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    (ext, name)
}

pub fn kind_of(file_name: &str) -> Kind {
    let (ext, name) = ext_and_name(file_name);
    if matches!(name.as_str(), "makefile" | "gnumakefile") || matches!(ext.as_str(), "mk" | "mak") {
        return Kind::Makefile;
    }
    match ext.as_str() {
        "py" | "pyw" | "pyi" | "gd" | "nim" => Kind::Python,
        "yaml" | "yml" => Kind::Yaml,
        "html" | "htm" | "xhtml" | "xml" | "xaml" | "svg" | "vue" | "svelte" | "jsx" | "tsx"
        | "csproj" | "vcxproj" | "props" | "targets" | "plist" | "xsl" | "xslt" => Kind::Markup,
        "sh" | "bash" | "zsh" | "ksh" => Kind::Shell,
        "lua" | "rb" | "rake" | "ex" | "exs" => Kind::Keyword,
        "md" | "markdown" => Kind::Markdown,
        "txt" | "log" | "" => Kind::Plain,
        _ => Kind::Brace,
    }
}

/// 没法从内容判断时，各语言常用的缩进单位。
fn default_unit(file_name: &str) -> String {
    let (ext, _) = ext_and_name(file_name);
    if kind_of(file_name) == Kind::Makefile || matches!(ext.as_str(), "go") {
        return "\t".into();
    }
    let w = match ext.as_str() {
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "json" | "jsonc" | "json5" | "vue"
        | "svelte" | "html" | "htm" | "xml" | "xaml" | "svg" | "css" | "scss" | "less" | "yaml"
        | "yml" | "rb" | "dart" | "nix" | "tf" | "md" | "markdown" | "ex" | "exs" | "lua" => 2,
        _ => 4,
    };
    " ".repeat(w)
}

/// 从文件现有内容推断缩进单位（Tab 或 2/3/4/8 个空格），推断不出时按语言默认。
pub fn indent_unit(buf: &Buffer, file_name: &str) -> String {
    let n = buf.line_count().min(2000);
    let mut tab_lines = 0usize;
    let mut space_lines = 0usize;
    let mut deltas = [0usize; 9];
    let mut prev = 0usize;
    for i in 0..n {
        let l = buf.line_content(i);
        if l.trim().is_empty() {
            continue;
        }
        if l.starts_with('\t') {
            tab_lines += 1;
            prev = 0;
            continue;
        }
        let sp = l.chars().take_while(|c| *c == ' ').count();
        if sp > 0 {
            space_lines += 1;
        }
        if sp > prev && sp - prev <= 8 {
            deltas[sp - prev] += 1;
        }
        prev = sp;
    }
    if tab_lines > 0 && tab_lines >= space_lines {
        return "\t".into();
    }
    // 1 个空格的增量多半来自 ` * ` 文档注释对齐，不算。平票时优先 4。
    let best = [4usize, 2, 8, 3]
        .into_iter()
        .max_by_key(|&w| (deltas[w], w == 4))
        .unwrap_or(4);
    if deltas[best] > 0 {
        " ".repeat(best)
    } else {
        default_unit(file_name)
    }
}

/// 缩进单位的显示宽度（Tab 按 4 计）。
fn unit_width(unit: &str) -> usize {
    if unit == "\t" {
        4
    } else {
        unit.len().max(1)
    }
}

fn leading_ws(s: &str) -> &str {
    let end = s.len() - s.trim_start_matches([' ', '\t']).len();
    &s[..end]
}

/// 去掉一级缩进。
pub fn dedent_once(indent: &str, unit: &str) -> String {
    if let Some(s) = indent.strip_suffix(unit) {
        return s.to_string();
    }
    if let Some(s) = indent.strip_suffix('\t') {
        return s.to_string();
    }
    let w = unit_width(unit);
    let spaces = indent.len() - indent.trim_end_matches(' ').len();
    indent[..indent.len() - spaces.min(w)].to_string()
}

/// 回车要做的编辑：把 [光标 - back, 选区末尾 + fwd) 替换成 insert，
/// 光标放到替换起点之后 caret 个字符处。
#[derive(Debug, PartialEq, Eq)]
pub struct EnterEdit {
    pub back: usize,
    pub fwd: usize,
    pub insert: String,
    pub caret: usize,
}

const VOID_TAGS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source",
    "track", "wbr", "param", "keygen",
];

/// 行尾是否是一个未闭合的开始标签，例如 `<div class="a">`。
fn opens_tag(line: &str) -> bool {
    let t = line.trim_end();
    if !t.ends_with('>') || t.ends_with("/>") || t.ends_with("-->") || t.ends_with("?>") {
        return false;
    }
    let Some(lt) = t.rfind('<') else { return false };
    let tag = &t[lt + 1..];
    if tag.starts_with(['/', '!', '?']) {
        return false;
    }
    let name: String = tag
        .chars()
        .take_while(|c| c.is_alphanumeric() || matches!(c, '-' | ':' | '.'))
        .collect::<String>()
        .to_ascii_lowercase();
    !name.is_empty() && !VOID_TAGS.contains(&name.as_str())
}

fn ends_with_word(t: &str, words: &[&str]) -> bool {
    words.iter().any(|w| {
        t.strip_suffix(w)
            .is_some_and(|rest| rest.is_empty() || !rest.ends_with(|c: char| c.is_alphanumeric() || c == '_'))
    })
}

fn starts_with_word(t: &str, words: &[&str]) -> bool {
    words.iter().any(|w| {
        t.strip_prefix(w)
            .is_some_and(|rest| rest.is_empty() || !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
    })
}

/// 去掉行尾注释后的有效内容（粗略：Python `#`、C 类 `//`），用来判断行尾是不是 `:` / `{`。
fn strip_line_comment(t: &str, kind: Kind) -> &str {
    let marker = match kind {
        Kind::Python | Kind::Yaml | Kind::Shell | Kind::Makefile => "#",
        Kind::Brace | Kind::Markup => "//",
        Kind::Keyword => "--",
        _ => return t,
    };
    // 引号里的注释符不算：只在引号平衡的位置截断
    // 按字符遍历（char_indices 保证切片落在字符边界上，中文注释不会 panic）
    let mut in_s = false;
    let mut in_d = false;
    let mut escaped = false;
    for (i, c) in t.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '\'' if !in_d => in_s = !in_s,
            '"' if !in_s => in_d = !in_d,
            _ if !in_s && !in_d && t[i..].starts_with(marker) => return t[..i].trim_end(),
            _ => {}
        }
    }
    t
}

/// Markdown 列表标记：返回 (标记文本含尾随空格, 下一项的标记)。
fn md_list_marker(rest: &str) -> Option<(String, String)> {
    for b in ["- [ ] ", "- [x] ", "* [ ] ", "- ", "* ", "+ "] {
        if rest.starts_with(b) {
            let next = if b.contains('[') { format!("{}[ ] ", &b[..2]) } else { b.to_string() };
            return Some((b.to_string(), next));
        }
    }
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && digits.len() <= 9 {
        let after = &rest[digits.len()..];
        for sep in [". ", ") "] {
            if after.starts_with(sep) {
                let n: u64 = digits.parse().unwrap_or(0);
                return Some((format!("{digits}{sep}"), format!("{}{sep}", n + 1)));
            }
        }
    }
    None
}

/// 计算回车的编辑。`before` 是光标所在行光标之前的文字，`after` 是选区末尾所在行
/// 选区末尾之后的文字。
pub fn on_enter(before: &str, after: &str, unit: &str, file_name: &str) -> EnterEdit {
    on_enter_kind(before, after, unit, kind_of(file_name))
}

/// 括号是否未闭合（忽略引号里的内容），例如 `foo(a,` 或 `if (x) {  // c`。
fn unbalanced_open(code: &str) -> bool {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in code.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if let Some(q) = quote {
            match c {
                '\\' => escaped = true,
                _ if c == q => quote = None,
                _ => {}
            }
            continue;
        }
        match c {
            '"' | '\'' | '`' => quote = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    depth > 0
}

/// 回车时，如果当前行是「收尾」行（`end`、`else`、`fi`、`</div>`、Python 的 `else:` …），
/// 而它的缩进还和上一行一样深，应先把当前行退回一级。
pub fn is_dedent_line(content: &str, kind: Kind) -> bool {
    let t = content.trim();
    match kind {
        Kind::Python => {
            t.ends_with(':') && starts_with_word(t, &["else", "elif", "except", "finally", "case"])
        }
        Kind::Keyword => {
            starts_with_word(t, &["end", "else", "elsif", "elseif", "rescue", "ensure", "when", "until"])
        }
        Kind::Shell => starts_with_word(t, &["fi", "done", "esac", "else", "elif"]) || t == "}",
        Kind::Markup => t.starts_with("</"),
        Kind::Offside => starts_with_word(t, &["in", "else", "end"]),
        _ => false,
    }
}

pub fn on_enter_kind(before: &str, after: &str, unit: &str, kind: Kind) -> EnterEdit {
    let indent = leading_ws(before).to_string();
    let before_trim = before.trim_end();
    let after_trim = after.trim_start_matches([' ', '\t']);
    // 离开的这一行去掉行尾空白；光标还在缩进里（整行空白）时不动它。
    let back = if before_trim.is_empty() {
        0
    } else {
        before.chars().count() - before_trim.chars().count()
    };
    let fwd = after.chars().count() - after_trim.chars().count();
    let content = before_trim.trim_start_matches([' ', '\t']);

    let simple = |ins: String| EnterEdit {
        back,
        fwd,
        caret: ins.chars().count(),
        insert: ins,
    };

    // ── Markdown 列表续写；空项再回车则结束列表 ──
    if kind == Kind::Markdown {
        if let Some((marker, next)) = md_list_marker(&before[indent.len()..]) {
            if content == marker.trim_end() && after_trim.is_empty() {
                // 空列表项：删掉标记，保留缩进，不换行
                return EnterEdit {
                    back: before.chars().count() - indent.chars().count(),
                    fwd,
                    insert: String::new(),
                    caret: 0,
                };
            }
            return simple(format!("\n{indent}{next}"));
        }
    }

    // ── C 类块注释续写：/** … 和 * … ──
    if matches!(kind, Kind::Brace | Kind::Markup) && !content.contains("*/") {
        if content.starts_with("/*") {
            return simple(format!("\n{indent} * "));
        }
        if content.starts_with("* ") || content == "*" {
            return simple(format!("\n{indent}* "));
        }
    }

    let code = strip_line_comment(content, kind);
    let last = code.chars().last();
    let mut increase = matches!(last, Some('{' | '[' | '('));
    if kind != Kind::Plain && kind != Kind::Markdown {
        increase |= unbalanced_open(code);
    }
    increase |= match kind {
        Kind::Python | Kind::Yaml | Kind::Makefile => last == Some(':'),
        Kind::Markup => opens_tag(code),
        Kind::Shell => ends_with_word(code, &["then", "do", "else", "in"]) || last == Some('{'),
        Kind::Offside => {
            code.ends_with('=') && !code.ends_with("==")
                || code.ends_with("->")
                || code.ends_with("=>")
                || ends_with_word(code, &["do", "where", "of", "then", "else", "let", "in", "begin", "struct", "sig"])
        }
        // C / Java / JS 的 switch：`case x:` / `default:` 之后缩进
        Kind::Brace => last == Some(':') && starts_with_word(code, &["case", "default"]),
        Kind::Keyword => {
            ends_with_word(code, &["then", "do", "else", "begin"])
                || starts_with_word(code, &["function", "local function", "def", "class", "module", "if", "unless", "while", "for", "case"])
                    && !ends_with_word(code, &["end"])
        }
        _ => false,
    };
    if kind == Kind::Yaml && (code.ends_with(": |") || code.ends_with(": >")) {
        increase = true;
    }

    let unit_here = if kind == Kind::Makefile { "\t" } else { unit };

    // 光标后紧跟对应的闭合符号：{|} → 三行，光标在中间那行
    let closes = match last {
        Some('{') => after_trim.starts_with('}'),
        Some('[') => after_trim.starts_with(']'),
        Some('(') => after_trim.starts_with(')'),
        _ => kind == Kind::Markup && increase && after_trim.starts_with("</"),
    };

    if increase && closes {
        let inner = format!("\n{indent}{unit_here}");
        let caret = inner.chars().count();
        return EnterEdit {
            back,
            fwd,
            insert: format!("{inner}\n{indent}"),
            caret,
        };
    }
    if increase {
        return simple(format!("\n{indent}{unit_here}"));
    }

    // Python：return / pass / break / continue / raise 之后回退一级
    if kind == Kind::Python && starts_with_word(content, &["return", "pass", "break", "continue", "raise"]) {
        return simple(format!("\n{}", dedent_once(&indent, unit)));
    }

    simple(format!("\n{indent}"))
}

/// 光标前（同一行）全是空白时，输入闭合符号应删掉多少个字符（回退一级）。
pub fn closer_dedent(before: &str, unit: &str) -> usize {
    if before.is_empty() || !before.chars().all(|c| c == ' ' || c == '\t') {
        return 0;
    }
    before.chars().count() - dedent_once(before, unit).chars().count()
}

/// 退格：光标前全是空格时，一次删到上一个缩进位置。返回要删的字符数（0 = 按普通退格）。
pub fn smart_backspace(before: &str, unit: &str) -> usize {
    if unit == "\t" || before.is_empty() || !before.chars().all(|c| c == ' ') {
        return 0;
    }
    let w = unit_width(unit);
    let col = before.chars().count();
    match col % w {
        0 => w,
        r => r,
    }
}

/// 整块缩进 / 反缩进。`text` 是若干完整行（可能含 `\r\n`）。空行不加缩进。
pub fn shift_lines(text: &str, unit: &str, indent: bool) -> String {
    let w = unit_width(unit);
    text.split('\n')
        .map(|line| {
            if indent {
                if line.trim().is_empty() {
                    line.to_string()
                } else {
                    format!("{unit}{line}")
                }
            } else if let Some(rest) = line.strip_prefix('\t') {
                rest.to_string()
            } else {
                let sp = line.len() - line.trim_start_matches(' ').len();
                line[sp.min(w)..].to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ins(before: &str, after: &str, file: &str) -> String {
        on_enter(before, after, "    ", file).insert
    }

    #[test]
    fn keeps_indent() {
        assert_eq!(ins("    let a = 1;", "", "a.rs"), "\n    ");
        assert_eq!(ins("\tfoo", "", "a.c"), "\n\t");
        assert_eq!(ins("  hello", "", "a.txt"), "\n  ");
    }

    #[test]
    fn increases_after_opener() {
        assert_eq!(ins("fn main() {", "", "a.rs"), "\n    ");
        assert_eq!(ins("    let v = [", "", "a.js"), "\n        ");
        assert_eq!(ins("if x:", "", "a.py"), "\n    ");
        assert_eq!(ins("def f():  # 注释", "", "a.py"), "\n    ");
        assert_eq!(ins("key:", "", "a.yaml"), "\n    ");
        assert_eq!(ins("<div class=\"a\">", "", "a.html"), "\n    ");
        assert_eq!(ins("<br>", "", "a.html"), "\n");
        assert_eq!(ins("<p>text</p>", "", "a.html"), "\n");
        assert_eq!(ins("if [ -f x ]; then", "", "a.sh"), "\n    ");
        assert_eq!(on_enter("all:", "", "    ", "Makefile").insert, "\n\t");
    }

    #[test]
    fn splits_brace_pair() {
        let e = on_enter("  fn a() {", "}", "    ", "a.rs");
        assert_eq!(e.insert, "\n      \n  ");
        assert_eq!(e.caret, 7);
        let e = on_enter("<div>", "</div>", "  ", "a.vue");
        assert_eq!(e.insert, "\n  \n");
    }

    #[test]
    fn unbalanced_and_offside() {
        assert_eq!(ins("    foo(a,", "", "a.rs"), "\n        ");
        assert_eq!(ins("let s = \"(\";", "", "a.rs"), "\n");
        assert_eq!(on_enter_kind("f x =", "", "  ", Kind::Offside).insert, "\n  ");
        assert_eq!(on_enter_kind("    case 1:", "", "    ", Kind::Brace).insert, "\n        ");
        assert!(is_dedent_line("    else:", Kind::Python));
        assert!(is_dedent_line("  end", Kind::Keyword));
        assert!(is_dedent_line("  </div>", Kind::Markup));
        assert!(!is_dedent_line("  endpoint = 1", Kind::Keyword));
    }

    #[test]
    fn python_dedent_after_return() {
        assert_eq!(ins("        return x", "", "a.py"), "\n    ");
    }

    #[test]
    fn strips_trailing_whitespace() {
        let e = on_enter("    foo   ", "  bar", "    ", "a.rs");
        assert_eq!((e.back, e.fwd), (3, 2));
    }

    #[test]
    fn markdown_lists() {
        assert_eq!(ins("- item", "", "a.md"), "\n- ");
        assert_eq!(ins("  9. item", "", "a.md"), "\n  10. ");
        let e = on_enter("- ", "", "  ", "a.md");
        assert_eq!((e.back, e.insert.as_str()), (2, ""));
    }

    #[test]
    fn doc_comments() {
        assert_eq!(ins("    /**", "", "a.java"), "\n     * ");
        assert_eq!(ins("     * text", "", "a.java"), "\n     * ");
    }

    #[test]
    fn detects_unit() {
        let b = Buffer::new("a {\n  b {\n    c\n  }\n}\n");
        assert_eq!(indent_unit(&b, "a.c"), "  ");
        let b = Buffer::new("a {\n\tb\n}\n");
        assert_eq!(indent_unit(&b, "a.c"), "\t");
        let b = Buffer::new("");
        assert_eq!(indent_unit(&b, "a.go"), "\t");
        assert_eq!(indent_unit(&b, "a.ts"), "  ");
        assert_eq!(indent_unit(&b, "a.rs"), "    ");
    }

    #[test]
    fn closer_and_backspace() {
        assert_eq!(closer_dedent("        ", "    "), 4);
        assert_eq!(closer_dedent("    x", "    "), 0);
        assert_eq!(smart_backspace("      ", "    "), 2);
        assert_eq!(smart_backspace("        ", "    "), 4);
        assert_eq!(smart_backspace("  a", "    "), 0);
    }

    #[test]
    fn shifts_blocks() {
        assert_eq!(shift_lines("a\r\n\r\n  b", "  ", true), "  a\r\n\r\n    b");
        assert_eq!(shift_lines("    a\n  b\n\tc", "    ", false), "a\nb\nc");
    }
}
