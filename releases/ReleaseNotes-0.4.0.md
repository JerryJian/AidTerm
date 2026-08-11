> 跨平台终端模拟器 · Tauri 2.0 + Vue 3 + Rust + Electron 双后端
> 支持平台：Windows / macOS / Linux (x86_64 + arm64)

---

## 新功能

### ADB（Android 设备）
- ADB shell 会话：内置 adb 使用独立 5038 server，外部/系统 adb（`AIDTERM_ADB`/PATH）使用默认 5037；模拟器自动发现；5037 被用户自建 server 占用时只读检测上报，不重启用户 server
- ADB 文件浏览：复用 SFTP 面板（`kind` 分发），目录列表 / 上传(`adb push`) / 下载(`adb pull`) / 新建 / 删除 / 重命名 / 远程编辑（`adb cat` + 临时文件 push）
- adb 二进制随安装包内置打包（`npm run fetch-adb`），无 adb 环境也可直接使用

### ADB 投屏 (Cast)
- 双后端完整支持：Tauri `cast.rs` 与 Electron `cast.ts` 忠实移植 scrcpy 官方 v4.1 wire 协议（dummy byte → 设备名 → codec id → session header → 帧头循环）
- 流保持 avc 格式直推前端 WebCodecs 解码渲染，config 包解析为 avcC 作为 decoder description
- 触摸(`tap/swipe`) / 滚轮 / 键盘经 `adb shell input` 注入
- 投屏面板全新布局并适配浅色主题
- 点击/滑动坐标映射到设备真实分辨率（`wm size`），而非缩放后的视频分辨率；设备旋转时自动对齐方向
- 新增 `max_fps=30`，降低带宽与解码压力
- Electron 端帧传输重构：后端解复用一次后经 MessageChannel 将二进制 `ArrayBuffer` 直接推送给渲染进程（免 base64 编解码），Tauri 端保持轮询，双端共用同一字节级解码核心
- `scrcpy-server.jar` 内置打包（`npm run fetch-scrcpy`），`AIDTERM_SCRCPY` 兜底

### 文件浏览（本地 / WSL）
- 本地目录文件浏览（复用文件面板）
- WSL 发行版选择对话框，支持在 WSL 发行版内浏览文件
- 本地 shell 配置与已安装 shell 实时同步，WSL 条目独立拆分

### 其他
- 连接与文件 API 在 Tauri / Electron 双后端统一
- 命令行 `--ssh user@host` 传参的双后端命令名统一
- AI 助手面板在未配置 API 时保持可见，便于随时设置

---

## 修复
- ADB 投屏花屏 / 黑屏：重构帧同步与关键帧恢复逻辑，解码出错等待下一关键帧自愈
- 投屏停止后内部状态未完全重置，导致再次启动画面模糊
- `wsl.exe -l -q` 输出无 BOM UTF-16LE 时发行版列表解析错误
- cast.rs 编译警告
- AI 模型列表拉取失败（列表为空）：`/models` 接口兼容 2025+ 新版 `data` 格式（`id/type/display_name`），双后端手动容错解析；失败时在设置面板红字显示接口错误原因
- AI 多轮问答报 `400 No tool output found for function call`：对话历史发送前规范化 tool_call/tool 配对，未得到输出的调用自动剔除、孤立 tool 消息丢弃；已取消/失败的命令以 tool 结果告知模型，避免历史携带悬空 tool_call

---

## 构建 / CI
- adb 打包精简为三个运行时文件，`fetch-adb` 脚本移入 `scripts/`
- 移除 `bin/.gitkeep` 占位文件及其在 electron-builder / README 中的引用
