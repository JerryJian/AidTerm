# AidTerm 功能清单与开发规划

> 本文档记录功能实现状态（勾选反映代码实际状态，2026-08 核对）与开发阶段规划。

---

## 功能清单

### 1. 会话协议支持
- [x] **SSH2** — 密码、密钥认证；端口转发（本地/远程/动态）
- [x] **Telnet** — 基础 Telnet 连接
- [x] **Serial (串口)** — 波特率、数据位、停止位、奇偶校验配置
- [x] **本地 Shell** — Windows (`cmd`, `powershell`)、Linux/macOS (`bash`, `zsh`)，自动探测
- [x] **SFTP** — 远程文件浏览、上传、下载、删除、重命名
- [x] **ADB** — Android 设备交互 shell（端口按 adb 来源切换：程序自带 adb 用独立 5038 server，外部/系统 adb（`AIDTERM_ADB`/PATH）用默认 5037；模拟器由 server 自动发现；5037 占用用裸 adb 协议只读检测，不重启用户 server；adb 二进制可用 `npm run fetch-adb` 内置打包，`AIDTERM_ADB`/PATH 兜底）
- [x] **ADB 文件浏览** — 复用 SFTP 面板（`fileStore` 按 `kind: 'sftp'|'adb'` 分发，tool_tab 复用 'sftp'），目录列表/上传(`adb push`)/下载(`adb pull`)/新建/删除/重命名/远程编辑(`adb cat` + 临时文件 push) 全部走 `-P <port> -s <serial>`（内置 adb→5038、外部/系统 adb→5037）；设备端路径经 `shq` 单引号转义后进 `adb shell`，ls 解析兼容 toybox/GNU 日期格式并剥离符号链接目标
- [x] **ADB 投屏 (Cast)** — 右侧面板 tab（Tauri `cast.rs` / Electron `cast.ts` 双后端）：scrcpy-server 独立版按官方 v4.1 协议跑（send_device_meta/send_stream_meta/send_frame_meta 全默认开），双后端忠实翻译官方 demuxer + packet_merger（dummy byte → 64B 设备名 → 4B codec id → 12B session header → 12B 帧头循环），config 包并入下一个媒体包并解析为 avcC，流保持 avc 格式直推前端 WebCodecs；触摸(`tap/swipe`)/滚轮/键盘经 `adb shell input` 注入；`scrcpy-server.jar` 用 `npm run fetch-scrcpy` 内置打包（版本须匹配 `SCRCPY_VERSION`），`AIDTERM_SCRCPY` 兜底
- [ ] **SCP** — 快速文件传输
- [ ] **Zmodem** — 后端接收检测已接线（SSH 流中发 `zmodem-start/end`），保存/接收循环为死代码待接线，前端交互/发送待做

### 2. 终端核心
- [x] **Terminal Emulation** — xterm.js，xterm-256color
- [x] **多 Tab 管理** — 新建、关闭、重命名、右键菜单（拖拽排序待做）
- [x] **分屏 (Split)** — 水平/垂直分屏（自定义 flex 树，固定 50/50，拖拽调大小待做）
- [x] **搜索** — xterm-addon-search 正向/反向搜索，高亮匹配
- [x] **选中即复制 / 右键粘贴** — 右键菜单 + 配置
- [x] **无限回滚** — 大缓存滚动（1000–1000000 行可配置）
- [x] **命令历史** — 右侧面板 Tab：按用户输入（Enter 提交 + 提示符上下文，Tab 补全/↑↓ 记忆按显示行解析）自动记录每条执行命令，按终端/分屏面板隔离；支持搜索、点击执行、单条删除、清空（记忆命令/带颜色提示符 OK，进度条/启动横幅不会误录）
- [x] **字体与主题 (Theme)** — 暗/亮主题、透明度、背景图已实现；自定义配色与字体待做

### 3. 会话管理
- [x] **会话存储** — 分组、最近使用（`last_connected` 记录，列表 UI 待做）
- [x] **快速连接栏** — 输入 `ssh user@host` 直接连接
- [ ] **会话导出/导入** — JSON 格式（已有终端文本导出，无会话级导入导出）
- [ ] **连接共享** — 多个 Tab 共享同一 SSH 连接（跳板机场景）

