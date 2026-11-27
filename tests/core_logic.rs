//! 核心逻辑单元测试：Buffer / search / file_ops。

use lumen::buffer::Buffer;
use lumen::file_ops::{decode_bytes, detect_eol, looks_binary, normalize_eol, read_text_file, write_text_file};
use lumen::search::{find_matches, replace_all, replace_one, SearchMode, SearchQuery};

// ===== Buffer =====

#[test]
fn buffer_insert_undo_redo() {
    let mut b = Buffer::new("");
    b.insert(0, "hello");
    b.insert(5, " world");
    assert_eq!(b.to_string(), "hello world");
    assert_eq!(b.char_count(), 11);
    // 连续插入会合并为一条撤销记录，一次撤销回到空串
    b.undo();
    assert_eq!(b.to_string(), "");
    b.redo();
    assert_eq!(b.to_string(), "hello world");
}

#[test]
fn buffer_remove_and_undo() {
    let mut b = Buffer::new("abcdef");
    b.remove(1..4); // "aef"
    assert_eq!(b.to_string(), "aef");
    b.undo();
    assert_eq!(b.to_string(), "abcdef");
}

#[test]
fn buffer_replace_and_char_positions() {
    let mut b = Buffer::new("héllo wörld");
    // 中文字符按 char 索引
    assert_eq!(b.char_count(), 11);
    b.replace(0..5, "hi");
    assert_eq!(b.to_string(), "hi wörld");
}

#[test]
fn buffer_lines() {
    let b = Buffer::new("第一行\nsecond\n\nthird");
    assert_eq!(b.line_count(), 4);
    assert_eq!(b.line_to_char(1), 4); // "第一行" 3 字符 + \n = 4
    assert_eq!(b.line_content(1), "second");
    assert_eq!(b.line_content(2), "");
}

#[test]
fn buffer_set_text_resets_history() {
    let mut b = Buffer::new("abc");
    b.set_text("xyz");
    assert_eq!(b.to_string(), "xyz");
    assert!(!b.undo());
}

// ===== search =====

fn q(pattern: &str) -> SearchQuery {
    SearchQuery {
        pattern: pattern.to_string(),
        replace: String::new(),
        mode: SearchMode::Normal,
        match_case: false,
        whole_word: false,
    }
}

#[test]
fn find_normal_case_insensitive() {
    let text = "Hello hello HELLO hola";
    let m = find_matches(text, &q("hello"));
    assert_eq!(m.len(), 3);
}

#[test]
fn find_case_sensitive() {
    let mut query = q("Hello");
    query.match_case = true;
    let m = find_matches("Hello hello HELLO", &query);
    assert_eq!(m.len(), 1);
}

#[test]
fn find_whole_word() {
    let mut query = q("cat");
    query.whole_word = true;
    query.match_case = true;
    let m = find_matches("cat catalog concat cat", &query);
    assert_eq!(m.len(), 2);
}

#[test]
fn find_regex() {
    let mut query = q(r"\d+");
    query.mode = SearchMode::Regex;
    let m = find_matches("a1 bb22 ccc333", &query);
    assert_eq!(m.len(), 3);
    assert_eq!(&"a1 bb22 ccc333"[m[1].0..m[1].1], "22");
}

#[test]
fn find_extended_escapes() {
    let mut query = q("a\\tb");
    query.mode = SearchMode::Extended;
    let m = find_matches("x\ta\ty a\tb", &query);
    assert_eq!(m.len(), 1);
}

#[test]
fn replace_all_counts() {
    let mut query = q("foo");
    query.replace = "bar".to_string();
    let (new, n) = replace_all("foo FOO foo", &query).unwrap();
    assert_eq!(new, "bar bar bar");
    assert_eq!(n, 3);
}

#[test]
fn replace_all_regex_captures() {
    let mut query = q(r"(\w+)@(\w+)");
    query.mode = SearchMode::Regex;
    query.replace = "$2/$1".to_string();
    let (new, n) = replace_all("user@host", &query).unwrap();
    assert_eq!(new, "host/user");
    assert_eq!(n, 1);
}

#[test]
fn replace_one_at_range() {
    let mut query = q("b");
    query.replace = "X".to_string();
    let text = "abc";
    let matches = find_matches(text, &query);
    let new = replace_one(text, &query, matches[0]).unwrap();
    assert_eq!(new, "aXc");
}

