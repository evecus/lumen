//! 与界面无关的编辑算法：注释切换、括号匹配、单词边界、自动配对判断、显示列换算。
//! 全部是纯函数，便于单元测试；`workspace.rs` 负责把结果写进缓冲区。

use crate::lang::{closing_of, LangSpec};

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

// ───────────────────────── 注释 ─────────────────────────

/// 切换行注释。`text` 是若干完整行（用 `\n` 分隔，行尾可能带 `\r`）。
/// 所有非空行都已注释 → 取消注释；否则在最小缩进处统一加上 `token `。
pub fn toggle_line_comment(text: &str, token: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    fn body(l: &str) -> &str {
        l.trim_start_matches([' ', '\t']).trim_end_matches('\r')
    }
    let non_blank: Vec<&str> = lines.iter().copied().filter(|l| !l.trim().is_empty()).collect();
    if non_blank.is_empty() {
        return text.to_string();
    }
    let all_commented = non_blank.iter().all(|l| body(l).starts_with(token));

    if all_commented {
        lines
            .iter()
            .map(|l| {
                let ind = l.len() - l.trim_start_matches([' ', '\t']).len();
                let rest = &l[ind..];
                match rest.strip_prefix(token) {
                    Some(r) => {
                        let r = r.strip_prefix(' ').unwrap_or(r);
                        format!("{}{}", &l[..ind], r)
                    }
                    None => l.to_string(),
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        let min_ind = non_blank
            .iter()
            .map(|l| l.len() - l.trim_start_matches([' ', '\t']).len())
            .min()
            .unwrap_or(0);
        lines
            .iter()
            .map(|l| {
                if l.trim().is_empty() {
                    l.to_string()
                } else {
                    format!("{}{} {}", &l[..min_ind], token, &l[min_ind..])
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// 切换块注释：已被 `open … close` 包住（忽略首尾空白）则去掉，否则包上。
pub fn toggle_block_comment(text: &str, open: &str, close: &str) -> String {
    let lead = text.len() - text.trim_start().len();
    let trail = text.len() - text.trim_end().len();
    let inner = &text[lead..text.len() - trail];
    if let Some(body) = inner.strip_prefix(open).and_then(|r| r.strip_suffix(close)) {
        let body = body.strip_prefix(' ').unwrap_or(body);
        let body = body.strip_suffix(' ').unwrap_or(body);
        format!("{}{}{}", &text[..lead], body, &text[text.len() - trail..])
    } else {
        format!("{}{} {} {}{}", &text[..lead], open, inner, close, &text[text.len() - trail..])
    }
}

// ───────────────────────── 括号匹配 ─────────────────────────

fn bracket_partner(c: char) -> Option<(char, bool)> {
    // (配对字符, 是否向前查找)
    Some(match c {
        '(' => (')', true),
        '[' => (']', true),
        '{' => ('}', true),
        ')' => ('(', false),
        ']' => ('[', false),
        '}' => ('{', false),
        _ => return None,
    })
}

/// 在 `chars`（文本的一段窗口，`base` 是窗口起点在全文中的字符索引）里，
/// 为光标 `cursor` 旁边的括号找配对括号。优先看光标右边的字符，其次左边。
/// 返回两个括号在全文中的字符索引。
pub fn match_bracket(chars: &[char], base: usize, cursor: usize) -> Option<(usize, usize)> {
    let local = cursor.checked_sub(base)?;
    let candidates = [Some(local), local.checked_sub(1)];
    for pos in candidates.into_iter().flatten() {
        let Some(&c) = chars.get(pos) else { continue };
        let Some((partner, forward)) = bracket_partner(c) else { continue };
        let mut depth = 0i32;
        if forward {
            for (i, &d) in chars.iter().enumerate().skip(pos) {
                if d == c {
                    depth += 1;
                } else if d == partner {
                    depth -= 1;
                    if depth == 0 {
                        return Some((base + pos, base + i));
                    }
                }
            }
        } else {
            for i in (0..=pos).rev() {
                let d = chars[i];
                if d == c {
                    depth += 1;
                } else if d == partner {
                    depth -= 1;
                    if depth == 0 {
                        return Some((base + pos, base + i));
                    }
                }
            }
        }
        return None;
    }
    None
}

// ───────────────────────── 单词 ─────────────────────────

/// 一行里所有完整单词 `needle` 的出现位置（字符索引区间）。
pub fn word_occurrences(line: &str, needle: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let chars: Vec<char> = line.chars().collect();
    let nd: Vec<char> = needle.chars().collect();
    let n = nd.len();
    if chars.len() < n {
        return out;
    }
    let mut i = 0;
    while i + n <= chars.len() {
        if chars[i..i + n] == nd[..]
            && (i == 0 || !is_word_char(chars[i - 1]))
            && (i + n == chars.len() || !is_word_char(chars[i + n]))
        {
            out.push((i, i + n));
            i += n;
        } else {
            i += 1;
        }
    }
    out
}

/// 光标所在（或紧挨着）的单词，返回行内字符区间。
pub fn word_at(line: &str, col: usize) -> Option<(usize, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let col = col.min(chars.len());
    let mut s = col;
    while s > 0 && is_word_char(chars[s - 1]) {
        s -= 1;
    }
    let mut e = col;
    while e < chars.len() && is_word_char(chars[e]) {
        e += 1;
    }
    (s < e).then_some((s, e))
}

// ───────────────────────── 自动配对 ─────────────────────────

#[derive(Debug, PartialEq, Eq)]
pub enum PairAction {
    /// 不处理，按普通输入
    None,
    /// 光标后面正好是这个闭合字符：直接跳过它
    Skip,
    /// 插入一对，光标放在中间
    Pair(char),
    /// 有选区：用这对符号把选区包起来
    Wrap(char),
}

/// 输入单个字符 `typed` 时是否自动配对。`prev` / `next` 是光标前后的字符。
pub fn pair_action(spec: &LangSpec, typed: char, prev: Option<char>, next: Option<char>, has_selection: bool) -> PairAction {
    let is_bracket = spec.brackets.contains(typed);
    let is_quote = spec.quotes.contains(typed);
    let is_closer = matches!(typed, ')' | ']' | '}') && spec.brackets.contains(match typed {
        ')' => '(',
        ']' => '[',
        _ => '{',
    });

    if has_selection {
        return if is_bracket || is_quote {
            PairAction::Wrap(closing_of(typed).unwrap_or(typed))
        } else {
            PairAction::None
        };
    }
    if (is_closer || is_quote) && next == Some(typed) {
        return PairAction::Skip;
    }
    // 后面紧跟着字母数字时不配对（例如在 `foo` 前面输入 `(`）
    let next_ok = match next {
        None => true,
        Some(c) => c.is_whitespace() || matches!(c, ')' | ']' | '}' | ',' | ';' | ':' | '>' | '.'),
    };
    if is_bracket && next_ok {
        return PairAction::Pair(closing_of(typed).unwrap_or(typed));
    }
    if is_quote && next_ok {
        // 前一个字符是字母数字或同种引号时不配对：don't、x'、""" …
        let prev_ok = match prev {
            None => true,
            Some(c) => !is_word_char(c) && c != typed && c != '\\',
        };
        if prev_ok {
            return PairAction::Pair(typed);
        }
    }
    PairAction::None
}

/// 退格时光标正好在一对空符号中间（`(|)`、`"|"`）：两个一起删。
pub fn deletes_pair(spec: &LangSpec, prev: Option<char>, next: Option<char>) -> bool {
    match (prev, next) {
        (Some(p), Some(n)) => {
            (spec.brackets.contains(p) || spec.quotes.contains(p)) && closing_of(p) == Some(n)
        }
        _ => false,
    }
}

// ───────────────────────── Tab 展开 ─────────────────────────

/// 一行文本的显示形式：Tab 展开成空格对齐到制表位。
/// `char_disp[k]` 是第 k 个字符在显示文本里的字节偏移（末尾多一项 = 显示文本长度），
/// `char_orig[k]` 是它在原文本里的字节偏移。
pub struct DispLine {
    pub text: String,
    pub char_disp: Vec<usize>,
    pub char_orig: Vec<usize>,
}

impl DispLine {
    pub fn new(line: &str, tab_w: usize) -> Self {
        let tab_w = tab_w.max(1);
        let mut text = String::with_capacity(line.len());
        let mut char_disp = Vec::with_capacity(line.len() + 1);
        let mut char_orig = Vec::with_capacity(line.len() + 1);
        let mut col = 0usize;
        for (ob, ch) in line.char_indices() {
            char_disp.push(text.len());
            char_orig.push(ob);
            if ch == '\t' {
                let n = tab_w - col % tab_w;
                for _ in 0..n {
                    text.push(' ');
                }
                col += n;
            } else {
                text.push(ch);
                col += 1;
            }
        }
        char_disp.push(text.len());
        char_orig.push(line.len());
        Self {
            text,
            char_disp,
            char_orig,
        }
    }

    pub fn chars(&self) -> usize {
        self.char_disp.len() - 1
    }

    /// 字符索引 → 显示字节偏移
    pub fn disp(&self, k: usize) -> usize {
        self.char_disp[k.min(self.chars())]
    }

    /// 显示列（每个显示字节 = 一列；只用于 ASCII 缩进与 Tab 计算）
    pub fn col(&self, k: usize) -> usize {
        self.text[..self.disp(k)].chars().count()
    }

    /// 原文本字节偏移 → 显示字节偏移（用于语法高亮区间）
    pub fn orig_to_disp(&self, b: usize) -> usize {
        let k = self.char_orig.partition_point(|&o| o < b);
        self.disp(k)
    }

    /// 显示字节偏移 → 最近的字符索引（点在 Tab 展开出的空白中间时就近取整）
    pub fn disp_to_char(&self, b: usize) -> usize {
        let n = self.chars();
        let k = self.char_disp.partition_point(|&d| d <= b).saturating_sub(1).min(n);
        if k < n {
            let (a, c) = (self.char_disp[k], self.char_disp[k + 1]);
            if c - a > 1 && (b - a) * 2 >= c - a {
                return k + 1;
            }
        }
        k
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indent::Kind;
    use crate::lang::spec_for;

    #[test]
    fn line_comments() {
        assert_eq!(toggle_line_comment("    a\n\n      b", "//"), "    // a\n\n    //   b");
        assert_eq!(toggle_line_comment("    // a\n\n    //   b", "//"), "    a\n\n      b");
        assert_eq!(toggle_line_comment("# x\r\ny\r", "#"), "# # x\r\n# y\r");
    }

    #[test]
    fn block_comments() {
        assert_eq!(toggle_block_comment("  a b ", "/*", "*/"), "  /* a b */ ");
        assert_eq!(toggle_block_comment("/* a b */", "/*", "*/"), "a b");
    }

    #[test]
    fn brackets() {
        let t: Vec<char> = "f(a[1], {b})".chars().collect();
        assert_eq!(match_bracket(&t, 0, 1), Some((1, 11)));
        assert_eq!(match_bracket(&t, 0, 12), Some((11, 1)));
        assert_eq!(match_bracket(&t, 0, 8), Some((8, 10)));
        // 窗口不从 0 开始：返回的是全文索引
        assert_eq!(match_bracket(&t, 10, 13), Some((13, 15)));
    }

    #[test]
    fn words() {
        assert_eq!(word_occurrences("foo foobar foo_x foo", "foo"), vec![(0, 3), (17, 20)]);
        assert_eq!(word_at("let abc = 1", 5), Some((4, 7)));
        assert_eq!(word_at("a  b", 2), None);
    }

    #[test]
    fn pairs() {
        let rs = spec_for(Some("Rust"), "a.rs");
        let py = spec_for(Some("Python"), "a.py");
        assert_eq!(pair_action(&rs, '(', None, None, false), PairAction::Pair(')'));
        assert_eq!(pair_action(&rs, '(', None, Some('x'), false), PairAction::None);
        assert_eq!(pair_action(&rs, ')', None, Some(')'), false), PairAction::Skip);
        assert_eq!(pair_action(&rs, '\'', None, None, false), PairAction::None);
        assert_eq!(pair_action(&py, '\'', Some('n'), None, false), PairAction::None);
        assert_eq!(pair_action(&py, '"', Some(' '), None, false), PairAction::Pair('"'));
        assert_eq!(pair_action(&py, '[', None, None, true), PairAction::Wrap(']'));
        assert!(deletes_pair(&rs, Some('{'), Some('}')));
        assert_eq!(spec_for(None, "a.txt").kind, Kind::Plain);
        assert_eq!(pair_action(&spec_for(None, "a.txt"), '(', None, None, false), PairAction::None);
    }

    #[test]
    fn tabs() {
        let d = DispLine::new("\ta\tb", 4);
        assert_eq!(d.text, "    a   b");
        assert_eq!(d.disp(1), 4);
        assert_eq!(d.disp(3), 8);
        assert_eq!(d.disp_to_char(1), 0);
        assert_eq!(d.disp_to_char(3), 1);
        assert_eq!(d.orig_to_disp(2), 5);
        assert_eq!(d.col(4), 9);
    }
}
