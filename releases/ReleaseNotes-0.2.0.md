# AidTerm v0.2.0 — Release Notes

> 跨平台终端模拟器 · Tauri 2.0 + Vue 3 + Rust + Electron 双后端
> 支持平台：Windows / macOS / Linux (x86_64 + arm64)

---

## 新功能

### 分屏 (Split)
- 递归分屏：水平/垂直分屏，会话克隆，活动窗格高亮（固定 50/50，拖拽调大小待做）
- 分屏时保留终端内容，重定位后窗格保持交互
- 分屏菜单改为布局标签展示（左右 / 上下）
- 状态栏跟随选中窗格，分屏目标跟随选中窗格（per-tab）
- 单窗格隐藏选中轮廓，无选中窗格时禁用分屏
- Tab 切换时恢复每个 Tab 的分屏选择

### Tab 右键菜单
- 右键菜单：重命名、导出终端文本、分屏

### Electron 双后端
- Electron 后端支持（node-pty / ssh2 / serialport），兼容旧版 Linux 发行版
- 平台化窗口边框：win/mac 原生边框，WSLg 透明 + 手动缩放，Tauri 原生缩放
- 双后端统一 API 适配层（前端 `src/api` 自动检测）
- 打包后保留原生模块绑定文件，serialport / ssh2-sftp-client 正常加载

### 本地终端 Profiles
- 内置 shell 检测的本地终端 profiles（Windows cmd/powershell，unix bash/zsh）
- 跨平台默认 shell 自动探测

### AI 智能助手增强
- AI 助手改为全局右面板，命令在选中终端中执行，AI 对话 per-tab 隔离
- AI 命令输出分页读取（`read_output_page` 工具），长输出不再截断
- 移除 AI 命令执行超时，用户可随时停止等待
- AI 命令卡片头部新增红色方块停止按钮
- AI 面板头部显示主 Tab 标题 + 刷新图标新建对话
- Ollama tool calling 支持，移除 Anthropic
- AI system prompt 使用会话真实 shell 信息

### 主题与显示
- Catppuccin 主题替换为 VS Code Dark+/Light+ 配色
- 全局透明度改为按背景图的透明度（每背景图独立调节）
- 背景图经 `convertFileSrc` / data URL 加载，透明终端 + 背景图
- 原生 select 下拉框跟随暗/亮主题（color-scheme）
- Linux 标题栏显示窗口控制按钮

### 其他
- 可配置回滚行数（1000–1000000）+ 断开重连 overlay
- 所有会话结束路径统一发送 `session-status disconnected`，前端覆盖层可触发
- About 对话框与版本显示

---

## 修复（跨平台 issue #4–#22）
- 关闭 Tab 时杀死整个进程组，避免孤儿 shell 进程（#4）
- 私钥导入/存储 chmod 0600 权限收紧（#5）
- 打包后 CLI args 偏移修正，macOS 剥离 `-psn` 参数（#6）
- Local 端口转发改用 `ssh2 forwardOut`，不再依赖远程 `nc`（#7）
- 密钥生成/导入改用 `russh::keys`，移除 ssh-keygen 外部依赖（#8）
- 命令换行统一为 CR，触发器匹配行尾规范化（#10, #11）
- 本地 shell 标签标题平台化正确（#12）
- keychain 命令改用 `spawnSync` 数组参数，避免 shell 引号问题（#13）
- known_hosts 下溢 panic、添加时创建 `~/.ssh`、删除时保留非 host 行（#14）
- shell 探测检查存在性 + exec bit，移除 which/where 依赖（#15）
- 尊重已有 locale，以 C.UTF-8 兜底而非强制 en_US.UTF-8（#16）
- Windows 危险命令启发式 + AI 自动执行模式前缀（#17）
- IPv6 `host:port` 全链路支持：ssh/sftp/tunnel/proxy/快速连接/Deep Link/CLI 解析（#18）
- 远端架构用 `uname -m` 获取，不再依赖 `uname -a` 位置（#19）
- 内置会话组名改为 i18n 本地化而非硬编码中文（#20）
- macOS 允许 Cmd+Shift+I 切换 DevTools（#21）
- `ai_execute` 对 `cmd /C` 输出做 GBK 解码，命令经 `spawnSync` 数组传入（#22）

### 其他修复
- Electron Linux 空白屏 + node-pty 不可用问题
- Windows 子进程 `CREATE_NO_WINDOW`，避免黑窗闪烁
- 隐藏窗口模式下标题栏拖拽/最大化/还原的 Linux 处理
- `sftp_cancel_transfer` 双后端接线完成
- RSA bits 在密钥生成中生效，`execute_command` 工具描述对齐
- Vite 顶层 await / 静态导入警告 — 改懒加载与动态 import
- `detect_shells` 返回结构在 Tauri / Electron 间统一

---

## 重构
- AI / SFTP / Tunnel 统一为绑定主 Tab 的右侧边栏
- 分屏改为创建 wrapper 容器，Tab 保持为叶子节点
- Tauri 模式移除系统托盘
- Electron 构建输出调整、双后端 release 矩阵
- dev server 端口统一为 3000，平台 ID 规范化

---

## 项目 / 构建
- 新增 Electron 后端并完成打包：Electron（win exe、linux deb/AppImage、mac dmg）
- Tauri 打包补充 deb + AppImage
- release.yml 双后端三平台矩阵：Windows / Linux x86_64+arm64 / macOS arm64
- 添加作者邮箱、主页、maintainer 元数据（Linux deb）
- upload/download-artifact 升级 v4 → v5
- 单测 / E2E 仍待做（规划 Phase 11）

---

> 已知限制：端口转发 Local 仅限 Electron 端、代理为裸 TCP 直连、Zmodem / Deep Link 为占位、`write_text_file` 缺失。详见 AGENTS.md「双后端差异」。
