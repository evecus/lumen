# 本次修复

只改了 `src/workspace.rs`，没有新增依赖，`Cargo.lock` 不变（CI 的 `--locked` 照常可用）。

## 1. 编辑区文字选择

| 问题 | 原因 | 修复 |
| --- | --- | --- |
| 单击后不按鼠标、只移动，选区也跟着变 | 没有处理鼠标松开，`drag_mode` 一直停在 `Select` | 在 canvas 里注册窗口级 `MouseUpEvent`，左键松开即结束拖选；移动时再检查 `pressed_button`，左键没按着就不再选 |
| 点击位置和光标落点对不上（偏上约 10px） | 鼠标坐标多减了一次上边距 `content_top` | 新增 `hit_test()` 统一换算，右键、按下、拖动都用它 |
| 拖到编辑区外面选区就不动了 | `on_mouse_move` 挂在编辑区 div 上，只有鼠标悬停在编辑区时才触发 | 改为 `window.on_mouse_event` 窗口级监听；拖出上/下边缘自动滚动 |
| Shift+Home / Shift+End 不能选择；End 会跳到下一行开头 | Home/End 不看 Shift；行尾取成了「下一行行首」 | 支持 Shift；行尾 = 行首 + 可见字符数 |
| 用 Shift+方向键选到可视区域外，看不到选区 | 光标移动后不滚动 | 新增 `ensure_cursor_visible()`，键盘移动、输入、粘贴、撤销后调用 |
| 选中空行时没有任何高亮 | 选区背景只画在字形上 | 选区包含换行符时，在行尾补一块选区色 |
| 点查找面板时，编辑区会抢走焦点并移动光标 | 查找面板没有 `occlude` | 查找面板 `occlude()` + `stop_propagation` |
| — | — | 补上三击选整行、Ctrl+←/→ 按词移动、Ctrl+Home/End |

## 2. 新建文件弹窗：点选项弹窗就消失

原因：遮罩和卡片是兄弟元素。卡片的 `on_mouse_down` 是空函数，既没有 `occlude` 也没有 `stop_propagation`，GPUI 会把同一次按下分发给鼠标下所有未被遮挡的元素，所以遮罩的「点空白关闭」也被触发了；下层编辑区还会把输入焦点抢走。

修复：
- 外层容器 `occlude()`，挡住编辑区和侧边栏；
- 卡片 `occlude()` + `stop_propagation()`，挡住遮罩；
- 编码列表原来有 `max_h(160)` 但不能滚动，选项溢出到卡片外。现在改为最高 260px、可滚动，卡片最高为窗口的 90%；
- 关闭弹窗时把 `input_focus` 复位，避免之后打字没有反应。

## 3. 打开 / 另存为改用系统对话框

删除了自绘的目录浏览弹窗（`FileDialog`、`DirEntry`、`file_dialog_view` 等），改用 GPUI 封装的系统对话框，在 Windows 上就是资源管理器样式的「打开 / 另存为」窗口：
- **打开**（工具栏「打开」/ Ctrl+O）：`cx.prompt_for_paths`，可以多选，每个文件开一个标签；
- **另存为**（未保存过的文件按「保存」/ Ctrl+Shift+S）：`cx.prompt_for_new_path`，默认文件名是当前标签名；已保存过的文件从它所在目录开始。保存后标签名会更新为新文件名（旧版一直显示原来的名字）；
- 对话框打开期间切换了标签，也会保存到当初发起另存为的那个标签。

---

# 第二轮修复

## 4. 新建弹窗改不了文件名

原来的输入框只是一段静态文字，不是真正的输入框：
- 没有光标，不能用鼠标或方向键移动，也不能选择，只能从末尾退格再重新输入；
- 空格键被吞掉（空格是具名键，不走输入法，输入框分支里也没有处理）。

现在三个输入框（新建文件名、查找、替换）都改成真正的单行输入框：
- 光标、选区高亮、聚焦时蓝色边框；
- ← → Home End（按住 Shift 可选择）、Backspace、Delete、空格；
- Ctrl+A / C / X / V（粘贴时把换行换成空格）；
- 鼠标点击定位光标，Shift+点击扩展选区，双击全选；
- **打开新建弹窗时，默认选中扩展名前面的部分**（和资源管理器重命名一样），直接打字就能替换 `未命名-2`，`.txt` 保留；
- 中文输入法的候选窗贴着输入框里的光标显示。

## 5. 语法高亮

新增 `src/highlight.rs`，依赖 `two-face`（bat 项目整理的 syntect 语法包，内含 syntect）：
- 约 190 种语言 / 文本格式：Rust、C/C++、C#、Java、Kotlin、Go、Python、JS/TS/TSX、Vue、Svelte、PHP、Ruby、Swift、Dart、Lua、SQL、Shell/Bash、Batch、HTML/CSS/SCSS/LESS、XML、JSON、YAML、TOML、INI、Markdown、Dockerfile、Makefile、CMake、Nginx、Diff、LaTeX、Protobuf、GraphQL、Terraform、Zig、Nix 等；
- 识别顺序：特定文件名（Dockerfile、Makefile、CMakeLists.txt、Cargo.lock…）→ 扩展名（另有 .jsonc、.csproj、.xaml、.svg、.cmd、.reg 等别名）→ 首行（`#!/usr/bin/env python`、`<?xml`、`<?php`…）；
- 状态栏最左边显示当前语言，另存为换了扩展名会自动重新识别；
- 配色用白底的 Inspired GitHub，和 Lumen 原来的明亮主题一致；选区和查找高亮叠加在语法颜色上，可以同时显示；
- 性能：只给可见行着色；每行缓存解析状态，编辑后只重新解析改动的那一行起、到状态重新一致为止的几行；超过 500 万字符的文件、超过 1 万字节的单行（压缩过的 JS）不着色；
- 语法包在启动时用后台线程预加载；
- 正则引擎用纯 Rust 的 fancy-regex，不需要 C 编译器去编 Oniguruma。代价是 two-face 的 fancy 版本不含 PowerShell、Sass(缩进语法)、ARM 汇编这几种语法（它们用到了 fancy-regex 不支持的正则特性）。

**注意**：新增了依赖，`Cargo.lock` 会变化。CI 用的是 `cargo build --locked`，请先在本地 `cargo build` 一次，把更新后的 `Cargo.lock` 一起提交。
