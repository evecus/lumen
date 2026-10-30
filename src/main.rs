use gpui::*;
use lumen::file_assoc;
use lumen::workspace;

fn launch_file_from_argv() -> Option<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    for a in args {
        let p = std::path::PathBuf::from(&a);
        if p.is_file() {
            if let Some(c) = p.canonicalize().ok().and_then(|c| c.to_str().map(String::from)) {
                return Some(c);
            }
            return Some(a);
        }
    }
    None
}

fn main() {
    // Windows 上启动时静默注册常见扩展名文件关联（其它平台为 no-op）
    file_assoc::register_all_common_extensions_silently();

    let launch_file = launch_file_from_argv();

    Application::new().run(move |cx: &mut App| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.0), px(0.0)),
                    size(px(1200.0), px(800.0)),
                ))),
                focus: true,
                show: true,
                titlebar: Some(TitlebarOptions {
                    title: Some("Lumen".into()),
                    appears_transparent: false,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |_window, cx| workspace::Workspace::new(cx, launch_file),
        )
        .unwrap();
    });
}
