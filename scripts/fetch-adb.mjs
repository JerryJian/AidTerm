#!/usr/bin/env node
// Downloads the adb binary from Google's platform-tools into bin/ so it can be
// bundled into release packages. Run via `npm run fetch-adb`.
//
//   - Windows  -> platform-tools-latest-windows.zip  (adb.exe + AdbWin*.dll)
//   - macOS    -> platform-tools-latest-darwin.zip   (adb)
//   - Linux    -> platform-tools-latest-linux.zip    (adb)
//
// adb.exe imports AdbWinApi.dll (which in turn uses AdbWinUsbApi.dll) at
// runtime, so on Windows those DLLs are kept next to the binary and must be
// bundled alongside it. Everything else in platform-tools is discarded.

import { createWriteStream, existsSync, copyFileSync, mkdirSync, rmSync, readdirSync } from 'node:fs'
import { pipeline } from 'node:stream/promises'
import { execSync } from 'node:child_process'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const binDir = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'bin')
const platform = process.platform
const exeName = platform === 'win32' ? 'adb.exe' : 'adb'

const urls = {
  win32: 'https://dl.google.com/android/repository/platform-tools-latest-windows.zip',
  darwin: 'https://dl.google.com/android/repository/platform-tools-latest-darwin.zip',
  linux: 'https://dl.google.com/android/repository/platform-tools-latest-linux.zip',
}

const url = urls[platform]
if (!url) {
  console.error(`Unsupported platform: ${platform}`)
  process.exit(1)
}

const stamp = `${Date.now()}-${Math.floor(Math.random() * 1e9)}`
const zipPath = path.join(tmpdir(), `platform-tools-${stamp}.zip`)
const extractDir = path.join(tmpdir(), `platform-tools-${stamp}`)

try {
  console.log(`Downloading ${url}`)
  const res = await fetch(url)
  if (!res.ok || !res.body) throw new Error(`Download failed: HTTP ${res.status}`)
  await pipeline(res.body, createWriteStream(zipPath))

  mkdirSync(extractDir, { recursive: true })
  if (platform === 'win32') {
    execSync(`tar -xf "${zipPath}" -C "${extractDir}"`)
  } else {
    execSync(`unzip -q "${zipPath}" -d "${extractDir}"`)
  }

  const src = path.join(extractDir, 'platform-tools', exeName)
  if (!existsSync(src)) throw new Error(`adb not found in archive: ${src}`)

  mkdirSync(binDir, { recursive: true })
  const dest = path.join(binDir, exeName)
  rmSync(dest, { force: true })
  copyFileSync(src, dest)
  if (platform !== 'win32') {
    execSync(`chmod +x "${dest}"`)
  }
  console.log(`adb installed at ${dest}`)

  // adb.exe needs AdbWinApi.dll / AdbWinUsbApi.dll next to it at runtime.
  if (platform === 'win32') {
    for (const name of ['AdbWinApi.dll', 'AdbWinUsbApi.dll']) {
      const d = path.join(binDir, name)
      rmSync(d, { force: true })
      copyFileSync(path.join(extractDir, 'platform-tools', name), d)
      console.log(`adb dependency installed at ${d}`)
    }
  }
} finally {
  rmSync(zipPath, { force: true })
  rmSync(extractDir, { recursive: true, force: true })
}
