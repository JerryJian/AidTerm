#!/usr/bin/env node
// Downloads the scrcpy-server jar from the scrcpy GitHub release into bin/ so
// it can be bundled into release packages. Run via `npm run fetch-scrcpy`.
//
// The jar version must match SCRCPY_VERSION in src-tauri/src/cast.rs (the
// server refuses to run against a mismatched version). The win64 zip is used on
// every platform because it always contains the platform-neutral scrcpy-server.
//
// To download through a proxy (Node >= 24.3):
//   $env:HTTPS_PROXY="http://127.0.0.1:7897"; $env:NODE_USE_ENV_PROXY="1"; npm run fetch-scrcpy

import { createWriteStream, existsSync, copyFileSync, mkdirSync, rmSync, readdirSync } from 'node:fs'
import { pipeline } from 'node:stream/promises'
import { execSync } from 'node:child_process'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const SCRCPY_VERSION = process.argv[2] || '4.1'
const binDir = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'bin')
const url = `https://github.com/Genymobile/scrcpy/releases/download/v${SCRCPY_VERSION}/scrcpy-win64-v${SCRCPY_VERSION}.zip`

const stamp = `${Date.now()}-${Math.floor(Math.random() * 1e9)}`
const zipPath = path.join(tmpdir(), `scrcpy-${stamp}.zip`)
const extractDir = path.join(tmpdir(), `scrcpy-${stamp}`)

try {
  console.log(`Downloading ${url}`)
  const res = await fetch(url)
  if (!res.ok || !res.body) throw new Error(`Download failed: HTTP ${res.status}`)
  await pipeline(res.body, createWriteStream(zipPath))

  mkdirSync(extractDir, { recursive: true })
  if (process.platform === 'win32') {
    execSync(`tar -xf "${zipPath}" -C "${extractDir}"`)
  } else {
    execSync(`unzip -q "${zipPath}" -d "${extractDir}"`)
  }

  // The win64 zip nests the jar one directory deep (scrcpy-win64-v4.1/).
  const src = findJar(extractDir)
  if (!src) throw new Error(`scrcpy-server not found in archive: ${extractDir}`)

  mkdirSync(binDir, { recursive: true })
  const dest = path.join(binDir, 'scrcpy-server.jar')
  rmSync(dest, { force: true })
  copyFileSync(src, dest)
  console.log(`scrcpy-server v${SCRCPY_VERSION} installed at ${dest}`)
} finally {
  rmSync(zipPath, { force: true })
  rmSync(extractDir, { recursive: true, force: true })
}

function findJar(root) {
  const stack = [root]
  while (stack.length) {
    const cur = stack.pop()
    let entries
    try {
      entries = readdirSync(cur, { withFileTypes: true })
    } catch {
      continue
    }
    for (const e of entries) {
      const p = path.join(cur, e.name)
      if (e.isDirectory()) stack.push(p)
      else if (e.name === 'scrcpy-server') return p
    }
  }
  return null
}
