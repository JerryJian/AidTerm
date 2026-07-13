# AidTerm 开发指南

AidTerm 同时支持 **Tauri 2.0** (Rust 后端) 和 **Electron** (Node.js 后端) 两种桌面运行时。

前端代码 (`src/`) 通过 `src/api/index.ts` 自动检测运行时环境，统一调用 `invoke` / `listen` 等 API，业务代码无需关心底层实现。

---

## 1. 环境准备

### 1.1 通用依赖

```bash
# Node.js >= 18
node -v

# 安装前端依赖（Tauri 和 Electron 共用）
npm install
```

### 1.2 Tauri 专用依赖

```bash
# Rust 工具链（https://rustup.rs）
rustc --version

# 安装 Tauri CLI
npm install -g @tauri-apps/cli
# 或使用项目内 npx
npx tauri --version
```

**Linux 额外系统依赖：**

```bash
sudo apt-get update
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libxdo-dev \
  libssl-dev \
  librsvg2-dev \
  patchelf \
  libayatana-appindicator3-dev \
  xdg-utils
```

### 1.3 Electron 专用依赖

```bash
cd src-electron
npm install
```

> `node-pty`、`serialport` 等原生模块会自动编译，确保系统安装了对应的 C/C++ 构建工具。  
> Windows：安装 [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)，勾选"Desktop development with C++"。  
> macOS：`xcode-select --install`  
> Linux：`sudo apt-get install build-essential`

---

## 2. 开发调试

### 2.1 Tauri 模式（默认）

```bash
npm run tauri dev
```

- Vite 前端 dev server 启动在 `http://localhost:1420`
- Rust 后端热重载（仅重新编译变化的 crate）
- 前端 HMR 热更新即时生效
- DevTools：菜单栏 → Window → Toggle DevTools，或代码中调用 `invoke('open_devtools')`

**常用调试技巧：**

```bash
# 仅构建前端（跳过 Rust 编译，验证 Vue/TS 是否通过）
npm run build

# 仅检查 TypeScript 类型
npx vue-tsc --noEmit

# 仅运行 lint
npm run lint
```

### 2.2 Electron 模式

需要开两个终端：

**终端 1 — 启动 Vite 前端 dev server：**

```bash
npm run dev
# Vite server 启动在 http://localhost:1420
```

**终端 2 — 启动 Electron 主进程：**

```bash
cd src-electron
npm run dev
# 编译 TypeScript → dist/main.js，然后 electron dist/main.js
```

- Electron 主进程连接 Vite dev server (`http://localhost:1420`)
- 前端代码变更通过 Vite HMR 热更新
- 主进程代码变更需重启 Electron（手动重跑 `npm run dev`）
- DevTools：自动开启，或菜单栏 View → Toggle DevTools

**注意事项：**

- `src-electron/main.ts` 修改后需重新 `npm run dev`（或单独 `npm run build` 仅编译 TS）
- 主进程中的 `console.log` 输出在 Electron 终端窗口中查看
- Preload 脚本 (`preload.ts`) 也需重新编译，已包含在 `npm run dev` 中

### 2.3 仅调试前端（浏览器模式）

```bash
npm run dev
# 打开 http://localhost:1420
```

在浏览器中打开可快速调试 UI，但 `invoke()` 调用会失败（无 Tauri/Electron 运行时），需要 mock。

---

## 3. 生产构建

### 3.1 Tauri 构建

```bash
# 构建当前平台的安装包
npm run tauri build

# 构建指定 target（交叉编译）
npx tauri build --target x86_64-unknown-linux-gnu
npx tauri build --target aarch64-apple-darwin
npx tauri build --target x86_64-pc-windows-msvc
```

**产物位置：**

| 平台 | 路径 |
|------|------|
| Windows | `src-tauri/target/release/bundle/nsis/AidTerm_0.1.0_x64-setup.exe` |
| macOS | `src-tauri/target/release/bundle/dmg/AidTerm_0.1.0_aarch64.dmg` |
| Linux | `src-tauri/target/release/bundle/appimage/AidTerm_0.1.0_amd64.AppImage` |

### 3.2 Electron 构建

```bash
cd src-electron

# 编译 TypeScript
npm run build

# 打包安装包（electron-builder）
npm run dist
```

