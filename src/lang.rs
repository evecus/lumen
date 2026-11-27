//! 每种语言的编辑规则：缩进类型、行注释 / 块注释符号、自动配对的引号。
//!
//! 语言名来自语法高亮识别出的 syntect 语法名（约 190 种），这样高亮、缩进、
//! 注释切换、括号补全用的是同一套语言识别结果。

use crate::indent::Kind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LangSpec {
    pub kind: Kind,
    /// 行注释前缀，例如 `//`、`#`、`--`
    pub line_comment: Option<&'static str>,
    /// 块注释，例如 (`/*`, `*/`)、(`<!--`, `-->`)
    pub block_comment: Option<(&'static str, &'static str)>,
    /// 输入时自动配对的括号（开括号列表，对应闭括号见 [`closing_of`]）
    pub brackets: &'static str,
    /// 输入时自动配对的引号
    pub quotes: &'static str,
}

impl LangSpec {
    const fn new(kind: Kind) -> Self {
        Self {
            kind,
            line_comment: None,
            block_comment: None,
            brackets: "([{",
            quotes: "\"'",
        }
    }
    const fn line(mut self, c: &'static str) -> Self {
        self.line_comment = Some(c);
        self
    }
    const fn block(mut self, a: &'static str, b: &'static str) -> Self {
        self.block_comment = Some((a, b));
        self
    }
    const fn quotes(mut self, q: &'static str) -> Self {
        self.quotes = q;
        self
    }
    const fn brackets(mut self, b: &'static str) -> Self {
        self.brackets = b;
        self
    }
}

pub fn closing_of(c: char) -> Option<char> {
    Some(match c {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '<' => '>',
        '"' => '"',
        '\'' => '\'',
        '`' => '`',
        _ => return None,
    })
}

const C_LIKE: LangSpec = LangSpec::new(Kind::Brace).line("//").block("/*", "*/");
const HASH: LangSpec = LangSpec::new(Kind::Brace).line("#");
const PLAIN: LangSpec = LangSpec::new(Kind::Plain).brackets("").quotes("");

