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

---

## 中优先级

| # | 问题 | 位置 | 说明 |
|---|------|------|------|
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
- **密钥生成/导入不再依赖外部 `ssh-keygen`**：`src-tauri/src/keychain/mod.rs`——用 `russh::keys`（ssh-key）`PrivateKey::random` 原生生成 ED25519 / RSA(4096)，`encrypt` 支持 passphrase 加密私钥，`write_openssh_file` 自动 0600 权限；导入时用 `load_secret_key` 直接解析私钥并提取公钥/指纹，不再调用 `ssh-keygen`。
- **命令发送换行符统一为真实回车 `\r`（#10）**：`src/components/terminal/TabBar.vue`（批量输入）、`src/components/snippet/SnippetPanel.vue`（快捷命令直接发送 + 变量替换后发送）、`src/hooks/useTriggerWatcher.ts`（触发器响应）——原发 `\n`，现与 `src/hooks/useAiConversation.ts` 一致发 `\r`。PTY 中 Enter 键即产生 `\r`，raw 模式（`vim`/`top`）下 `\n` 会被当作 Ctrl-J 而非执行键，统一后多行粘贴/串口/Windows cmd 行为一致。
- **触发器匹配换行归一化（#11）**：`src/stores/triggerStore.ts` `findMatch`——匹配前将 `\r\n` 与孤立 `\r` 归一化为 `\n`。后端注入横幅（`[Process exited]`、SSH/Telnet/Serial 错误）全平台用 `\r\n`，而 Unix PTY 原生输出只有 `\n`；归一化后锚定正则（如 `/\[Process exited\]$/`）在 Unix 上不再失配，Windows 也不受影响。
- **Tab 标题平台错误（#12）**：`src/stores/terminal.ts`——① `shellKeyMap` 补充 Unix 绝对路径键（`/bin/bash`、`/usr/bin/bash`、`/bin/zsh`、`/usr/bin/zsh`、`/bin/sh`、`/usr/bin/sh`、`/usr/bin/fish`、`/usr/bin/pwsh`）；② 命令未命中映射表时取 basename（含 `.exe` 归一化）再查表，未知 shell 用 `te` 检测回退显示 basename，不再出现原始 key `shell./bin/bash`；③ 新建本地 Tab 不传命令时默认标题从 `shell.cmd`（「命令提示符」，仅 Windows 正确）改为平台中立 `menu.local_shell`（「本地终端」/「Local Shell」）。
- **密钥命令 shell 引用问题（#13）**：`src-electron/main.ts`——`runCmd` 从 `execSync` 字符串拼接改为 `spawnSync(cmd, args)`（`shell: false`，数组参数原样传递）；`key_generate_rsa`/`key_generate_ed25519` 的 `-N ${JSON.stringify(passphrase)}` 改为独立的 `'-N', passphrase || ''` 参数，路径不再包 `JSON.stringify`；`key_import` 的公钥提取从 `execSync("ssh-keygen -y -f \"${destPriv}\"")` 改为 `runCmd('ssh-keygen', ['-y', '-f', destPriv])`。路径/口令含 `$`、反引号、空格、`\`、引号时不再被 shell 展开或破坏（`ai_execute` 的 `sh -c ${JSON.stringify(cmd)}` 属 #22 另计）。

## 复测指南（#5–#8、#10–#13）

以下为各修复的复现条件与验证方法（Linux/macOS 优先）。

