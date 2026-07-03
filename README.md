# TndTerm

A cross-platform terminal emulator built with Tauri 2.x + Vue 3 (TypeScript).

## Development

```bash
npm run tauri dev
```

## Proxy / Mirror 配置（公司网络恢复用）

```ini
# ~\.cargo\config.toml
[http]
proxy = "http://192.168.8.200:7897"
```

```ini
# ~\.npmrc
proxy=http://192.168.8.200:7897
https-proxy=http://192.168.8.200:7897
registry=https://registry.npmmirror.com
```
