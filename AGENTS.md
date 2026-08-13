# AidTerm — 开发指南（AI / 贡献者）

**Tauri 2 + Vue 3 (Composition API + TypeScript) + Rust** 跨平台终端模拟器，另有 **Electron 双后端**（`src-electron/`，node-pty / ssh2 / serialport），前端 `src/api/index.ts` 自动检测并统一调用。

功能实现状态与阶段规划见 [docs/ROADMAP.md](docs/ROADMAP.md)（勾选反映代码实际状态）。

---

## 常用命令（仓库根目录）

| 命令 | 说明 |
|------|------|
| `npm run dev` | Vite 前端开发服务器 |
| `npm run build` | `vue-tsc --noEmit` 类型检查 + `vite build` |
| `npm run lint` | ESLint 检查 `src/`（改前端后必须跑） |
| `npm run format` | Prettier 格式化 |
| `npm run tauri dev` / `npm run tauri build` | Tauri 开发 / 构建 |
| `cargo check`（`src-tauri/`） | Rust 后端编译检查（改 Rust 后必须跑） |
| `npm run fetch-adb` / `npm run fetch-scrcpy` | 拉取并内置 adb / scrcpy-server 二进制 |
| `cd src-electron && npm run dev` / `npm run dist` | Electron 开发 / 打包 |

测试（`vitest` / `cargo test`）已规划，暂未搭建。

---

## 目录结构

```
src/                    # Vue 前端（Tauri 与 Electron 共用）
├── api/                # tauri.ts / electron.ts / index.ts 统一适配层 + types.ts
├── components/         # about ai editor file keychain lock proxy session
│                       # settings sidebar snippet status terminal titlebar
│                       # tools trigger tunnel
├── stores/             # terminal, session, settings, theme, ai, proxy, file(kind sftp/adb), snippet, trigger, tunnel, ui
├── hooks/              # useTerminal, useAiConversation, useTriggerWatcher
├── i18n/  router/  types/
src-electron/           # Electron 后端（node-pty / ssh2 / serialport）
src-tauri/              # Tauri Rust 后端
├── src/
│   ├── commands/       # Tauri IPC 命令
│   ├── session/ serial/ tunnel/ proxy/ keychain/ known_hosts/ ai/ adb/
│   └── *.rs            # cast.rs crypto.rs sftp.rs session_store.rs zmodem.rs 等
├── Cargo.toml / tauri.conf.json
docs/                   # ROADMAP.md（功能状态）DEVELOPMENT.md（开发指南）
bin/                    # fetch-adb.mjs / fetch-scrcpy.mjs（发布时内置打包）
```

---

## 开发原则

1. **Rust/Node 后端** 负责 I/O 密集型与系统调用：PTY、SSH、Serial、文件传输、加密
2. **Vue 前端** 负责 UI 渲染、交互、状态管理，通过统一 `api` 层（自动检测 Tauri/Electron）与后端通信
3. **所有 IPC 消息需有明确类型定义**，前后端共享 `src/api/types.ts`
4. 每个功能模块先验证可行性（POC 原型）再深入实现
5. 修改后必须跑 `npm run lint`（前端）或 `cargo check`（Rust）；改完先自查再让用户确认

---

## 关键一致性规则

### 版本管理
版本号手动维护在两处构建入口，发布时改这两处 + 写 `releases/ReleaseNotes-<版本>.md` + 打 `vX.Y.Z` 标签：
- Tauri：`src-tauri/Cargo.toml`（`tauri.conf.json` 不再写 version）
- Electron：`src-electron/package.json`
- `Cargo.lock` 与两个 `package-lock.json` 由 cargo/npm 自动维护，勿手改
- 前端 UI（TitleBar/About）版本号一律经 IPC `get_app_version` 获取，无硬编码

### 双后端差异（改动需同时或标注）
- Electron 缺失 / 占位：端口转发仅 Local、代理为裸 TCP 直连（无 HTTP CONNECT/SOCKS5 握手）、Zmodem 与 Deep Link 占位、`write_text_file` 缺失（终端文本导出会失败）
- Tauri 完整：Remote/Dynamic 转发、HTTP/SOCKS5/JumpHost、Deep Link、zmodem 检测
- ADB 双后端均完整：端口按 adb 来源切换（内置→5038 隔离、外部/系统→5037）+ 模拟器自动发现 + 5037 只读检测
- ADB 投屏（Cast）双后端均完整：Electron `cast.ts` 为 `cast.rs` 忠实 Node 移植，前端统一 `cast_*` IPC

### 断开/错误路径
所有会话结束路径必须发 `session-status disconnected`，保证前端覆盖层可触发。

---

## 待办优先级（完整清单见 docs/ROADMAP.md）

会话导出/导入、通知接入、Tab 拖拽排序与自定义主题、Guake/托盘、SSH 自动重连与指纹验证、SCP/Zmodem/Trzsz、AI 书签/MCP、单测/E2E、云同步/连接共享/录制回放、ssh-agent/X11。