/// 根据 syntect 语法名（例如 "Rust"、"Python"、"Batch File"）和文件名选择规则。
/// `syntax_name` 为 None 表示没识别出语言（纯文本）。
pub fn spec_for(syntax_name: Option<&str>, file_name: &str) -> LangSpec {
    let Some(name) = syntax_name else {
        return fallback(file_name);
    };
    let n = name.to_ascii_lowercase();
    let has = |s: &str| n.contains(s);

    // ── 标记语言 ──
    if has("html") || has("xml") || has("vue") || has("svelte") || has("jsp") || has("asp")
        || has("astro") || n == "svg" || has("xaml") || has("nant") || has("plist")
    {
        return LangSpec::new(Kind::Markup).block("<!--", "-->");
    }
    if has("markdown") || has("mediawiki") || n == "asciidoc" || has("restructuredtext") || has("textile") || has("org mode") {
        let s = LangSpec::new(Kind::Markdown).brackets("([").quotes("`");
        return if has("markdown") { s.block("<!--", "-->") } else { s };
    }
    if has("latex") || n == "tex" || has("bibtex") {
        return LangSpec::new(Kind::Brace).line("%").quotes("$");
    }

    // ── 以缩进 / 冒号开块 ──
    if has("python") || n == "cython" || has("gdscript") || n == "starlark" || has("bazel") {
        return LangSpec::new(Kind::Python).line("#").quotes("\"'");
    }
    if n == "nim" {
        return LangSpec::new(Kind::Python).line("#").block("#[", "]#");
    }
    if has("yaml") || has("salt") {
        return LangSpec::new(Kind::Yaml).line("#");
    }
    if has("makefile") || n == "make" || n == "ninja" {
        return LangSpec::new(Kind::Makefile).line("#").quotes("\"'");
    }

    // ── 关键字开块 ──
    if has("bourne again") || has("bash") || n == "shell-unix-generic" || has("zsh") || n == "fish" || has("shell") {
        return LangSpec::new(Kind::Shell).line("#").quotes("\"'`");
    }
    if n == "lua" {
        return LangSpec::new(Kind::Keyword).line("--").block("--[[", "]]");
    }
    if has("ruby") || has("crystal") || has("rails") || has("haml") || has("slim") {
        return LangSpec::new(Kind::Keyword).line("#").block("=begin", "=end");
    }
    if has("elixir") {
        return LangSpec::new(Kind::Keyword).line("#");
    }
    if has("julia") {
        return LangSpec::new(Kind::Keyword).line("#").block("#=", "=#");
    }
    if has("pascal") || has("delphi") || n == "ada" || has("visual basic") || n == "vb" || has("fortran") || has("matlab") || has("vhdl") {
        let line = if has("pascal") || has("delphi") {
            "//"
        } else if n == "ada" || has("vhdl") {
            "--"
        } else if has("fortran") {
            "!"
        } else if has("matlab") {
            "%"
        } else {
            "'"
        };
        return LangSpec::new(Kind::Keyword).line(line).quotes("\"");
    }

    // ── 函数式 / 越位规则 ──
    if has("haskell") || n == "elm" || has("purescript") || n == "idris" || n == "agda" {
        return LangSpec::new(Kind::Offside).line("--").block("{-", "-}").quotes("\"");
    }
    if has("ocaml") || n == "f#" || n == "sml" || has("standard ml") || n == "reason" {
        let s = LangSpec::new(Kind::Offside).block("(*", "*)").quotes("\"");
        return if n == "f#" { s.line("//") } else { s };
    }
    if has("erlang") {
        return LangSpec::new(Kind::Offside).line("%").quotes("\"");
    }

    // ── Lisp 系 ──
    if has("lisp") || has("clojure") || has("racket") || has("scheme") || n == "fennel" || n == "hy" {
        return LangSpec::new(Kind::Brace).line(";").block("#|", "|#").quotes("\"");
    }

    // ── 大括号 / C 系 ──
    if n == "rust" {
        // Rust 的 ' 多是生命周期（'a），不自动配对
        return C_LIKE.quotes("\"");
    }
    if n.starts_with("javascript") || n.starts_with("typescript") || has("jsx") || has("tsx") || n == "coffeescript" || n == "livescript" || n == "go" || n == "kotlin" || n == "dart" || n == "groovy" || n == "graphql" {
        if n == "coffeescript" || n == "livescript" {
            return LangSpec::new(Kind::Python).line("#").block("###", "###").quotes("\"'`");
        }
        return C_LIKE.quotes("\"'`");
    }
    if n == "sql" || n.ends_with(" sql") || n.contains("sql") || n == "plsql" {
        return LangSpec::new(Kind::Brace).line("--").block("/*", "*/").quotes("'\"`");
    }
    if n == "css" || has("scss") || has("less") || has("stylus") || has("sass") {
        let s = C_LIKE;
        return if n == "css" { LangSpec::new(Kind::Brace).block("/*", "*/") } else { s };
    }
    if n == "json" || has("json") {
        return LangSpec::new(Kind::Brace).quotes("\"");
    }
    if has("toml") || has("ini") || has("properties") || has("dotenv") || n == "env" || has("editorconfig") || has("git config") || has("requirements") {
        let c = if has("ini") { ";" } else { "#" };
        return LangSpec::new(Kind::Brace).line(c).quotes("\"'");
    }
    if has("batch") || n == "dosbatch" {
        return LangSpec::new(Kind::Brace).line("REM").quotes("\"");
    }
    if has("powershell") {
        return HASH.block("<#", "#>").quotes("\"'");
    }
    if has("perl") || n == "r" || has("tcl") || has("cmake") || has("dockerfile") || has("nginx")
        || has("apache") || has("terraform") || has("hcl") || has("nix") || has("puppet") || has("awk")
        || has("crontab") || has("gitignore") || has("git attributes") || has("diff") || has("sed")
        || has("nsis") || has("cabal") || has("jq") || has("rego") || has("robot") || has("vyper")
    {
        let mut s = HASH;
        if has("nix") || has("terraform") || has("hcl") {
            s = s.block("/*", "*/");
        }
        if has("nsis") {
            s = s.line(";").block("/*", "*/");
        }
        if has("diff") {
            s = PLAIN;
        }
        return s;
    }
    if has("assembly") || has("asm") || has("nasm") || has("llvm") {
        return LangSpec::new(Kind::Brace).line(";").quotes("\"'");
    }
    if has("vim") {
        return LangSpec::new(Kind::Keyword).line("\"").quotes("'");
    }
    if n == "verilog" || has("systemverilog") || n == "solidity" || n == "zig" || n == "v" {
        return C_LIKE.quotes("\"");
    }
    if has("plain text") || n == "text" || has("log") || has("todo") || has("email") || has("csv") {
        return PLAIN;
    }

    // C / C++ / C# / Java / Swift / Scala / PHP / Objective-C / GLSL / Protobuf / … 都走这里
    C_LIKE
}

/// 没有识别出语言时，按扩展名兜底（主要是纯文本）。
fn fallback(file_name: &str) -> LangSpec {
    match crate::indent::kind_of(file_name) {
        Kind::Plain => PLAIN,
        k => LangSpec::new(k).line("#"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_comment_tokens() {
        assert_eq!(spec_for(Some("Rust"), "a.rs").line_comment, Some("//"));
        assert_eq!(spec_for(Some("Python"), "a.py").line_comment, Some("#"));
        assert_eq!(spec_for(Some("Python"), "a.py").kind, Kind::Python);
        assert_eq!(spec_for(Some("Lua"), "a.lua").line_comment, Some("--"));
        assert_eq!(spec_for(Some("SQL"), "a.sql").line_comment, Some("--"));
        assert_eq!(spec_for(Some("HTML"), "a.html").block_comment, Some(("<!--", "-->")));
        assert_eq!(spec_for(Some("Batch File"), "a.bat").line_comment, Some("REM"));
        assert_eq!(spec_for(Some("YAML"), "a.yml").kind, Kind::Yaml);
        assert_eq!(spec_for(Some("Makefile"), "Makefile").kind, Kind::Makefile);
        assert_eq!(spec_for(None, "a.txt"), PLAIN);
        assert!(!spec_for(Some("Rust"), "a.rs").quotes.contains('\''));
    }
}