### 4. UI / UX
- [x] **多语言 (i18n)** — 中文/英文（vue-i18n）
- [ ] **快捷键自定义** — 仅内置 F11 全屏 / F5 / Ctrl+Shift+I，暂不可配置
- [x] **锁屏** — 本地密码锁屏界面（明文存 localStorage，crypto 加密待做）
- [x] **全屏模式** — F11 / 设置开关
- [x] **透明度调节** — 设置面板滑块（亚克力/毛玻璃待做）
- [ ] **通知** — 未接入（Tauri 未装 plugin-notification，Electron 无实现）

### 5. 安全与密钥
- [x] **密钥管理** — 生成 RSA/ED25519 密钥对，导入/删除（KeyManagerPanel）
- [ ] **Pageant/ssh-agent 转发** — 参数已预留，后端未实现
- [ ] **X11 转发** — 参数已预留，后端未实现
- [x] **known_hosts 管理** — 列表/添加/删除已实现；指纹验证弹窗待做（当前 `check_server_key` 恒返回 true）

### 6. 高级
- [x] **端口转发（Tunneling）** — Local / Remote / Dynamic（TunnelPanel）
- [x] **代理支持** — HTTP / SOCKS5 / Jump Host（ProxyPanel + tokio-socks）
- [x] **宏 / 触发器 (Macro/Triggers)** — 匹配终端输出 → 自动响应（TriggerPanel + useTriggerWatcher）
- [ ] **录制与回放** — 会话录制为文本或 ANSI 日志
- [x] **一键快捷命令 (Snippets)** — 命令列表 + `{{var}}` 变量替换（SnippetPanel）
- [x] **批量输入** — 同时向多个终端发送命令（TabBar 批量模式）
- [x] **远程文件编辑** — 双击远程文件，内置编辑器直接修改（FileEditor）

### 7. AI 智能助手（electerm 特色）
- [x] **AI 命令建议** — 集成 OpenAI 兼容 API（DeepSeek/DashScope/Ollama），自然语言 → 命令；`execute_command` 工具内联展示供用户确认执行，输出回传给 AI 继续推理
- [x] **脚本生成** — AI 对话生成 shell 脚本 / 命令
- [x] **终端内容解释** — 选中终端输出，右键「AI 解释选中内容」
- [ ] **AI 智能创建书签** — 自然语言描述连接目标，AI 辅助填写参数
- [ ] **MCP 协议支持** — Model Context Protocol
- [ ] **MCP Widget** — 供 AI 助手和外部工具集成的组件

### 8. 云同步与协作
- [ ] **GitHub Gist 同步** — 书签/主题/快捷命令同步到 GitHub Secret Gist
- [ ] **WebDAV 同步** — 自建服务端同步
- [ ] **electerm 云兼容** — 可选对接 electerm 云服务

### 9. 特色体验
- [ ] **全局快捷键（Guake 模式）** — i18n 有占位，功能未实现
- [x] **终端背景图** — 设置面板可选背景图片
- [x] **Deep Link** — tauri-plugin-deep-link（仅 Tauri；Electron 未实现）
- [x] **命令行传参** — 双端均经 `cli_args` IPC：Electron 读 `process.argv`，Tauri 返回 `std::env::args`（前端 `App.vue` 解析 `--ssh user@host[:port]` 预填 SSH 对话框）
- [x] **检查更新** — 关于对话框：GitHub Releases 检查最新版 → 下载安装包 → 调起安装（Tauri `commands/update.rs` / Electron `check_for_update` 等 IPC；Windows 按注册表 Uninstall 键检测安装包类型并下载对应 MSI/EXE，`msiexec /i ... /qn` 或 `/S` 静默装、macOS `open` dmg、Linux 运行 AppImage）
- [ ] **Trzsz 文件传输** — tmux 兼容的 Zmodem 替代方案
- [ ] **MCP Widget** — 供 AI 助手和外部工具集成的组件

### 10. 开发者 / 集成
- [ ] **插件系统** — 社区扩展（远景）
- [ ] **REST API / CLI 控制** — 外部控制终端（远景）

---

## 开发任务规划

