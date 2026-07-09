// 文件关联注册逻辑：仅在 Windows 下生效。
// 策略：写入 HKEY_CURRENT_USER 而不是 HKEY_LOCAL_MACHINE，这样不需要管理员权限，
// 且只影响当前用户。把程序注册为该扩展名的"可选打开方式"（App Paths + OpenWithProgids），
// 不强制修改用户已有的默认关联，除非用户在设置页里显式点击"设为默认"。

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
    /// 这样文件不会被强制改成用 Lumen 打开，但会出现在"打开方式"菜单里，
    /// 用户也可以自己在资源管理器里选择"始终使用此应用打开"。
    pub fn register_extension(ext: &str) -> Result<(), String> {
        let ext = ext.trim_start_matches('.').to_lowercase();
        if ext.is_empty() {
            return Err("扩展名不能为空".to_string());
        }
        let exe = exe_path()?;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        let prog_id = format!("{}.{}", PROG_ID_PREFIX, ext);

        // 1. 注册 ProgID：HKCU\Software\Classes\Lumen.TextFile.<ext>
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

        // 2. 把 ProgID 加入该扩展名的 OpenWithProgids 列表
        //    HKCU\Software\Classes\.ext\OpenWithProgids
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

        notify_shell_change();
        Ok(())
    }

    /// 从"打开方式"列表中移除关联（不影响其他关联）
    pub fn unregister_extension(ext: &str) -> Result<(), String> {
        let ext = ext.trim_start_matches('.').to_lowercase();
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let prog_id = format!("{}.{}", PROG_ID_PREFIX, ext);

        let ext_key_path = format!("Software\\Classes\\.{}\\OpenWithProgids", ext);
        if let Ok(owp_key) = hkcu.open_subkey_with_flags(&ext_key_path, KEY_ALL_ACCESS) {
            let _ = owp_key.delete_value(&prog_id);
        }

        notify_shell_change();
        Ok(())
    }

    /// 将 Lumen 设为该扩展名的默认打开程序（写入 UserChoice 需要走系统 API，
    /// 这里采用更简单可靠的方式：直接设置 .ext 的默认值指向我们的 ProgID）
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

        notify_shell_change();
        Ok(())
    }

    pub fn is_registered(ext: &str) -> bool {
        let ext = ext.trim_start_matches('.').to_lowercase();
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let prog_id = format!("{}.{}", PROG_ID_PREFIX, ext);
        let ext_key_path = format!("Software\\Classes\\.{}\\OpenWithProgids", ext);
        if let Ok(owp_key) = hkcu.open_subkey(&ext_key_path) {
            return owp_key.get_value::<String, _>(&prog_id).is_ok();
        }
        false
    }

    fn notify_shell_change() {
        // 通知资源管理器刷新文件关联缓存。
        // 使用 SHChangeNotify 需要 windows crate，这里用一个轻量做法：
        // 大部分情况下重启资源管理器或重新登录后才会完全生效，
        // 这一点已在前端设置页做了提示。
    }
}

#[cfg(not(target_os = "windows"))]
mod other {
    pub fn register_extension(_ext: &str) -> Result<(), String> {
        Err("文件关联功能仅支持 Windows".to_string())
    }
    pub fn unregister_extension(_ext: &str) -> Result<(), String> {
        Err("文件关联功能仅支持 Windows".to_string())
    }
    pub fn set_as_default(_ext: &str) -> Result<(), String> {
        Err("文件关联功能仅支持 Windows".to_string())
    }
    pub fn is_registered(_ext: &str) -> bool {
        false
    }
}

#[cfg(target_os = "windows")]
use win as platform;
#[cfg(not(target_os = "windows"))]
use other as platform;

#[tauri::command]
pub fn register_file_extension(ext: String) -> Result<(), String> {
    platform::register_extension(&ext)
}

#[tauri::command]
pub fn unregister_file_extension(ext: String) -> Result<(), String> {
    platform::unregister_extension(&ext)
}

#[tauri::command]
pub fn set_default_file_extension(ext: String) -> Result<(), String> {
    platform::set_as_default(&ext)
}

#[tauri::command]
pub fn is_extension_registered(ext: String) -> bool {
    platform::is_registered(&ext)
}
