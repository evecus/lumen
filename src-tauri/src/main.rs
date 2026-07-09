// 防止在 Windows Release 构建下额外弹出控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    lumen_editor_lib::run();
}