- **#5 导入私钥 0644**：复现条件——在 umask 为 `022` 的 Linux/macOS 上，Tauri 侧「设置→密钥管理→生成」私钥后 `ls -l` 查看 `appData/keys/*id_rsa`（修复前 0644，修复后 0600）；Electron 侧「导入私钥」到任意位置后同样应为 0600。验证：`ssh-keygen -y -f <key> -P ""` 不再报 "UNPROTECTED PRIVATE KEY FILE"，且可用该密钥完成一次 SSH 登录。
- **#6 打包后 CLI 参数错位**：复现条件——用 `npm run build` 打包后的可执行文件（`npx electron .` 开发态不受影响）在终端带参启动，如 `AidTerm.exe ssh user@host`（macOS 为 `open -a AidTerm --args ...`）。修复前首参被吞、后续参数前移导致连接对象错乱；修复后应解析为 `ssh user@host`，且 macOS 的 `-psn_0_xxx` 被过滤。
- **#7 Local 隧道依赖远程 `nc`**：复现条件——连接一台未安装 `nc`（`command -v nc` 为空）的 Linux 服务器，添加 Local 端口转发。修复前日志报 `nc: command not found`、隧道失效；修复后改用 `forwardOut`（direct-tcpip）直连目标端口，`nc` 缺失时隧道仍可用。验证：`ssh -T` 场景下本地 `curl 127.0.0.1:<local_port>` 能访问远程目标。
- **#8 依赖外部 `ssh-keygen`**：复现条件——在未安装 `openssh-client`（Linux 最小安装）或 PATH 中无 `ssh-keygen` 的环境，Tauri 侧「密钥管理→生成 RSA/ED25519」与「导入私钥」。修复前报 `ssh-keygen not found`；修复后不依赖外部命令。验证：生成 RSA 后 `ls -l` 为 0600、`.pub` 内容以 `ssh-rsa AAAA` 开头、指纹显示 `SHA256:...`；设置 passphrase 时私钥文件开头为 `-----BEGIN OPENSSH PRIVATE KEY-----`（加密负载，可用 `openssl` 或 `ssh-keygen -y -f <key>` 输入口令验证加密是否生效）；导入无 `.pub` 的私钥也能自动提取公钥与指纹。注：当前 SSH 连接 `src-tauri/src/session/ssh.rs:219` 仍以 `None` 口令加载密钥，带 passphrase 的密钥暂不能用于登录（需后续接入口令输入，属已知缺口）。
- **#10 命令发送换行符**：复现条件——Unix 上在 `vim`（raw 模式）或 Windows cmd 中批量发送 / 快捷命令 / 触发器响应。修复前 `vim` 收到 `\n` 触发的是 Ctrl-J（光标下移）而非回车执行，cmd 多行输入错乱；修复后发 `\r` 行为等同真实 Enter。验证：`vim` 中批量发送 `:q` 能正常退出；任意平台批量发送 `echo ok` 后命令立即执行且提示符换行正确。
- **#11 触发器锚定正则**：复现条件——任何平台新建触发器，pattern 填 `^\[Process exited\]$`，response 填任意内容，然后关闭一个本地终端 Tab（或连接一个无法连通的 SSH/Serial 目标）。修复前 Unix 上匹配不到（注入横幅是 `\r\n` 结尾而 shell 原生输出是 `\n`）；修复后触发器触发（`\r\n` 与孤立 `\r` 已在匹配前归一化为 `\n`）。Windows 上行为不变。
- **#12 Tab 标题平台错误**：复现条件——① Linux/macOS 上点击「新建本地终端」或首次启动自动建 Tab（不传命令），修复前标题为「命令提示符」，修复后为「本地终端」/「Local Shell」；② 会话管理中新建本地会话、命令填 `/bin/bash`（或任意绝对路径 shell）后打开，修复前标题显示原始 key `shell./bin/bash`，修复后显示「Bash」；③ 命令填不常见路径（如 `/usr/local/bin/tcsh`）时显示 basename `tcsh` 而非原始 key。
- **#13 密钥命令 shell 引用**：复现条件——Electron 后端（`src-electron/` 下 `npm run dev`），密钥管理里生成/导入时口令或名称含特殊字符（如口令 `pa$s "wd\`、名称 `my key`）。修复前 `-N ${JSON.stringify(...)}` 与 `"${path}"` 字符串拼接在 Linux/macOS 被 sh 展开、Windows 反斜杠错乱 → 生成失败或口令错误；修复后 `spawnSync` 数组参数原样传递。验证：口令含 `$`/空格/反引号/双引号时能成功生成并用该口令解出私钥（`ssh-keygen -y -f <key>` 输入口令成功）；路径含空格时导入/生成不报错。

## 修复建议优先级

1. known_hosts（#14）三处 bug。
