use encoding_rs::Encoding;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone)]
pub struct FileReadResult {
    pub content: String,
    pub encoding: String,
    pub eol: String,
    pub is_binary: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FileMeta {
    pub name: String,
    pub path: String,
    pub size: u64,
}

/// 粗略判断一段字节是否为二进制内容：
/// 如果前 8000 字节里出现 NUL 字节，或者不可打印控制字符比例过高，视为二进制。
fn looks_binary(bytes: &[u8]) -> bool {
    let sample_len = bytes.len().min(8000);
    let sample = &bytes[..sample_len];
    if sample.iter().any(|&b| b == 0) {
        return true;
    }
    if sample.is_empty() {
        return false;
    }
    let control_count = sample
        .iter()
        .filter(|&&b| b < 9 || (b > 13 && b < 32))
        .count();
    (control_count as f64) / (sample.len() as f64) > 0.05
}

fn detect_eol(text: &str) -> String {
    if text.contains("\r\n") {
        "CRLF".to_string()
    } else if text.contains('\r') {
        "CR".to_string()
    } else {
        "LF".to_string()
    }
}

/// 使用 chardetng 做编码嗅探，再用 encoding_rs 解码为 UTF-8 字符串。
fn decode_bytes(bytes: &[u8]) -> (String, String) {
    // 优先检测 BOM
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let (text, _, _) = encoding_rs::UTF_8.decode(&bytes[3..]);
        return (text.into_owned(), "UTF-8 (BOM)".to_string());
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (text, _, _) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
        return (text.into_owned(), "UTF-16 LE".to_string());
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (text, _, _) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
        return (text.into_owned(), "UTF-16 BE".to_string());
    }

    // 尝试严格 UTF-8
    if let Ok(s) = std::str::from_utf8(bytes) {
        return (s.to_string(), "UTF-8".to_string());
    }

    // 用 chardetng 猜测编码（常见于 GBK/GB18030 等中文文件）
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let guessed: &'static Encoding = detector.guess(None, true);
    let (text, _, _) = guessed.decode(bytes);
    (text.into_owned(), guessed.name().to_string())
}

#[tauri::command]
pub fn read_text_file(path: String) -> Result<FileReadResult, String> {
    let bytes = fs::read(&path).map_err(|e| format!("读取文件失败: {}", e))?;

    if looks_binary(&bytes) {
        return Ok(FileReadResult {
            content: String::new(),
            encoding: "binary".to_string(),
            eol: "LF".to_string(),
            is_binary: true,
        });
    }

    let (content, encoding) = decode_bytes(&bytes);
    let eol = detect_eol(&content);

    Ok(FileReadResult {
        content,
        encoding,
        eol,
        is_binary: false,
    })
}

#[tauri::command]
pub fn write_text_file(path: String, content: String, eol: String) -> Result<(), String> {
    let normalized = normalize_eol(&content, &eol);
    fs::write(&path, normalized).map_err(|e| format!("保存文件失败: {}", e))
}

fn normalize_eol(text: &str, eol: &str) -> String {
    // 先统一成 \n，再按目标格式转换，避免混合换行叠加
    let unified = text.replace("\r\n", "\n").replace('\r', "\n");
    match eol {
        "CRLF" => unified.replace('\n', "\r\n"),
        "CR" => unified.replace('\n', "\r"),
        _ => unified,
    }
}

#[tauri::command]
pub fn get_file_meta(path: String) -> Result<FileMeta, String> {
    let p = Path::new(&path);
    let meta = fs::metadata(&path).map_err(|e| format!("无法获取文件信息: {}", e))?;
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());
    Ok(FileMeta {
        name,
        path,
        size: meta.len(),
    })
}
