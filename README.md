# AidTerm

A cross-platform terminal emulator built with **Tauri 2.x** + **Vue 3 + TypeScript** + **Rust**.

## Features

- Local Shell (cmd, PowerShell, bash, zsh)
- SSH2 (password & key authentication)
- Telnet
- Serial (波特率/数据位/停止位/奇偶校验)
- SFTP file browser (upload, download, delete, rename)
- Multi-tab & split panes (horizontal/vertical)
- xterm.js terminal emulation with full VT100/VT200 support
- Search, infinite scrollback, customizable themes
- Session management (groups, favorites, recently used)
- Quick connect bar (`ssh user@host`)
- AI assistant (DeepSeek / OpenAI / DashScope integration)
- Port forwarding (Local/Remote/Dynamic)
- Proxy support (HTTP/HTTPS/SOCKS5/Jump Host)
- Snippets / Triggers / Broadcast input
- i18n (Chinese/English)
- Global hotkey (Guake mode), system tray, lock screen
- Deep Link (`ssh://` protocol)

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Desktop Framework | Tauri 2.x (Rust) |
| Frontend | Vue 3 + TypeScript + Vite |
| State Management | Pinia |
| Routing | Vue Router |
| Terminal Rendering | xterm.js |
| SSH | ssh2 (libssh2) + ssh-rs |
| Serial | serialport |
| PTY | portable-pty |

## Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) >= 18
- [Rust](https://rustup.rs/)
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

### Install

```bash
npm install
```

### Development

```bash
npm run tauri dev
```

### Build

```bash
npm run tauri build
```

## Project Structure

```
src/                # Vue frontend
├── components/     # terminal, session, sftp, settings
├── stores/         # Pinia stores
├── hooks/          # composables
├── i18n/           # translations
├── router/         # Vue Router
└── types/          # TypeScript types

src-tauri/          # Rust backend
├── src/commands/   # Tauri IPC commands
├── src/ssh/        # SSH client
├── src/serial/     # Serial client
├── src/telnet/     # Telnet client
├── src/local/      # Local shell
├── src/sftp/       # SFTP
├── src/tunnel/     # Port forwarding
└── src/keychain/   # Key management
```

## License

MIT
