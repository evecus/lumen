# Lumen 文本编辑器

一个用 **Tauri + Svelte** 写的 Windows 桌面文本/代码编辑器：体积小、启动快，支持多标签页、查找替换、真实本地文件读写，并且安装后会自动让自己成为常见文本/代码格式的可选打开方式。

## 功能

- 多标签页同时编辑多个文件
- 打开任意"看起来是文本"的文件（不限制白名单扩展名，二进制文件会被识别并拒绝以文本方式打开）
- 真正的本地文件读写（不是浏览器下载模拟保存）
- 自动编码嗅探（UTF-8 / UTF-16 / GBK 等），保存时按原文件的换行符风格写回
- 编辑核心基于 **CodeMirror 6**：
  - 自动换行永久开启，长段落超出编辑区宽度会自动折行，不会出现内容被截断看不全的情况
  - 软换行时行号栏正确地只在每个逻辑行的开头显示数字，折行部分自动留空
  - 内置真正的视口虚拟化，大文件也能流畅滚动、编辑
  - 查找 / 替换直接使用 CodeMirror 官方 `@codemirror/search` 扩展自带的面板（点击编辑区后按 `Ctrl+F` / `Ctrl+H` 呼出，或点工具栏"查找/替换"按钮），支持正则、大小写敏感、全部替换等
- 文件关联：程序每次启动时会自动、静默地把一批常见文本/代码扩展名注册为 Lumen 的"打开方式"候选（不需要用户手动设置），工具栏有个"设为默认"按钮，点一下就能把当前文件的格式设为 Lumen 默认打开程序

## 项目结构

```
lumen-editor/
├── src/                      # Svelte 前端
│   ├── App.svelte
│   ├── app.css
│   ├── main.js
│   └── lib/
│       ├── store.js              # 标签页/文件状态
│       ├── fileActions.js        # 打开/保存文件的动作
│       ├── fileAssocActions.js   # "设为默认"文件关联动作
│       ├── toastStore.js
│       └── components/           # 各 UI 组件
├── src-tauri/                # Rust 后端
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs            # Tauri 启动入口、命令注册、启动时自动注册文件关联
│   │   ├── file_ops.rs       # 文件读写、编码/二进制检测
│   │   └── file_assoc.rs     # Windows 注册表文件关联
│   ├── capabilities/default.json
│   ├── tauri.conf.json
│   ├── Cargo.toml
│   └── icons/
├── .github/workflows/build.yml   # CI：自动编译 Windows 安装包
├── package.json
├── vite.config.js
└── index.html
```

## 本地开发（如果你想在本地跑起来调试）

前置要求：
- Node.js 18+
- Rust（[rustup.rs](https://rustup.rs)）
- Windows 上还需要 [WebView2 运行时](https://developer.microsoft.com/microsoft-edge/webview2/)（Windows 10/11 通常已自带）

```bash
npm install
npm run tauri dev
```

## 用 GitHub Actions 编译成 Windows 安装包（你不需要本地环境）

项目已经内置了 `.github/workflows/build.yml`，只需要：

1. 把这个项目推到你自己的 GitHub 仓库：
   ```bash
   git init
   git add .
   git commit -m "init lumen editor"
   git branch -M main
   git remote add origin <你的仓库地址>
   git push -u origin main
   ```

2. **触发构建的两种方式：**

   - **只想要构建产物、不发正式 Release**：直接推送到 `main` 分支，或者去 GitHub 仓库的 Actions 页面手动点击 "Run workflow"。构建完成后，在该次 workflow run 的页面底部 "Artifacts" 里下载 `lumen-windows-installer`，里面是生成的 `.exe` 安装包。

   - **想要发布一个正式版本（推荐）**：打一个 tag 再推送，例如：
     ```bash
     git tag v0.1.0
     git push origin v0.1.0
     ```
     这会触发构建，并自动在仓库的 Releases 里创建一个**草稿** Release，把编译好的 `.exe` 安装包作为附件传上去。你可以去 GitHub 仓库的 "Releases" 页面，检查一下草稿内容，确认无误后点击 "Publish release" 正式发布。

3. 编译时间：GitHub 提供的 Windows runner 上，Rust 是从源码编译的，第一次大概需要 8-15 分钟（后续因为有 `rust-cache`，会明显加快）。

4. 编译产物：`.exe` 是 NSIS 生成的安装向导程序（双击后引导用户完成安装、创建开始菜单快捷方式），安装到当前用户目录下（不需要管理员权限）。

## 关于文件关联

- 程序**每次启动**时都会在后台线程里静默地把一批常见扩展名（txt/md/log、html/css/js/ts/json、py/java/c/cpp/cs/go/rs/php/rb、ini/yaml/xml/toml/env、sh/bat/ps1 等）注册进 Windows 的"打开方式"候选列表（写入 `HKEY_CURRENT_USER`，不需要管理员权限，只影响当前 Windows 用户）
- 这一步**不会**把 Lumen 设为任何格式的默认程序，只是让 Lumen 有资格出现在右键"打开方式"菜单里，以及双击一个还没有默认程序的文件时系统弹出的选择框里
- 想让某个格式**默认**用 Lumen 打开：打开一个该格式的文件后，点工具栏的"设为默认"按钮即可
- 这个注册过程是幂等的，重复执行不会有副作用，也不会覆盖用户已经设置好的默认程序（除非用户主动点了"设为默认"）
- 修改文件关联后，有时需要重新登录 Windows 或重启资源管理器（`explorer.exe`）才会在右键菜单里完全刷新显示，这是 Windows Shell 缓存机制导致的，属于正常现象

## 已知限制 / 后续可以做的事

- 编辑核心已经换成 CodeMirror 6，没有做语法高亮（按你的要求先把核心功能做稳）。如果之后想要语法高亮，CodeMirror 6 生态里有现成的 `@codemirror/lang-*` 系列包，接入成本不算高。
- 切换标签页时，每个标签的撤销/重做历史目前是独立维护的（每次切换会创建新的 EditorState），不会在标签间串台，但也意味着来回切换标签不会保留"跨标签"的撤销栈，这是预期行为。
- 文本选择走的是浏览器原生选择机制（未启用 CodeMirror 的自绘选区层），所以不支持多光标同时编辑，但保证了选中、复制、粘贴这些操作和普通输入框行为完全一致。
- 文件关联的注册表写入完成后，Windows 有时不会立刻刷新右键菜单显示，需要用户重新登录或重启 `explorer.exe`。
