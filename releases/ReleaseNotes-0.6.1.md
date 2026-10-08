> 跨平台终端模拟器 · Tauri 2.0 + Vue 3 + Rust + Electron 双后端
> 支持平台：Windows / macOS / Linux (x86_64 + arm64)

---

## 修复

- 修复发布版中 vim / htop 等全屏程序按键无反应、界面假死：vite 6 的默认 `build.target`（含 `firefox78`）迫使 esbuild 0.25.12 降级编译，而该版本会错误地丢弃 `let` 声明、只保留 `||=` 的赋值，导致产物在 xterm.js 的 `requestMode`（DECRQM 模式查询）里抛出 `ReferenceError`，异常冲出 `Terminal._innerWrite` 使渲染循环中断。开发模式不受影响，故此前只在 Release 包中复现
- 修复 `npm run tauri dev` 启动时报 Tauri 包版本不匹配（npm 包与 Rust crate 的 major/minor 必须一致）

---

## 依赖 / 工具链

- 构建器 vite 6 → 8，改用 rolldown / oxc，esbuild 已完全移出依赖树
- @vitejs/plugin-vue 5 → 6，TypeScript 5.6 → 6.0，vue-tsc 2 → 3
- ESLint 10.12 / typescript-eslint 8.71 / eslint-plugin-vue 10.11 / Prettier 3.9.9
- pinia 3 → 4，vue-router 4 → 5（对本项目用法无破坏性变更）
- vue 3.5.43、vue-i18n 11.4.13、marked 18.1.0
- Tauri Rust crate 与 npm 包对齐：tauri 2.12.1、plugin-dialog 2.8.1、plugin-opener 2.7.0、plugin-deep-link 2.6.1、@tauri-apps/cli 2.12.1

---

## 构建 / CI

- vite 配置移除 `build.target` 兼容 hack，`__dirname` 改用 `import.meta.dirname`
- 验证：`npm run lint`、`vue-tsc --noEmit`、`vite build`、`cargo check` 均通过；DECRQM 回归测试在 vite 8 产物上通过、在 esbuild 0.25.12 对照组上复现原始 `ReferenceError`
