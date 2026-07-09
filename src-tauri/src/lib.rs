mod file_assoc;
mod file_ops;

use tauri::Emitter;
use tauri::Manager;

/// 获取启动时的命令行参数中的文件路径（用于"打开方式"双击文件启动时）。
/// argv[0] 是程序自身路径，argv[1..] 可能包含被双击打开的文件路径。
fn extract_launch_file(args: &[String]) -> Option<String> {
    args.iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .cloned()
}

#[tauri::command]
fn get_launch_file() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    extract_launch_file(&args)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // 已有实例运行时，第二次双击文件会走这里：把新文件路径发给前端打开新标签
            if let Some(path) = extract_launch_file(&argv) {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("open-file-request", path);
                    let _ = window.set_focus();
                }
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_launch_file,
            file_ops::read_text_file,
            file_ops::write_text_file,
            file_ops::get_file_meta,
            file_assoc::register_file_extension,
            file_assoc::unregister_file_extension,
            file_assoc::set_default_file_extension,
            file_assoc::is_extension_registered,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
