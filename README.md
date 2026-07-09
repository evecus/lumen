# Lumen 文本编辑器

一个用 **Tauri + Svelte** 写的 Windows 桌面文本/代码编辑器：体积小、启动快，支持多标签页、查找替换、真实本地文件读写，并可以在设置里把自己注册成任意扩展名的可选（或默认）打开方式。

## 功能

- 多标签页同时编辑多个文件
- 打开任意"看起来是文本"的文件（不限制白名单扩展名，二进制文件会被识别并拒绝以文本方式打开）
- 真正的本地文件读写（不是浏览器下载模拟保存）
- 自动编码嗅探（UTF-8 / UTF-16 / GBK 等），保存时按原文件的换行符风格写回
- 查找 / 替换面板，参照 Notepad++ 交互：
  - 反向查找、全词匹配、匹配大小写、循环查找
  - 普通 / 扩展转义 (`\n \r \t \0 \xFF`) / 正则表达式 三种查找模式
  - 替换、在选区替换、全部替换、计数
- 大文件模式：超过 5000 行自动切换为行号虚拟渲染 + 关闭自动换行，避免卡顿
- 设置页里勾选想关联的扩展名，加入 Windows"打开方式"列表，也可以一键设为默认程序
- 双击已关联的文件，或者用"打开方式"选择 Lumen，都能正确打开（包括程序已在运行时）

## 项目结构

```
lumen-editor/
├── src/                      # Svelte 前端
│   ├── App.svelte
│   ├── app.css
│   ├── main.js
│   └── lib/
│       ├── store.js          # 标签页/文件状态
│       ├── searchStore.js    # 查找替换状态与匹配逻辑
│       ├── fileActions.js    # 打开/保存文件的动作
│       ├── toastStore.js
│       └── components/       # 各 UI 组件
├── src-tauri/                # Rust 后端
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs            # Tauri 启动入口、命令注册
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

出于安全和权限考虑，本项目默认：

- **不会**自动把自己设为任何文件类型的默认打开程序
- 只有你在应用内"设置"页面里手动勾选某个扩展名，才会：
  - 把 Lumen 加入该扩展名的"打开方式"候选列表（写入 `HKEY_CURRENT_USER`，不需要管理员权限，只影响当前 Windows 用户）
  - 如果你点击"设为默认"，才会真正把 Lumen 设为该扩展名的默认打开程序

修改文件关联后，有时需要重新登录 Windows 或重启资源管理器（`explorer.exe`）才会在右键菜单里完全刷新显示，这是 Windows Shell 缓存机制导致的，属于正常现象。

## 已知限制 / 后续可以做的事

- 编辑核心用的是原生 `<textarea>`，没有做语法高亮（按你的要求先把核心功能做稳）。如果之后想要语法高亮或者更彻底的大文件虚拟化编辑，需要换成 CodeMirror 6 或 Monaco 这类专门的编辑器内核，工作量会明显增加。
- 大文件模式下，行号栏做了虚拟渲染，但 `textarea` 本身仍然承载完整文本内容——这是原生输入框的技术限制，无法做到"只渲染可见行的文本内容"。
- 文件关联的注册表写入完成后，Windows 有时不会立刻刷新右键菜单显示，需要用户重新登录或重启 `explorer.exe`。
