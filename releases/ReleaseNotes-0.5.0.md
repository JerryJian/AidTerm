> 跨平台终端模拟器 · Tauri 2.0 + Vue 3 + Rust + Electron 双后端
> 支持平台：Windows / macOS / Linux (x86_64 + arm64)

---

## 新功能

### 系统监控（Monitor）
- 新增右侧工具面板「监控」Tab，本地 / WSL / 远程（SSH）会话均可打开
- 本地与 WSL 会话采集系统资源（`get_system_metrics` 按 session 类型分发，WSL 经 `wsl.exe -e` 读取 Linux 数据）
- 远程（SSH）会话批量 exec 采集 `/proc/stat`、`meminfo`、`loadavg`、`df`、`/proc/net/dev`，双后端统一 `get_remote_system_metrics`
- 进度门控：仅当会话具备 exec 能力时才显示监控
- ECharts 可视化：CPU 半圆仪表图 + 折线趋势、内存环形饼图、GPU 占用率仪表图 + 显存渐变条、磁盘进度条、网络分接口 rx/tx 双折线
- GPU 自动探测 nvidia-smi / rocm-smi / intel_gpu_top，探测不到自动隐藏
- 仅面板显示时轮询（每 2s），窗口隐藏/切换标签页停止请求；按 session 缓存数据，切换回来不丢失

### 系统集成（Windows）
- 右键菜单集成：在 Windows 资源管理器右键打开 AidTerm 并定位到当前目录（`--cwd`），可一键开关
- PATH 集成：可选将 AidTerm 加入系统 PATH，支持命令行直接启动，可一键开关
- 两者均支持读写检测当前开关状态（注册表），非 Windows 平台自动隐藏

### 单例模式
- 默认关闭，多个实例互不影响
- 开启后重复启动聚焦已有窗口并转发 `--cwd`，仅通过单实例同时满足单开与目标目录定位
- 右键菜单打开时直接定位目录，不再创建重复标签页

### 命令历史
- 新增「命令历史」面板，按连接（session）分别持久化命令记录
- 切换会话自动切换到对应历史，删除单条或清空历史

---

## 修复
- 窗口最大化后鼠标移至屏幕边缘仍显示缩放光标但无法调整：缩放手柄在最大化时隐藏，且仅在 Linux（无边框缺原生缩放）显示，Windows/macOS 交还系统原生边缘缩放
- 切换标签页时监控数据被清空：数据按 session 缓存，切回自动恢复
- WSL 高级菜单在 macOS/Linux 不显示
- 工作目录处理统一：右键菜单 `--cwd` 打开、双击启动定位用户主目录、无显式目录的新标签页继承进程工作目录；工作目录不存在时报错并提示
- WSL 会话启动改用 `--cd` 指定工作目录，正确处理发行版与目录参数

---

## 构建 / CI
- 引入 ECharts（按需注册），新增通用 ECChart 容器组件
- Rust 新增系统信息采集依赖（sysinfo）与 WSL 内部采集支持