// ===== file_ops =====

#[test]
fn detect_eol_variants() {
    assert_eq!(detect_eol("a\r\nb\r\n"), "CRLF");
    assert_eq!(detect_eol("a\rb\r"), "CR");
    assert_eq!(detect_eol("a\nb\n"), "LF");
    assert_eq!(detect_eol("no newline"), "LF");
}

#[test]
fn normalize_eol_variants() {
    assert_eq!(normalize_eol("a\r\nb\rc\nd", "LF"), "a\nb\nc\nd");
    assert_eq!(normalize_eol("a\nb", "CRLF"), "a\r\nb");
}

#[test]
fn looks_binary_variants() {
    assert!(looks_binary(&[0x00, 0x01, 0x02]));
    assert!(looks_binary(&b"abc\x00def"[..]));
    assert!(!looks_binary(b"plain text"));
    assert!(!looks_binary(b"\xe4\xbd\xa0\xe5\xa5\xbd")); // UTF-8 中文
}

#[test]
fn decode_utf8_and_gbk() {
    let (s, enc) = decode_bytes("你好".as_bytes());
    assert_eq!(s, "你好");
    assert_eq!(enc, "UTF-8");
    // GBK 编码的 "中文"
    let gbk = [0xd6, 0xd0, 0xce, 0xc4];
    let (s2, enc2) = decode_bytes(&gbk);
    assert_eq!(s2, "中文");
    assert_eq!(enc2, "GBK");
}

#[test]
fn read_write_roundtrip() {
    let dir = std::env::temp_dir().join("lumen_test_roundtrip");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.txt");
    std::fs::write(&path, "line1\nline2\n").unwrap();
    let r = read_text_file(path.to_str().unwrap()).unwrap();
    assert_eq!(r.content, "line1\nline2\n");
    assert_eq!(r.eol, "LF");
    let w = write_text_file(path.to_str().unwrap(), "x\r\ny", "CRLF", Some("UTF-8")).unwrap();
    assert!(!w.had_unmappable_chars);
    assert_eq!(std::fs::read(&path).unwrap(), b"x\r\ny");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn write_gbk_roundtrip() {
    let dir = std::env::temp_dir().join("lumen_test_gbk");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("g.txt");
    write_text_file(path.to_str().unwrap(), "中文", "LF", Some("GBK")).unwrap();
    let raw = std::fs::read(&path).unwrap();
    assert_eq!(raw, vec![0xd6, 0xd0, 0xce, 0xc4]);
    let r = read_text_file(path.to_str().unwrap()).unwrap();
    assert_eq!(r.content, "中文");
    assert_eq!(r.encoding, "GBK");
    std::fs::remove_dir_all(&dir).ok();
}

// ===== 输入法（IME）相关的缓冲区行为 =====

#[test]
fn ime_utf16_offsets_round_trip() {
    use lumen::buffer::Buffer;
    // "a😀你" ：😀 在 UTF-16 里占 2 个码元
    let b = Buffer::new("a😀你");
    assert_eq!(b.char_to_utf16(0), 0);
    assert_eq!(b.char_to_utf16(2), 3);
    assert_eq!(b.char_to_utf16(3), 4);
    assert_eq!(b.utf16_to_char(3), 2);
    assert_eq!(b.utf16_to_char(999), 3); // 越界夹到文末
    assert_eq!(b.slice_string(1..3), "😀你");
}

#[test]
fn ime_composition_stays_out_of_undo_history() {
    use lumen::buffer::Buffer;
    let mut b = Buffer::new("");
    // 模拟输入 "ni" → 候选 "你"：组合文本反复改写，最终只上屏一次。
    let end = b.replace_silent(0..0, "n");
    assert_eq!(end, 1);
    let end = b.replace_silent(0..1, "ni");
    assert_eq!(end, 2);
    b.replace_silent(0..2, ""); // 上屏前先移除组合文本
    let end = b.insert(0, "你");
    assert_eq!(end, 1);
    assert_eq!(b.to_string(), "你");
    assert!(b.is_dirty());
    // 一次撤销就应回到空文档，而不是逐个拼音字母回退。
    assert!(b.undo());
    assert_eq!(b.to_string(), "");
    assert!(!b.undo());
}
