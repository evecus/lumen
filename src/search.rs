//! 查找 / 替换逻辑（移植自原 `SearchPanel.svelte` 的查询构建与计数）。
//!
//! 与 CodeMirror 不同，Lumen 的查找是"全文档一次性计算匹配"，因此这里把匹配计算
//! 与替换实现成纯函数，UI 层只负责调用并把结果画成高亮。

use regex::Regex;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SearchMode {
    /// 普通字面量匹配
    Normal,
    /// 扩展转义：支持 \n \r \t \0 \xHH
    Extended,
    /// 正则表达式
    Regex,
}

#[derive(Clone, Debug)]
pub struct SearchQuery {
    pub pattern: String,
    pub replace: String,
    pub mode: SearchMode,
    pub match_case: bool,
    pub whole_word: bool,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            replace: String::new(),
            mode: SearchMode::Normal,
            match_case: false,
            whole_word: false,
        }
    }
}

impl SearchQuery {
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.pattern.is_empty()
    }

    pub fn is_regex(&self) -> bool {
        self.mode == SearchMode::Regex
    }

    pub fn is_extended(&self) -> bool {
        self.mode == SearchMode::Extended
    }
}

/// 把扩展模式里的转义序列替换成真实字符（\n \r \t \0 \xHH）。
fn expand_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('0') => out.push('\0'),
                Some('x') => {
                    let h: String = chars.by_ref().take(2).collect();
                    if h.len() == 2 {
                        if let Ok(code) = u8::from_str_radix(&h, 16) {
                            out.push(code as char);
                        } else {
                            out.push_str("\\x");
                            out.push_str(&h);
                        }
                    } else {
                        out.push_str("\\x");
                        out.push_str(&h);
                    }
                }
                Some(other) => out.push(other),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 把查询编译成一条 regex。字面量模式会先 `regex::escape` 再按需加 `(?i)` 与 `\b`。
fn compile(q: &SearchQuery) -> Option<Regex> {
    if q.pattern.is_empty() {
        return None;
    }
    let lit = if q.is_extended() {
        expand_escapes(&q.pattern)
    } else {
        q.pattern.clone()
    };

    let body = if q.is_regex() {
        lit
    } else {
        let mut s = regex::escape(&lit);
        if q.whole_word {
            s = format!(r"\b{}\b", s);
        }
        s
    };

    let prefixed = if q.match_case { body } else { format!("(?i){}", body) };
    Regex::new(&prefixed).ok()
}

/// 在 `text` 里找出所有匹配，返回「字符索引」区间 `[start, end)`。
/// 字符索引与 ropey 的字符序号一致（都是 Unicode 标量值计数）。
pub fn find_matches(text: &str, q: &SearchQuery) -> Vec<(usize, usize)> {
    let Some(re) = compile(q) else {
        return Vec::new();
    };
    let mut matches = Vec::new();
    for m in re.find_iter(text) {
        let cs = text[..m.start()].chars().count();
        let ce = text[..m.end()].chars().count();
        matches.push((cs, ce));
    }
    matches
}

/// 用于单处替换：给定某个字符区间，返回替换后的「字符区间」与替换文本字节区间。
/// 这里只需要返回替换后的整段新文本即可。
pub fn replace_all(text: &str, q: &SearchQuery) -> Option<(String, usize)> {
    let re = compile(q)?;
    let count = re.find_iter(text).count();
    if count == 0 {
        return Some((text.to_string(), 0));
    }
    // 非正则模式下，替换串里的 $ 不应被当作捕获组引用，先转义。
    let repl = if q.is_regex() {
        q.replace.clone()
    } else {
        q.replace.replace('$', "$$")
    };
    let new = re.replace_all(text, repl.as_str()).into_owned();
    Some((new, count))
}

/// 替换单个匹配（字符区间）。返回替换后的文本。
pub fn replace_one(text: &str, q: &SearchQuery, range: (usize, usize)) -> Option<String> {
    let re = compile(q)?;
    let start_byte = text.char_indices().nth(range.0).map(|(b, _)| b).unwrap_or(0);
    let end_byte = text
        .char_indices()
        .nth(range.1)
        .map(|(b, _)| b)
        .unwrap_or(text.len());
    if start_byte >= end_byte {
        return None;
    }
    let repl = if q.is_regex() {
        q.replace.clone()
    } else {
        q.replace.replace('$', "$$")
    };
    let m = re.find(&text[start_byte..end_byte])?;
    // 只替换该段内的首个匹配
    let (ms, me) = (start_byte + m.start(), start_byte + m.end());
    let mut new = String::with_capacity(text.len());
    new.push_str(&text[..ms]);
    new.push_str(&re.replace(&text[ms..me], repl.as_str()));
    new.push_str(&text[me..]);
    Some(new)
}
