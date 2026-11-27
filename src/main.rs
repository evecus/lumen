// 防止在 Windows 下额外弹出控制台终端窗口（旧版 Tauri 入口同样处理了这一点）。
// 旧版写法是 `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`，
// 只在 release 生效；这里改为始终生效，保证 `cargo run` 调试运行时也不弹黑框。
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use gpui::*;
use lumen::file_assoc;
use lumen::workspace;

/// 对齐旧版 `tauri.conf.json` 的初始窗口尺寸（逻辑像素）。
const PREFERRED_SIZE: Size<Pixels> = size(px(1200.0), px(800.0));
/// 对齐旧版 `tauri.conf.json` 的 minWidth / minHeight（逻辑像素）。
const MIN_SIZE: Size<Pixels> = size(px(640.0), px(400.0));

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

/// 计算初始窗口尺寸：默认沿用旧版的 1200×800，但在小屏 / 高 DPI 缩放下
/// 按主显示器可用逻辑尺寸收缩，避免窗口超出屏幕。
///
/// GPUI 的 `px()` 是逻辑像素，Windows 平台创建窗口时会再乘上 DPI
/// `scale_factor`；旧实现把窗口钉在屏幕左上角 `(0,0)` 并写死 1200×800，
/// 在 125% / 150% 缩放下会被放大到 1500×1000 / 1800×1200，顶到屏幕边缘，
/// 右侧、底部和状态栏被裁掉 —— 这就是"窗口显示不正常"的主因。
/// 另外 GPUI 会用 bounds 中心点判断显示器，中心点落在屏幕外会直接退化成
/// `default_bounds()`，尺寸完全不受控；居中计算可以避开这条分支。
fn initial_window_size(cx: &App) -> Size<Pixels> {
    let Some(display) = cx.primary_display() else {
        return PREFERRED_SIZE;
    };
    let screen = display.bounds().size;

    // 预留 10% 边距给系统标题栏、窗口边框阴影和任务栏。
    let max_w = f32::from(screen.width) * 0.9;
    let max_h = f32::from(screen.height) * 0.9;

    size(
        px(f32::from(PREFERRED_SIZE.width)
            .min(max_w)
            .max(f32::from(MIN_SIZE.width))),
        px(f32::from(PREFERRED_SIZE.height)
            .min(max_h)
            .max(f32::from(MIN_SIZE.height))),
    )
}

fn main() {
    // 与旧版 Tauri 一致：文件关联注册放到独立线程。首次启动要写几十个
    // 扩展名的注册表项，同步执行会明显拖慢窗口首次显示。
    // Windows 上生效，其它平台为 no-op。
    std::thread::spawn(|| {
        file_assoc::register_all_common_extensions_silently();
    });

    // 后台预加载语法高亮定义，打开第一个文件时不卡顿。
    lumen::highlight::preload();

    let launch_file = launch_file_from_argv();

    Application::new().run(move |cx: &mut App| {
        // 居中计算需要 &App，先算完再进入 open_window，避免借用冲突。
        let window_size = initial_window_size(cx);
        let window_bounds = WindowBounds::centered(window_size, cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                window_min_size: Some(MIN_SIZE),
                focus: true,
                show: true,
                is_resizable: true,
                is_minimizable: true,
                titlebar: Some(TitlebarOptions {
                    title: Some("Lumen 文本编辑器".into()),
                    // 保留系统标题栏：旧版 Tauri 同样是系统标题栏 + 前端自绘的
                    // 36px 标题栏。Workspace 自绘标题栏里没有最小化/最大化/关闭
                    // 按钮，也没有窗口拖拽处理，一旦改成 true 窗口将无法拖动和关闭。
                    appears_transparent: false,
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let ws = workspace::Workspace::new(cx, launch_file);
                // 必须让工作区拿到焦点：输入法处理器只会注册给"有焦点"的元素，
                // 否则不点一下编辑区就没法用拼音输入。
                let focus = ws.read(cx).focus_handle(cx);
                window.focus(&focus);
                ws
            },
        )
        .unwrap();
    });
}
