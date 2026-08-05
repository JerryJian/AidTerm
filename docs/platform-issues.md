# 跨平台问题清单（Windows 正常 / Linux / macOS 可能出错）

> 审查范围：Tauri Rust 后端（`src-tauri/src`）、Electron 后端（`src-electron`）、Vue 前端（`src`）。
> 结论：以下为"在 Windows 上运行正常，但在 Linux / macOS 上会出问题"（或行为与 Windows 不一致）的地方，按严重度排序。

---

## 高优先级（Linux/macOS 上真实出错）

| # | 问题 | 位置 | 说明 |
|---|------|------|------|
| 5 | **导入私钥 0644 世界可读** | `src-electron/main.ts:1043`（`key_import` 拷贝后无 chmod）；`src-tauri/src/crypto.rs:27`（`.store_key`） | Linux/macOS 上按默认 umask 落盘，通常 0644 world-readable。私钥被其他本地用户可读，且后续交给 git/ssh-agent 会触发 "UNPROTECTED PRIVATE KEY FILE" 拒绝。应 `fs.chmod(priv, 0o600)` / `OpenOptionsExt::mode(0o600)`。 |
| 6 | **打包后 CLI 参数错位** | `src-electron/main.ts:943` `process.argv.slice(2)` | 打包版 argv 第一个元素是应用可执行文件路径，`slice(2)` 会把首参吞掉、其余参数整体前移。应 `app.isPackaged ? process.argv.slice(1) : process.argv.slice(2)`（macOS 还要过滤 `-psn_0_xxx`）。 |
| 7 | **Local 隧道依赖远程 `nc`** | `src-electron/main.ts:724` | Local 端口转发在远程服务器执行 `nc host port`。Linux 精简发行版/容器未装 netcat、macOS 新版 `nc` 被移除（或只在 Xcode 中）→ Local 隧道失效且无降级。 |
| 8 | **密钥生成/导入依赖外部 `ssh-keygen`** | `src-tauri/src/keychain/mod.rs:52-61` | Windows 10+ 自带 OpenSSH client；Linux 最小安装（无 `openssh-client` 包）、旧 macOS 无 `ssh-keygen` → 密钥生成/导入直接报错。建议改用 `russh::keys` 自实现。 |

---

## 中优先级

