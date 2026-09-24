> 跨平台终端模拟器 · Tauri 2.0 + Vue 3 + Rust + Electron 双后端
> 支持平台：Windows / macOS / Linux (x86_64 + arm64)

---

## 修复

- 修复 SSH 会话在 vim 等全屏交互程序（Vim/Nano/Top）下按键无反应 / 界面冻结：重写 SSH 事件循环，写入改由独立 writer 任务处理，exec 任务异步派生，主循环不再阻塞在写入或 exec 上（根因是 russh 背压下的死锁）
- 终端背景跟随主题切换：暗色 `#0c0c0c`、亮色 `#ffffff`（通过 `--term-bg` 变量统一 xterm 主题与容器背景）

---

## 样式 / 主题

- 亮色模式下终端区域背景改为白色，暗色保持 `#0c0c0c`
- 提高 ANSI 红 / 白对比度：红 `#cd3131 → #f14c4c`，白 `#555555 → #e6e6e6`，vim 报错（如 E37）红底白字更醒目

---

## 构建 / CI

- 无新增依赖，仅前端 CSS 变量与 Rust SSH 事件循环改动