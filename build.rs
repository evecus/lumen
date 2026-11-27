//! Windows 构建时把图标嵌入 exe：资源 ID 1 同时被 GPUI 用作窗口 / 任务栏图标，
//! 也是资源管理器 / 桌面快捷方式显示的图标。
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=assets/lumen.rc");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_resource::compile("assets/lumen.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