### Phase 0: 项目工程搭建
| # | 任务 | 说明 |
|---|------|------|
| 0.1 | 初始化项目 | Vite + Vue Router + Pinia ✅ |
| 0.2 | 后端结构设计 | `src-tauri/src/{session,serial,sftp,tunnel,proxy,keychain,ai}` 模块化 ✅（未拆 `crates/`） |
| 0.3 | ESLint / Prettier | `npm run lint` / `npm run format` ✅（Husky 未装） |
| 0.4 | AGENTS.md 与开发文档 | 本文件与 README ✅ |

### Phase 1: 终端核心渲染
| # | 任务 | 说明 |
|---|------|------|
| 1.1 | 集成 Web Terminal 模拟器 | xterm.js + fit + web-links ✅ |
| 1.2 | 前后端 IPC 通道 | `invoke` + `event` 双向流 ✅ |
| 1.3 | 本地 Shell 启动 | Rust portable-pty / Electron node-pty ✅ |
| 1.4 | Tab 容器 | 多 Tab 管理 + 右键菜单 ✅（拖拽排序待做） |
| 1.5 | 分屏 (Split) | 水平/垂直，自定义 flex 树 ✅（拖拽调大小待做） |
| 1.6 | 终端搜索 | xterm-addon-search ✅ |
| 1.7 | 无限回滚 | scrollback 可配置（1000–1000000 行）✅ |
| 1.8 | 主题与配色 | 暗/亮 + 透明度 + 背景图 ✅（自定义配色/字体待做） |

### Phase 2: 会话协议
| # | 任务 | 说明 |
|---|------|------|
| 2.1 | SSH2 连接 | russh（Rust）/ ssh2（Electron），密码 + PTY shell ✅ |
| 2.2 | SSH 密钥认证 | 加载私钥 + 文件选择器 ✅ |
| 2.3 | Telnet 连接 | Rust `telnet` crate ✅ |
| 2.4 | Serial 连接 | tokio-serial + SerialDialog 参数配置 ✅ |
| 2.5 | 连接复用 + 重连 | 断开覆盖层：Enter / 按钮 / 右键菜单 ✅；所有会话结束路径统一发 `session-status disconnected` ✅；自动重试待做 |
| 2.6 | 快速连接栏 | `ssh user@host` 解析打开 SSH 对话框 ✅ |

### Phase 3: 会话管理
| # | 任务 | 说明 |
|---|------|------|
| 3.1 | 会话存储与分组 | Pinia + 本地 JSON 持久化（load/save_session_store）✅ |
| 3.2 | 会话导入/导出 | JSON 格式待做 |
| 3.3 | 最近使用列表 | `last_connected` 已记录，列表 UI 待做 |

### Phase 4: 文件传输
| # | 任务 | 说明 |
|---|------|------|
| 4.1 | SFTP 面板 | russh-sftp + SftpPanel ✅ |
| 4.2 | 拖拽上传/下载 | `tauri://drag-drop` 拖入上传 ✅ |
| 4.3 | Zmodem 集成 | 后端接收检测已接线，保存/接收循环为死代码待接线，前端待做 |

### Phase 5: 高级功能
| # | 任务 | 说明 |
|---|------|------|
| 5.1 | 端口转发管理 | Local/Remote/Dynamic（TunnelPanel + tunnel 后端）✅ |
| 5.2 | 代理配置 | HTTP/SOCKS5/Jump Host ✅ |
| 5.3 | 快捷命令 (Snippets) | 命令 + `{{var}}` 变量替换 ✅ |
| 5.4 | 触发器 (Triggers) | 匹配输出 → 自动响应 ✅ |
| 5.5 | 批量输入 | TabBar 批量勾选 + 批量发送 ✅ |
| 5.6 | 远程文件编辑 | FileEditor + sftp_read/write_file ✅ |
| 5.7 | 远程系统监控 | 无 Agent：SSH exec 采集 /proc/stat、meminfo、loadavg、df、/proc/net/dev，前端 MonitorPanel 每 2s 轮询显示 CPU/内存/磁盘/网络 ✅ |
| 5.8 | 本地系统监控 | local/wsl 会话同样支持资源监控：Tauri 用 sysinfo、Electron 用 systeminformation 采集本机 CPU/内存/磁盘/网络/负载，GPU 尽力而为（nvidia-smi/rocm-smi/intel_gpu_top）✅ |

