//! 文件关联注册逻辑（移植自原 Tauri 版 `file_assoc.rs`）。
//!
//! 仅在 Windows 下生效：写入 `HKEY_CURRENT_USER` 而非 `HKEY_LOCAL_MACHINE`，
//! 因此不需要管理员权限，且只影响当前用户。把程序注册为该扩展名的"可选打开方式"
//! （`OpenWithProgids`），不强制修改用户已有的默认关联。
//!
//! 这些扩展名会在程序每次启动时被静默、幂等地全部注册为 Lumen 的候选打开方式；
//! 工具栏的"设为默认"按钮会调用 `set_as_default` 把当前文件类型设为 Lumen 默认。
//!
//! 非 Windows 平台（如开发用的 Linux）只提供桩函数，返回错误——原项目本就只支持 Windows。

#[allow(dead_code)]
pub const COMMON_EXTENSIONS: &[&str] = &[
    // 纯文本 / 文档
    "txt", "log", "md", "markdown", "rst", "adoc", "asciidoc", "text", "me", "nfo",
    // Web 前端
    "html", "htm", "xhtml", "css", "scss", "sass", "less", "styl",
    "js", "mjs", "cjs", "jsx", "ts", "tsx", "mts", "cts", "vue", "svelte", "astro",
    "json", "json5", "jsonc", "webmanifest", "map",
    // 后端 / 通用编程语言
    "py", "pyw", "pyi",
    "java", "kt", "kts", "groovy", "scala", "clj", "cljs", "cljc",
    "c", "h", "cpp", "cc", "cxx", "hpp", "hxx", "hh",
    "cs", "csx", "vb", "fs", "fsx",
    "go", "rs",
    "php", "phtml",
    "rb", "erb", "rake", "gemspec",
    "swift",
    "dart",
    "lua",
    "pl", "pm",
    "r", "rmd",
    "jl",
    "ex", "exs", "erl", "hrl",
    "hs", "lhs",
    "ml", "mli",
    "nim",
    "zig",
    "v", "sv",
    "asm", "s",
    "sql", "psql", "plsql",
    "graphql", "gql",
    "proto",
    "wat",
    // 配置 / 数据 / 标记语言
    "ini", "cfg", "conf", "config", "properties",
    "yml", "yaml",
    "toml",
    "xml", "xsd", "xsl", "xslt", "dtd", "plist",
    "csv", "tsv",
    "env", "envrc",
    "editorconfig", "gitignore", "gitattributes", "dockerignore", "npmignore",
    "lock",
    // 脚本 / Shell / 自动化
    "sh", "bash", "zsh", "fish", "ksh",
    "bat", "cmd",
    "ps1", "psm1", "psd1",
    "awk", "sed",
    "makefile", "mk", "cmake", "gradle",
    "dockerfile",
    "vagrantfile",
    "gitconfig",
    // 模板引擎 / 文档标记
    "tpl", "twig", "jinja", "jinja2", "j2", "hbs", "handlebars", "mustache", "ejs", "pug", "jade",
    "haml", "slim",
    "tex", "bib", "cls", "sty",
    "wiki",
    // 其他常见开发相关纯文本格式
    "srt", "vtt", "ass", "sub",
    "diff", "patch",
    "reg",
    "manifest",
    "htaccess",
    "url", "desktop",
];

#[cfg(target_os = "windows")]
mod win {
    use std::env;
    use winreg::enums::*;
    use winreg::RegKey;

    const PROG_ID_PREFIX: &str = "Lumen.TextFile";

