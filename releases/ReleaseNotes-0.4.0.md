> 跨平台终端模拟器 · Tauri 2.0 + Vue 3 + Rust + Electron 双后端
> 支持平台：Windows / macOS / Linux (x86_64 + arm64)

---

## 新功能

### Android ADB
- ADB shell 会话：内置/系统 adb 均可连接 Android 设备（内置 adb 用隔离的 5038 服务端口，外部/系统 adb 用默认 5037，模拟器自动发现，5037 被占用时只读检测不干扰用户 server）
- ADB 文件浏览：复用 SFTP 面板，支持目录列表、上传(`adb push`)、下载(`adb pull`)、新建/删除/重命名、远程编辑
- adb 二进制可选内置打包（`npm run fetch-adb`），`AIDTERM_ADB`/PATH 兜底

### ADB 投屏 (Cast)
- 右侧面板投屏 tab：scrcpy-server 独立版 + WebCodecs 解码（avc 格式直推），支持点击/滑动/滚轮/键盘输入注入
- 投屏核心逻辑重构，解决花屏与黑屏问题；触摸坐标按设备真实分辨率映射，缩放后点击不错位
- 停止时重置状态，避免重启模糊；会话复用前检查流是否已结束
- `max_fps=30` 降低传输开销；CastPanel 布局重设计并适配浅色主题

### 其他
- WSL 发行版选择对话框；本地 Shell 与已安装 shell 同步、WSL 条目拆分
- 本地文件浏览（非远程会话也可用文件面板）
- AI 面板未配置时保持可见，引导配置

---

## 修复
- 投屏会话流结束后正确标记断开并自动重连
- `wsl.exe -l -q` 输出为无 BOM 的 UTF-16LE 时能正确识别
- CLI 传参命令名双后端统一
- cast.rs 代码告警清理（字节串字面量、未使用变量、`child.wait()` 回收等）

---

## 构建 / CI
- 只打包 adb 三个运行时文件，fetch 脚本移入 `scripts/`
- 移除 bin/ 下残留的 .gitkeep 占位文件及其引用
- Tauri 与 Electron 的连接/文件 API 统一（前端 `src/api` 自动检测）
