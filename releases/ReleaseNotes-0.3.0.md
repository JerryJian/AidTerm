> 跨平台终端模拟器 · Tauri 2.0 + Vue 3 + Rust + Electron 双后端
> 支持平台：Windows / macOS / Linux (x86_64 + arm64)

---

## 新功能

### 检查更新
- About 对话框：打开时自动检查 GitHub Releases 最新版，支持手动重新检查
- 显示最新版本号、发布时间、Release Notes 与安装包类型
- 发现新版本后一键下载安装包，实时进度条显示下载进度
- Windows 按注册表卸载项自动检测当前安装类型（MSI / EXE·NSIS），下载与当前安装一致的安装包
- 调起安装：Windows MSI 用 `msiexec /i ... /qn`、NSIS 用 `/S` 静默安装后自动重启；macOS `open` dmg；Linux AppImage 直接运行 / deb 用 xdg-open

---

## 修复
- AI 取消：中止进行中的请求并丢弃过期响应，避免取消后旧输出继续回流
- AI 助手回复气泡改为满宽显示，不再靠左挤压
- Windows ConPTY 输出改为流式解码，UTF-8 多字节字符跨读缓冲边界时不再乱码
- 检查更新结束的提示文字不再导致 About 对话框高度跳动

---

## 构建 / CI
- Electron Linux 以 glibc 2.31 基线构建，兼容旧发行版
- Release 资产命名统一约定：Tauri AppImage 使用 amd64、dmg 归入 dmg/ 目录；Electron mac dmg 与 deb amd64→x64 命名修正
- Release body 直接使用 `releases/ReleaseNotes-<版本>.md` 内容
- 创建新 Release 前删除同名过期 draft，避免冲突
- Electron Linux 构建改用 ubuntu-22.04 runner（20.04 镜像已退役）