    fn exe_path() -> Result<String, String> {
        env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| format!("无法获取程序路径: {}", e))
    }

    /// 注册一个 ProgID（如 Lumen.TextFile.txt），并把它加入该扩展名的 OpenWithProgids 列表。
    #[allow(dead_code)]
    pub fn register_extension(ext: &str) -> Result<(), String> {
        let ext = ext.trim_start_matches('.').to_lowercase();
        if ext.is_empty() {
            return Err("扩展名不能为空".to_string());
        }
        let exe = exe_path()?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        let prog_id = format!("{}.{}", PROG_ID_PREFIX, ext);

        let classes_path = format!("Software\\Classes\\{}", prog_id);
        let (prog_key, _) = hkcu
            .create_subkey(&classes_path)
            .map_err(|e| format!("创建注册表项失败: {}", e))?;
        prog_key
            .set_value("", &format!("Lumen 文本文件 (.{})", ext))
            .map_err(|e| e.to_string())?;

        let (icon_key, _) = prog_key
            .create_subkey("DefaultIcon")
            .map_err(|e| e.to_string())?;
        icon_key
            .set_value("", &format!("{},0", exe))
            .map_err(|e| e.to_string())?;

        let (cmd_key, _) = prog_key
            .create_subkey("shell\\open\\command")
            .map_err(|e| e.to_string())?;
        cmd_key
            .set_value("", &format!("\"{}\" \"%1\"", exe))
            .map_err(|e| e.to_string())?;

        let ext_key_path = format!("Software\\Classes\\.{}", ext);
        let (ext_key, _) = hkcu
            .create_subkey(&ext_key_path)
            .map_err(|e| e.to_string())?;
        let (owp_key, _) = ext_key
            .create_subkey("OpenWithProgids")
            .map_err(|e| e.to_string())?;
        let empty_value: String = String::new();
        owp_key
            .set_value(&prog_id, &empty_value)
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// 将 Lumen 设为该扩展名的默认打开程序（直接设置 .ext 的默认值指向我们的 ProgID）。
    pub fn set_as_default(ext: &str) -> Result<(), String> {
        register_extension(ext)?;
        let ext = ext.trim_start_matches('.').to_lowercase();
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let prog_id = format!("{}.{}", PROG_ID_PREFIX, ext);

        let ext_key_path = format!("Software\\Classes\\.{}", ext);
        let (ext_key, _) = hkcu
            .create_subkey(&ext_key_path)
            .map_err(|e| e.to_string())?;
        ext_key
            .set_value("", &prog_id)
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    #[allow(dead_code)]
    pub fn is_registered(ext: &str) -> bool {
        let ext = ext.trim_start_matches('.').to_lowercase();
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let prog_id = format!("{}.{}", PROG_ID_PREFIX, ext);
        let ext_key_path = format!("Software\\Classes\\.{}\\OpenWithProgids", ext);
        if let Ok(owp_key) = hkcu.open_subkey(ext_key_path) {
            return owp_key.get_value::<String, _>(&prog_id).is_ok();
        }
        false
    }

    /// 批量注册常见文本/代码扩展名为"打开方式"候选项（幂等）。
    pub fn register_common_extensions() {
        for ext in super::COMMON_EXTENSIONS {
            let _ = register_extension(ext);
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod other {
    #[allow(dead_code)]
    pub fn register_extension(_ext: &str) -> Result<(), String> {
        Err("文件关联功能仅支持 Windows".to_string())
    }
    pub fn set_as_default(_ext: &str) -> Result<(), String> {
        Err("文件关联功能仅支持 Windows".to_string())
    }
    #[allow(dead_code)]
    pub fn is_registered(_ext: &str) -> bool {
        false
    }
    pub fn register_common_extensions() {}
}

#[cfg(target_os = "windows")]
use win as platform;
#[cfg(not(target_os = "windows"))]
use other as platform;


/// 程序启动时调用一次：静默注册全部常见扩展名为 Lumen 的可选打开方式。幂等。
pub fn register_all_common_extensions_silently() {
    platform::register_common_extensions();
}

/// 把指定扩展名设为 Lumen 的默认打开程序（Windows 下生效）。
pub fn set_default_file_extension(ext: &str) -> Result<(), String> {
    platform::set_as_default(ext)
}
