# TndTerm — Tauri + Vue3 复刻 WindTerm 开发规划

## 项目概述
使用 **Tauri 2.x** + **Vue 3 (Composition API + TypeScript)** + **Rust** 后端，构建跨平台终端模拟器，对标 WindTerm 核心功能。

---

## WindTerm 功能清单

### 1. 会话协议支持
- [ ] **SSH2** — 密码、密钥、键盘交互认证；端口转发（本地/远程/动态）
- [ ] **Telnet** — 基础 Telnet 连接
- [ ] **Serial (串口)** — 波特率、数据位、停止位、奇偶校验配置
- [ ] **本地 Shell** — Windows (`cmd`, `powershell`)、Linux/macOS (`bash`, `zsh`)
- [ ] **SFTP** — 远程文件浏览、上传、下载、删除、重命名
- [ ] **SCP** — 快速文件传输
- [ ] **Zmodem** — 通过终端进行文件传输

### 2. 终端核心
- [ ] **Terminal Emulation** — xterm-256color 兼容，VT100/VT200 序列支持
- [ ] **多 Tab 管理** — 新建、关闭、重命名、拖拽排序
- [ ] **分屏 (Split)** — 水平/垂直分屏，任意调整大小
- [ ] **搜索** — 终端内容正向/反向搜索，高亮匹配
- [ ] **选中即复制 / 右键粘贴** — 可配置
- [ ] **无限回滚** — 大缓存滚动
- [ ] **字体与主题 (Theme)** — 自定义配色方案、字体、透明度、背景图

### 3. 会话管理
- [ ] **会话存储** — 分组、收藏、最近使用
- [ ] **快速连接栏** — 输入 `ssh user@host` 直接连接
- [ ] **会话导出/导入** — JSON / XML 格式
- [ ] **连接共享** — 多个 Tab 共享同一 SSH 连接（跳板机场景）

### 4. UI / UX
- [ ] **多语言 (i18n)** — 中文/英文/日文等
- [ ] **快捷键自定义** — 全局快捷键、终端内快捷键均可配置
- [ ] **锁屏** — 终端密码锁定
- [ ] **全屏模式**
- [ ] **透明度调节** — 亚克力/毛玻璃效果（Windows 11 / macOS）
- [ ] **通知** — 连接断开、命令完成

### 5. 安全与密钥
- [ ] **密钥管理** — 生成 RSA/ED25519 密钥对，导入/导出私钥
- [ ] **Pageant/ssh-agent 转发**
- [ ] **known_hosts 管理**

### 6. 高级
- [ ] **端口转发（Tunneling）** — Local / Remote / Dynamic（SOCKS5 代理）
- [ ] **代理支持** — HTTP / HTTPS / SOCKS5 / 跳板机 (Jump Host)
- [ ] **宏 / 触发器 (Macro/Triggers)** — 接收到特定字符串时自动发送命令
- [ ] **录制与回放** — 会话录制为文本或 ANSI 日志
- [ ] **一键快捷命令 (Snippets)** — 带参数变量的命令面板

### 7. 开发者 / 集成
- [ ] **插件系统** — 社区扩展（远景）
- [ ] **REST API / CLI 控制** — 外部控制终端（远景）

---

## 开发任务规划

### Phase 0: 项目工程搭建
| # | 任务 | 说明 |
|---|------|------|
| 0.1 | 初始化 Tauri 2.x + Vue 3 + TypeScript 项目 | `create-tauri-app`，集成 `Vite` + `Vue Router` + `Pinia` |
| 0.2 | Rust 后端结构设计 | `crates/` 分 crate：`terminal-core`, `ssh-client`, `serial-client`, `session-manager` |
| 0.3 | 配置 ESLint / Prettier / Husky | 统一代码风格 |
| 0.4 | 建立 AGENTS.md 与开发文档 | 本文件 |

### Phase 1: 终端核心渲染
| # | 任务 | 说明 |
|---|------|------|
| 1.1 | 集成 Web Terminal 模拟器 | 基于 `xterm.js` + `xterm-addon-fit` + `xterm-addon-web-links` |
| 1.2 | 前后端 IPC 通道 | Tauri `invoke` + `event` 实现 stdin/stdout 双向流 |
| 1.3 | 本地 Shell 启动 | Rust 端 `Command::new("bash")` / `cmd.exe`，分配 PTY (portable-pty) |
| 1.4 | Tab 容器 | 基于 Vue 实现多 Tab 管理，支持拖拽排序、右键菜单 |
| 1.5 | 分屏 (Split) | 实现水平/垂直分屏布局，基于 flexbox / grid |
| 1.6 | 终端搜索 | xterm-addon-search |
| 1.7 | 无限回滚 | xterm-addon-scrollback 或自定义缓冲区 |
| 1.8 | 主题与配色 | xterm 主题 + CSS 变量，支持暗色/亮色/自定义 |

### Phase 2: 会话协议
| # | 任务 | 说明 |
|---|------|------|
| 2.1 | SSH2 连接 | Rust 端 `ssh2` / `thrussh` crate，实现连接、认证、pty |
| 2.2 | SSH 密钥认证 | 加载 `~/.ssh/id_rsa` / `id_ed25519`，passphrase 弹窗 |
| 2.3 | Telnet 连接 | Rust 端或纯 JS `telnet-client` |
| 2.4 | Serial 连接 | Rust 端 `serialport` crate |
| 2.5 | 连接复用 + 重连 | 断线自动重试（可配置） |
| 2.6 | 快速连接栏 | 输入 `ssh user@host` 直接解析并连接 |