| # | 问题 | 位置 | 说明 |
|---|------|------|------|
| 10 | **命令发送换行符不一致** | `src/components/terminal/TabBar.vue:208`、`src/components/snippet/SnippetPanel.vue:77,84`、`src/hooks/useTriggerWatcher.ts:22` 发 `\n`；`src/hooks/useAiConversation.ts:350` 发 `\r` | 批量/快捷命令/触发器用 `\n`，AI 执行用 `\r`。Unix PTY 靠 ICRNL 都行；Windows cmd/多行粘贴/串口设备上总有一侧行为不对。应统一为真实回车语义（`\r` 或按平台 `\r\n`）。 |
| 11 | **注入消息全平台用 `\r\n`，Unix 触发正则失配** | `src-tauri/src/session/local.rs:121`、`ssh.rs:162,195`、`telnet.rs:29,58`、`serial/mod.rs:83,94,129` | `[Process exited]`、SSH/Telnet/Serial 错误横幅在所有平台注入 `\r\n`，而 Unix shell 原生输出只用 `\n`。前端 trigger 匹配（`src/stores/triggerStore.ts:50-64`）不做换行归一化 → 锚定正则（如 `/\[Process exited\]$/`）在 Unix 上失配。 |
| 12 | **Tab 标题平台错误** | `src/stores/terminal.ts:12-22,104,107` + `src/App.vue:124` | shell 映射表是 Windows 化的（`cmd.exe`/`.exe` 归一化，无 `/bin/bash` 键）；新建本地 Tab 不传命令 → 所有平台默认标题「命令提示符」；Linux/macOS 上手动输入 `/bin/bash` 标题会显示原始 key `shell./bin/bash`。 |
| 13 | **密钥命令用 `JSON.stringify` 当 shell 引用** | `src-electron/main.ts:982,983,1008,1011,1048` | `-N ${JSON.stringify(passphrase)}`、`-f`、`ssh-keygen -y -f "${destPriv}"` 等用 shell 字符串拼接。路径/口令含 `$`、反引号、空格、`\` 时，Linux/macOS 上被 sh 展开破坏；Windows 上 `\\` 双反斜杠也错。应改用 `spawn` 数组参数。 |
| 14 | **known_hosts 三处缺陷** | `src-tauri/src/known_hosts/mod.rs:53-54,78-80,85-99`；`src-electron/main.ts:248` | ① 指纹切片 `key[key.len()-8..]` 在 key<8 时 debug 下溢 panic；② `add()` 不创建 `~/.ssh` 目录（新装/未用过 ssh 的 Linux 机器常见）；③ `remove()` 整文件重写会丢失 hash host（`\|1\|...\|`）、`@cert-authority`、`@revoked`、注释行并改顺序；host 比较大小写敏感，与系统 ssh 共享文件时删不掉 `GitHub.com` 条目。 |
| 15 | **detect_shells Linux/macOS 分支不做存在性检查** | `src-tauri/src/commands/mod.rs:664-669,624-633` | Windows 分支对 `pwsh.exe`/`wsl.exe` 做了 `exe_in_path` 检查；Linux/macOS 分支对 `bash`/`sh` 无条件推送，且 `exe_in_path` 在 Unix 上只判断存在、不检查可执行位（`mode & 0o111`）。 |
| 16 | **强制 `LANG=en_US.UTF-8`** | `src-tauri/src/session/local.rs:69-71`；`src-electron/main.ts:355` | macOS GUI 启动通常无 `LANG`/`LC_ALL`，会命中兜底把用户 shell 强制切成英文 locale（提示符/程序输出变英文）；精简 Linux 可能未生成 `en_US.UTF-8` locale 而产生告警。 |
| 17 | **AI 危险命令启发式仅 Unix** | `src/hooks/useAiConversation.ts:195-211`；`src/stores/aiStore.ts:164-173` | `rm/mv/chmod/apt/brew/systemctl` 等有，Windows 的 `del/rd/taskkill/format/ipconfig` 无。Linux/macOS 上无碍；Windows 上 `del /s /q` 会被判定安全而自动执行。 |

---

## 低优先级

| # | 问题 | 位置 | 说明 |
|---|------|------|------|
| 18 | **IPv6 解析缺失** | `src/App.vue:332-357`、`src/components/session/QuickConnectBar.vue:52`、`src-tauri/src/session/ssh.rs:133,316`、`sftp.rs:152`、`tunnel/mod.rs:157,320,373,398`、`proxy/mod.rs:105,141,158` | 各处用 `split(':')` / `format!("{}:{}", host, port)`，纯 IPv6 字面量（含多个 `:`）全部解析错误。IPv6 在 Linux/macOS 使用更普遍。 |
| 19 | **Linux 远程 `uname -a` arch 解析错误** | `src-electron/main.ts:936` | 取 `parts[parts.length-2]`，Linux 远程末尾是 `... x86_64 GNU/Linux`，取到 "GNU/Linux" 而非架构；仅 macOS 远程末尾刚好是架构。 |
| 20 | **会话组名硬编码中文** | `src/stores/sessionStore.ts:134` | 内置「本地终端」分组对所有平台/语言都显示中文，应走 i18n。 |
| 21 | **macOS Cmd+Shift+I 不触发 DevTools** | `src-electron/main.ts:110-115` | 快捷键只认 `input.control`，macOS 用户习惯 `Cmd`（`input.meta`）不生效。 |
| 22 | **`LANG` 兜底与 AI 命令引用（反向缺口）** | `src-tauri/src/ai/mod.rs:387-405`；`src-electron/main.ts:805-808` | Windows 上 `cmd /C` 输出 GBK 时 `from_utf8_lossy` 乱码（缺 GBK 兜底）；Electron 侧 `sh -c ${JSON.stringify(cmd)}` 不转义 `$`/反引号导致语义偏差。 |

---

## 已修复（本轮）

- **关闭 Tab 子进程残留（孤儿）**：`src-tauri/src/session/local.rs`（新增 `pid` 字段，`kill()` Unix 下对进程组先 `SIGHUP`、300ms 后兜底 `SIGKILL`，并直接信号前台进程组）+ `src-electron/main.ts`（新增 `killPty()`，Unix 下 `process.kill(-pid, SIGHUP)` → 500ms 后 `SIGKILL`，Windows 仍走 `term.kill()` 整树）——`vim`/`top`/后台 `&` 任务不再残留。
- **hostname 恒为 "unknown"**：`src-tauri/src/commands/mod.rs`——新增 `get_hostname()`：优先 `COMPUTERNAME`/`HOSTNAME` 环境变量，缺失时回退 `hostname` 命令、再回退 `uname -n`，Linux/macOS GUI 启动不再返回 "unknown"。
- **Linux 窗口控制按钮**：`src/components/titlebar/TitleBar.vue`——新增 `isLinux` 分支，Linux 上与 Windows 一致显示最小化/最大化/关闭按钮，右键菜单不再被 `v-if="isWindows"` 隐藏；拖拽/最大化还原由 `3fb8606` 处理。
- **背景图加载**：`src/App.vue` + `src/api/{index,tauri,electron}.ts` + `src-electron/main.ts`（`file_to_data_url`）+ `src-tauri/tauri.conf.json`（`assetProtocol`）——弃用裸 `file:///`，Tauri 用 `convertFileSrc`、Electron 用 data URL。
- **终端透出背景**：`src/components/terminal/TerminalWrapper.vue`——背景图开启时终端容器与 xterm 主题背景改为透明。
- **detect_shells 返回形状统一**：`src-electron/main.ts`（`detect_shells` 改为与 Tauri 一致的 `{name,command,icon}` 对象数组）+ `src/App.vue`（`initBuiltInLocalProfiles` 归一化两种形状）——Electron 模式下内置本地会话 profile 字段不再 undefined。

## 修复建议优先级

1. 私钥权限（#5）：`chmod 0600`。
2. `ssh-keygen` 依赖（#8）：改用 `russh::keys`。
3. 换行符统一（#10、#11）。
4. known_hosts（#14）三处 bug。
