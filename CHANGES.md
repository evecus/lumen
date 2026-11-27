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

---

# 第三轮：自动缩进

新增 `src/indent.rs`（纯函数，有单元测试），在 `workspace.rs` 里接到回车、Tab、退格和闭合括号上。

## 缩进单位
从文件内容推断：Tab 多就用 Tab，否则统计相邻行缩进的增量，取 2 / 4 / 8 / 3 中最常见的。新文件或推断不出时按语言默认：
- Go、Makefile 用 Tab；
- JS/TS/JSX/TSX、JSON、Vue、Svelte、HTML/XML、CSS/SCSS/LESS、YAML、Ruby、Lua、Dart、Markdown 用 2 个空格；
- 其他（Rust、C/C++、C#、Java、Python、Go 以外…）用 4 个空格。

## 回车
| 情况 | 效果 |
| --- | --- |
| 普通行 | 新行保持上一行的缩进 |
| 行尾是 `{` `[` `(` | 多缩进一级 |
| 光标在 `{|}` `[|]` `(|)` 之间 | 展开成三行，光标在中间缩进好的那行，闭合符号对齐到原缩进 |
| Python / YAML / Makefile 行尾 `:`（忽略行尾 `#` 注释） | 多缩进一级（Makefile 用 Tab） |
| Python 的 `return` / `pass` / `break` / `continue` / `raise` 之后 | 回退一级 |
| HTML/XML/Vue/JSX 开始标签 `<div …>` 之后 | 多缩进一级；`<div>|</div>` 之间回车展开成三行；`<br>` `<img>` 这类空元素不缩进 |
| Shell 的 `then` / `do` / `else`，Lua/Ruby 的 `then` / `do` / `function` / `def` … | 多缩进一级 |
| 块注释 `/**` 或 ` * ` 行 | 自动续写 ` * ` |
| Markdown 列表 `- ` `* ` `1. ` `- [ ] ` | 自动续写下一项（数字自动加 1）；空项再按回车结束列表 |
| 离开的那一行有行尾空格 | 顺手删掉 |

## 其他按键
- **Tab**：插入到下一个缩进位置（按文件用 Tab 或空格）；选中多行时整块缩进（空行不加）；
- **Shift+Tab**：当前行或选中的各行回退一级；
- **退格**：光标前全是空格时，一次删掉一级缩进；
- **输入 `}` `]` `)`**：如果光标前只有空白，先回退一级再输入，闭合括号会自动对齐到开括号那一行。

---

# 第四轮：做成真正的代码编辑器

先评估了直接换成 gpui-component 的代码编辑器：0.5.1 是还兼容 gpui 0.2.2 的最后一版，它的回车只复制上一行缩进，没有括号补全、注释切换，高亮只有约 30 种语言，比现在少。所以保留 Lumen 自己的编辑器，把缺的功能补齐。

新增 `src/lang.rs`、`src/editing.rs`，扩展 `src/indent.rs`，重写 `workspace.rs` 的排版与绘制。

## 按语言的编辑规则（lang.rs）
语言识别直接复用语法高亮的结果（约 190 种），每种语言给出：
- 缩进类型：大括号类、Python 类（冒号）、YAML、标记语言、Shell、关键字类（Lua/Ruby/Elixir/Pascal/VB…）、越位规则（Haskell/OCaml/F#/Erlang）、Makefile、Markdown、纯文本；
- 行注释：`//` `#` `--` `;` `%` `REM` `!` `'` `"` …；块注释：`/* */` `<!-- -->` `{- -}` `(* *)` `--[[ ]]` `=begin =end` `<# #>` …；
- 自动配对的括号和引号（Rust 不配对 `'`，因为多是生命周期；纯文本不配对）。

## 编辑
| 功能 | 快捷键 |
| --- | --- |
| 括号 / 引号自动配对；光标后已有闭合符号时直接跳过；有选区时包住选区 | 输入 `(` `[` `{` `"` `'` `` ` `` |
| 在空括号 / 空引号中间退格，两个一起删 | Backspace |
| 切换行注释（多行一起，按最小缩进对齐） | Ctrl+/ |
| 切换块注释 | Shift+Alt+A |
| 上移 / 下移行 | Alt+↑ / Alt+↓ |
| 向上 / 向下复制行 | Shift+Alt+↑ / Shift+Alt+↓，Ctrl+Shift+D |
| 删除行 | Ctrl+Shift+K |
| 在下方 / 上方插入新行 | Ctrl+Enter / Ctrl+Shift+Enter |
| 选中整行（连按扩展） | Ctrl+L |
| 缩进 / 反缩进当前行或选中行 | Ctrl+] / Ctrl+[，Tab / Shift+Tab |
| 智能 Home：在首个非空白字符和行首之间切换 | Home |
| 撤销 / 重做后光标回到改动位置 | Ctrl+Z / Ctrl+Y |

缩进规则补充：括号没闭合（`foo(a,`）也会缩进；C/Java/JS 的 `case x:` 后缩进；Haskell/OCaml 的 `=` `->` `do` `where` 后缩进；在 `end` / `else` / `fi` / `done` / `</div>` / Python `else:` `except:` 这类收尾行按回车时，先把这一行退回一级再换行。

## 导航
- 转到行：Ctrl+G（支持 `行:列`）；工具栏也有按钮；
- 跳到配对括号：Ctrl+Shift+\；
- Ctrl+↑/↓ 只滚动不移动光标；
- 查找下一个 / 上一个时，匹配项在视口外会自动滚过去（以前不会滚动）。

## 显示
- **括号配对高亮**：光标在括号旁边时，它和配对括号都会加底色；
- **同名单词高亮**：选中一个单词或光标停在单词上，本屏其它同名单词会加浅底色；
- **缩进参考线**：每级缩进画一条竖线，空行沿用上下行的层级；
- **Tab 按 4 列制表位对齐显示**（以前 Tab 只占一个字符宽，用 Tab 缩进的代码会错位）；
- **自动换行开关**：Alt+Z 或工具栏「自动换行」。代码文件默认不换行，长行横向滚动（Shift+滚轮或触控板横滑，光标移出视口时自动跟随，底部有横向滚动条）；.txt / .md / .log 默认换行；
- 状态栏新增：行号、列号（按 Tab 展开后的列）、选中字符数、缩进方式（空格: 4 / Tab）、语言、换行状态。

## 性能
旧版每一帧都把整篇文档排版两遍，光标移动和鼠标点击也要从第一行累加到当前行，几千行的文件打字就会卡。现在：
- 每个标签有排版缓存，只在内容变化（缓冲区新增了版本号）或宽度变化时重算每行的 y 坐标；
- 自动换行时按行内容哈希缓存每行占几行，插入或删除一行，其它行不用重新排版；
- 绘制、点击、光标定位都是二分查找，只排版看得见的几十行。

## 修复
- `Buffer::replace` 遇到空区间（纯插入）时，会把**上一条**撤销记录弹出来改写，撤销后内容会错乱。现在空区间直接当作插入处理。