### Phase 3: 会话管理
| # | 任务 | 说明 |
|---|------|------|
| 3.1 | 会话存储与分组 | Pinia + 本地 JSON/Toml 持久化或 Tauri `store` plugin |
| 3.2 | 会话导入/导出 | JSON 格式导出，支持拖入文件导入 |
| 3.3 | 最近使用列表 | 快速切换 |

### Phase 4: 文件传输
| # | 任务 | 说明 |
|---|------|------|
| 4.1 | SFTP 面板 | Rust `ssh2` 的 sftp 子系统，Vue 文件管理器 UI |
| 4.2 | 拖拽上传/下载 | 从系统拖入自动上传 |
| 4.3 | Zmodem 集成 | Rust 端 `lrzsz` 协议实现或调用系统 `rz/sz` |

### Phase 5: 高级功能
| # | 任务 | 说明 |
|---|------|------|
| 5.1 | 端口转发管理 | Local/Remote/Dynamic 配置面板，SSH 隧道保持 |
| 5.2 | 代理配置 | HTTP/SOCKS5 代理设置，Jump Host 链式跳转 |
| 5.3 | 快捷命令 (Snippets) | 命令列表 + 变量替换，支持快捷键发送 |
| 5.4 | 触发器 (Triggers) | 匹配终端输出 → 自动响应 / 告警 |

### Phase 6: UI 完善
| # | 任务 | 说明 |
|---|------|------|
| 6.1 | i18n 多语言 | `vue-i18n`，中文 + 英文 |
| 6.2 | 快捷键系统 | 全局快捷键注册 + 配置 UI |
| 6.3 | 锁屏 | 本地密码加密存储，锁屏界面 |
| 6.4 | 窗口透明度 | Tauri `window.setTransparent()` |
| 6.5 | 全屏模式 | Tauri 全屏 API |
| 6.6 | 系统托盘 | Tauri tray plugin，后台运行 |

### Phase 7: 安全与密钥
| # | 任务 | 说明 |
|---|------|------|
| 7.1 | 密钥生成 | Rust `rsa` / `ed25519-dalek` 生成密钥对 |
| 7.2 | ssh-agent 转发 | 对接系统 ssh-agent / Pageant |
| 7.3 | known_hosts 管理 | 导入 `known_hosts`，指纹验证弹窗 |

### Phase 8: 测试与发布
| # | 任务 | 说明 |
|---|------|------|
| 8.1 | 单元测试 | Rust `#[cfg(test)]` + Vue `vitest` |
| 8.2 | E2E 测试 | Tauri `webdriver` + Playwright |
| 8.3 | CI/CD | GitHub Actions 构建 Windows / macOS / Linux 安装包 |
| 8.4 | 安装包 | NSIS (Windows) / DMG (macOS) / AppImage (Linux) |

---

## 技术栈

| 层 | 技术 |
|----|------|
| 桌面框架 | Tauri 2.x (Rust) |
| 前端 | Vue 3 + TypeScript + Vite |
| 状态管理 | Pinia |
| 路由 | Vue Router |
| 终端渲染 | xterm.js + addons |
| 样式 | UnoCSS / Tailwind CSS |
| 后端语言 | Rust |
| SSH | `ssh2` crate (libssh2 绑定) |
| Serial | `serialport` crate |
| PTY | `portable-pty` / `smithay-terminal-tools` |
| 密钥 | `rsa`, `ed25519-dalek`, `ssh-key` |

---

## 目录结构

```
TndTerm/
├── src/                    # Vue 前端
│   ├── assets/
│   ├── components/
│   │   ├── terminal/       # TerminalWrapper, TabBar, SplitPane
│   │   ├── session/        # SessionManager, QuickConnect
│   │   ├── sftp/           # FileBrowser, TransferQueue
│   │   └── settings/       # SettingsPanel, ThemeEditor
│   ├── stores/             # Pinia stores
│   ├── i18n/               # 多语言
│   ├── hooks/              # composables
│   ├── router/
│   ├── types/              # TS 类型定义
│   └── App.vue
├── src-tauri/              # Tauri Rust 后端
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── commands/       # Tauri IPC 命令
│   │   ├── session/        # 会话管理
│   │   ├── ssh/            # SSH 客户端
│   │   ├── serial/         # 串口客户端
│   │   ├── telnet/         # Telnet 客户端
│   │   ├── local/          # 本地 Shell
│   │   ├── sftp/           # SFTP 文件传输
│   │   ├── tunnel/         # 端口转发
│   │   └── keychain/       # 密钥管理
│   ├── Cargo.toml
│   └── tauri.conf.json
├── AGENTS.md
└── package.json
```

---

## 开发原则

1. **Rust 后端** 负责 I/O 密集型与系统调用：PTY、SSH、Serial、文件传输、加密
2. **Vue 前端** 负责 UI 渲染、交互、状态管理，通过 Tauri `invoke` / `event` 与后端通信
3. **每个功能模块先验证可行性**（POC 原型）再深入实现
4. **优先完成 Phase 0–1**（能跑终端 + 本地 Shell），后续按优先级迭代
5. **所有 IPC 消息需有明确的类型定义**，前后端共享 `types`（通过 Tauri 的 `tauri-specta` 或手动维护）

---

## 首批开发任务（从 Phase 0 开始）

1. `npm create tauri-app@latest` 初始化项目
2. 集成 `xterm.js`，在 Vue 组件中渲染
3. Rust 端启动本地 Shell 并通过 IPC 连接 xterm.js
4. 实现多 Tab 容器
5. 实现 SSH 连接
