# AidTerm v0.1.0 — Release Notes

> 跨平台终端模拟器 · Tauri 2.0 + Vue 3 + Rust
> 支持平台：Windows / macOS / Linux (x86_64 + arm64)

---

## 新功能

### 终端核心
- 终端核心渲染完成（xterm.js + fit addon）— Phase 1
- 本地 Shell：Windows (cmd/powershell)、Linux/macOS (bash/zsh) 跨平台默认 shell 自动检测与修复
- PTY 窗口尺寸同步、Windows GBK 输出乱码修复
- 自定义标题栏：窗口控件、设置/锁定按钮、拖拽最大化
- 多 Tab 管理：新建、关闭、拖拽排序、右键菜单、终端状态保持
- IDE 布局：Splitpanes 可拖拽调整侧边栏/终端面板宽度
- 工具标签页系统 + 视图菜单重构
- 终端容器 4px 内边距优化

### SSH / Telnet / Serial
- SSH2 连接：密码认证 + 密钥认证 + 快速连接栏
- Telnet 连接
- 串口 (Serial) 连接 — tokio-serial 异步实现
- SSH 连接动画 overlay、连接错误终端内显示
- 记住密码选项 (AES-256-GCM 加密存储)
- 远程系统信息自动检测并显示在标签页标题
- 独立 SSH 连接获取远端系统信息，不侵入主终端
- 迁移 ssh-rs → russh + tokio-socks，支持 HTTP/SOCKS5/JumpHost 代理

### SFTP 文件管理
- SFTP 面板：根据当前 SSH 会话自动连接、可编辑路径输入
- 并发传输 + 后台 tokio 任务 + 实时进度条显示
- 拖拽上传（overlay + 多文件）
- 取消传输按钮（单任务取消 + 5s 自动清理）
- 创建文件/文件夹对话框（权限勾选）
- 删除确认对话框、行内操作菜单合并
- 上传/下载进度显示在底部传输队列中
- russh-sftp 协议替换 shell-based SFTP

### 端口转发 / 代理 / 隧道
- 端口转发模块 + 前端面板（Local/Remote/Dynamic）
- 代理配置面板：HTTP/SOCKS5/JumpHost
- 隧道状态轮询 + Running 状态展示

### 快捷命令 / Snippets
- 快捷命令 Snippets 系统
- 触发器 (Triggers) — 接收特定字符串时自动响应
- 批量输入 + 远程文件编辑

### AI 智能助手
- AI 助手侧边栏：自然语言检测 → 命令生成 → 终端执行 → 结果回传
- 命令危险分级 + 安全命令自动执行 + 执行状态指示器
- 三种 AI 触发模式：Auto / Prefix（多字符前缀）/ Keybinding（Ctrl+Enter）
- AI 对话隔离：每个终端 Tab 独立 sessionId
- Markdown 渲染（marked + AnsiRenderer）、表格边框、CJK 宽度优化
- 消息气泡复制按钮（hover 显示）、右键菜单、时间戳
- 发送按钮内嵌输入框右下角、圆形图标
- 最近 3 轮对话上下文带入新查询
- AI SDK 库替换手写 HTTP 请求、Provider 预设（OpenAI / DeepSeek / DashScope / Ollama / Anthropic）
- AI 设置面板：分步表单 + 模型列表从 API 拉取

### 会话管理
- 会话存储与分组、新建 Tab 下拉显示分组会话
- 会话导出/导入
- 保存的会话管理入口

### UI / UX
- 暗色/亮色主题系统（CSS 变量 + xterm 动态主题）
- i18n 多语言完成：全组件模板翻译 + 翻译键补全
- 标题栏 i18n 工具提示 + 双击最大化
- 全局右键菜单禁用 + 自定义输入上下文菜单
- 弹框点击空白处关闭行为移除
- 左侧边栏：固定像素宽度、工具/会话面板互斥

### 密钥管理
- RSA / ED25519 密钥生成 + 导入对话框
- known_hosts 管理（查看/删除）
- ssh-agent / X11 转发选项

### CI/CD
- 多平台 Release 工作流（Windows / macOS / Linux x86_64+arm64）
- AppImage xdg-utils 打包依赖

---

## 修复

### AI 助手
- AI 输入拦截全面修复（自然语言检测后 shell 不再执行缓存字符）
- AI 命令执行改用标记法检测完成，超时 Ctrl+C 打断
- AI 对话结束后光标消失 → 恢复显示与闪烁
- AI 多行文本 `\n` 缺 `\r` 导致每行递增右偏
- AI tool call chain 在 1-2 轮后断裂（missing tool_calls in saved history）
- AI 消息列表粗体不渲染 + 列表续行对齐 + 表格 CJK 宽度
- AI 过程中执行命令多出来的换行符
- AI 结束后发送回车让 shell 重新打印提示符
- AI 提示词使用终端会话的远端 OS 信息而非本地
- AI 执行命令前按回车刷新提示符，去掉 `$` 前缀显示
- 自动执行状态在命令输出后立即更新（而非等 AI 响应）
- 右键 Ask AI 在关闭菜单前获取选中文本

### 终端
- SSH 读超时导致连接断开（超时 10s→30s）
- SSH 连接成功后更新 session status 为 connected
- 连接检测不再抑制输出，终端欢迎信息正常显示
- 静默探测后刷新 shell 提示符，解决终端空白问题
- Tab 补全去掉 `\x03` 前缀避免 shell 打断产生多余空行
- `\r\n` → `\r` 避免 ICRNL 造成双换行
- 自然结束不发 `\x03` 避免自动 `^C` 混淆用户
- 终端编辑历史命令时字符覆盖/修改丢失/方向键不同步
- 去除 OSC 序列 (ESC]...BEL) 和 ANSI 括号粘贴序列
- Windows cmd 最后一行 prompt（如 `C:\Users\>`）不再被错误截断
- 禁用 F5 页面刷新
- 透明窗口启动闪烁 — 窗口默认隐藏，Vue 挂载后再显示

### SFTP
- Tauri 命令改为 async 防止 UI 冻结
- channel.eof() 解除上传阻塞
- 上传进度 UI
- Tab 关闭时断开、Tab 切换时 v-show 防止重连
- 右键菜单 `closeCtxMenu` 先于 action 执行导致功能无法触发
- 取消传输不再立即移除任务（使用 5s 自动清理）

### 其他
- Vite dep-scan EMFILE 错误 — 限制扫描范围为源码目录
- 跨平台兼容性修复
- Vue-i18n composition mode 切换
- `{{ }}` 转义防止 vue-i18n 编译器崩溃
- xterm 主题切换不生效（computed with DOM dep never re-evaluating）
- i18n key knownHosts snake_case→camelCase 修正

---

## 重构
- 工具移至 per-tab 侧边栏
- Sessions 面板按钮改为图标
- SFTP 地址输入合并为单行
- SFTP 操作按钮合并为行内 ⋮ 菜单
- SFTP 路由面包屑替换为可编辑路径
- emoji 图标替换为 Lucide SVG + remixicon
- AI 模式概念移除，终端始终为输入模式
- AI SDK 库统一字段命名

---

## 项目
- 项目从 TndTerm 重命名为 AidTerm
- 更新应用图标
- README 更新为公开文档
- 添加 `.gitattributes` 统一换行符 (LF)