**产物位置：**

| 平台 | 路径 |
|------|------|
| Windows | `src-electron/release/AidTerm Setup 0.1.0.exe` |
| macOS | `src-electron/release/AidTerm-0.1.0.dmg` |
| Linux | `src-electron/release/AidTerm-0.1.0.AppImage` |

**electron-builder 配置在 `src-electron/package.json` 的 `build` 字段中。**

### 3.3 CI/CD

GitHub Actions 自动构建 Tauri 版本（参见 `.github/workflows/release.yml`）：

- 推送 `v*` tag 或手动触发 workflow
- 矩阵构建：Windows / macOS (x86_64 + aarch64) / Linux (x86_64 + aarch64)
- 产物上传为 workflow artifacts + draft release

Electron 版本的 CI 构建可参照 Tauri workflow 添加，使用 `electron-builder` 替换 `tauri-action`。

---

## 4. 架构说明

### 4.1 运行时自动检测

```typescript
// src/api/index.ts
const isElectron = !!(window as any).electronAPI

if (isElectron) {
  mod = await import('./electron')    // Electron: 通过 preload 桥接
} else {
  mod = await import('./tauri')       // Tauri: 直接调用 @tauri-apps/api
}
```

前端所有 `import { invoke, listen } from '@/api'` 自动适配两种运行时，**业务代码无需修改**。

### 4.2 文件结构

```
src/                  # 前端代码（共享）
├── api/
│   ├── index.ts      # 自动检测运行时
│   ├── types.ts      # 共享类型定义
│   ├── tauri.ts      # Tauri API 适配
│   └── electron.ts   # Electron API 适配
├── components/
├── stores/
├── hooks/
└── ...

src-tauri/            # Tauri 后端（Rust）
├── src/
│   ├── commands/     # IPC 命令
│   ├── session/      # SSH/Telnet/Local
│   ├── sftp.rs       # SFTP
│   ├── ai/           # AI 助手
│   └── ...
└── Cargo.toml

src-electron/         # Electron 后端（Node.js）
├── main.ts           # 主进程（所有 IPC 处理器）
├── preload.ts        # contextBridge 桥接
├── package.json
└── tsconfig.json
```

### 4.3 两种运行时的差异

| 特性 | Tauri | Electron |
|------|-------|----------|
| 后端语言 | Rust | Node.js |
| PTY | `portable-pty` | `node-pty` |
| SSH | `russh` (纯 Rust) | `ssh2` (Node.js) |
| SFTP | `russh-sftp` | `ssh2-sftp-client` |
| Serial | `tokio-serial` | `serialport` |
| 密码加密 | AES-256-GCM (Rust) | AES-256-CBC (Node.js crypto) |
| 包体积 | ~5-15 MB | ~80-150 MB |
| 内存占用 | 较低 | 较高 |
| 系统要求 | WebKit2GTK 4.1 (Linux) | 无特殊要求 |
| 原生模块 | Rust crates | node-gyp 编译 |

---

## 5. 常见问题

### Q: Electron 模式下 `spawn_terminal` 报错？
确保 `src-electron/node_modules` 已安装，特别是 `node-pty`。如果编译失败，检查系统是否有 C++ 构建工具。

### Q: Tauri 模式下 Linux 缺少 WebKit2GTK？
AidTerm 使用 Tauri 2.0，要求 **WebKit2GTK 4.1**（非 4.0）。Ubuntu 20.04 及更早版本不支持，需升级系统或使用 Electron 版本。

### Q: 如何切换前端代码使用的运行时？
无需手动切换。`src/api/index.ts` 在运行时自动检测 `window.electronAPI` 是否存在：
- Electron 模式：preload 脚本通过 `contextBridge` 暴露 `electronAPI`
- Tauri 模式：`@tauri-apps/api` 直接注入到 window

### Q: Electron 主进程修改后需要重启吗？
是的。`src-electron/main.ts` 是 Node.js 主进程代码，不支持 HMR，修改后需重新运行 `cd src-electron && npm run dev`。

### Q: 两种模式的安装包能同时构建吗？
可以。Tauri 和 Electron 的构建是完全独立的：
- Tauri：`npm run tauri build`
- Electron：`cd src-electron && npm run dist`