### Phase 6: UI 完善
| # | 任务 | 说明 |
|---|------|------|
| 6.1 | i18n 多语言 | 中文 + 英文 ✅ |
| 6.2 | 全局快捷键（Guake 模式） | 待做（含终端内快捷键自定义） |
| 6.3 | 锁屏 | LockScreen（密码明文存 localStorage，加密待做）✅ |
| 6.4 | 窗口透明度 + 背景图 | 设置面板滑块 + 背景图选择 ✅ |
| 6.5 | 全屏模式 | F11 / 设置 ✅ |
| 6.6 | 系统托盘 | 待做 |
| 6.7 | Deep Link 协议 | tauri-plugin-deep-link ✅（仅 Tauri；Electron 未实现） |
| 6.8 | 命令行传参 | 双端均经 `cli_args` IPC：Electron `process.argv`，Tauri `std::env::args`，前端解析 `--ssh user@host[:port]` 预填 SSH 对话框 ✅ |

### Phase 7: AI 智能助手
| # | 任务 | 说明 |
|---|------|------|
| 7.1 | AI 终端助手 | 自然语言 → AI 生成命令 → `execute_command` 工具内联确认执行 → 输出回传继续推理 ✅ |
| 7.2 | AI 配置面板 | Provider/Model/BaseURL/API Key 配置 ✅ |
| 7.3 | 终端内容解释 | 右键「AI 解释选中内容」✅ |
| 7.4 | AI 书签 | 待做 |
| 7.5 | MCP 协议支持 | 待做 |
| 7.6 | MCP Widget | 待做 |

### Phase 8: 安全与密钥
| # | 任务 | 说明 |
|---|------|------|
| 8.1 | 密钥生成 | RSA/ED25519 生成 + 导入/删除 ✅ |
| 8.2 | ssh-agent 转发 | 待做（参数已预留） |
| 8.3 | X11 转发 | 待做（参数已预留） |
| 8.4 | known_hosts 管理 | 列表/增删 ✅（指纹验证弹窗待做） |

### Phase 9: 云同步与协作
| # | 任务 | 说明 |
|---|------|------|
| 9.1 | GitHub Gist 同步 | 待做 |
| 9.2 | WebDAV 同步 | 待做 |
| 9.3 | electerm 云兼容 | 待做 |

### Phase 10: 文件传输增强
| # | 任务 | 说明 |
|---|------|------|
| 10.1 | Trzsz 支持 | 待做 |
| 10.2 | SCP 快速传输 | 待做 |

### Phase 11: 测试与发布
| # | 任务 | 说明 |
|---|------|------|
| 11.1 | 单元测试 | Rust `#[cfg(test)]` + Vue `vitest` 待做 |
| 11.2 | E2E 测试 | Tauri `webdriver` / Playwright 待做 |
| 11.3 | CI/CD | `.github/workflows/release.yml` ✅（GitHub Actions 矩阵：win-x64 / linux x64+arm64 / mac arm64，Tauri + Electron 双后端三平台构建） |
| 11.4 | 安装包 | 经 release.yml 产出：Tauri（win exe/msi、linux deb/AppImage、mac dmg）+ Electron（win exe、linux deb/AppImage、mac dmg）✅ |
| 11.5 | 检查更新 | GitHub Releases 检查 + 下载安装包调起安装（`commands/update.rs` / Electron IPC；Windows 按注册表 `WindowsInstaller` 值检测 MSI/NSIS 下载对应包，`msiexec /i ... /qn` 或 `/S`、macOS `open` dmg、Linux AppImage）✅ |

---

## 待办优先级建议（按当前缺口）

1. 会话导出/导入（3.2）+ 最近使用列表（3.3）
2. 通知接入（需先装 tauri-plugin-notification）
3. Tab 拖拽排序（1.4）、自定义主题配色/字体（1.8）
4. Guake 模式 / 全局快捷键（6.2）、系统托盘（6.6）
5. SSH 自动重连（2.5）、指纹验证弹窗（8.4）
6. SCP（10.2）、Zmodem 前端（4.3）、Trzsz（10.1）
7. AI 书签（7.4）、MCP（7.5/7.6）
8. 单测 / E2E（11.1/11.2）
9. 云同步（9.x）、连接共享（3.4）、录制回放（6.x）
10. ssh-agent / X11 转发（8.2/8.3）
