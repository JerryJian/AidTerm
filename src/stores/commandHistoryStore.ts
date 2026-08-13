import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { CommandHistoryEntry, TerminalTab } from '../types'

const MAX_HISTORY = 200
const STORAGE_KEY = 'aidterm_command_history_v1'

type StoredHistories = Record<string, CommandHistoryEntry[]>

function loadHistories(): StoredHistories {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return {}
    const parsed = JSON.parse(raw) as StoredHistories
    if (!parsed || typeof parsed !== 'object') return {}
    return Object.fromEntries(
      Object.entries(parsed).map(([key, entries]) => [
        key,
        Array.isArray(entries)
          ? entries.filter(entry => entry && typeof entry.command === 'string').slice(0, MAX_HISTORY)
          : [],
      ])
    )
  } catch {
    return {}
  }
}

function persistHistories(histories: StoredHistories) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(histories))
  } catch {
    // A full or unavailable browser storage must not break terminal input.
  }
}

function normalizeIdentityPart(value: string | number | undefined | null): string {
  return String(value ?? '').trim().toLowerCase() || 'default'
}

/** Stable, non-secret identity for a terminal connection. */
export function commandHistoryConnectionKey(tab: TerminalTab): string {
  const session = tab.session
  const type = session?.type ?? 'local'
  switch (type) {
    case 'local':
      return `local:${normalizeIdentityPart(session?.command)}`
    case 'wsl':
      return `wsl:${normalizeIdentityPart(tab.wslInfo?.distro)}`
    case 'ssh':
      return `ssh:${normalizeIdentityPart(tab.sshInfo?.username)}@${normalizeIdentityPart(tab.sshInfo?.host)}:${normalizeIdentityPart(tab.sshInfo?.port)}`
    case 'telnet':
      return `telnet:${normalizeIdentityPart(tab.telnetInfo?.host)}:${normalizeIdentityPart(tab.telnetInfo?.port)}`
    case 'serial':
      return `serial:${normalizeIdentityPart(tab.serialInfo?.portName)}`
    case 'adb':
      return `adb:${normalizeIdentityPart(tab.adbInfo?.serial)}`
    default:
      return `${type}:${normalizeIdentityPart(session?.title)}`
  }
}

/**
 * Per-pane input tracking for command detection.
 *
 * Commands are recovered from the user's keystrokes (xterm onData) instead of
 * the output stream, so startup banners, progress bars and echoed output can
 * never leak into the history. A command is recorded only when the user
 * submits the input line with Enter (\r / \n).
 *
 * Two shell behaviours rewrite the line with text the user never typed, so the
 * mini line editor is not authoritative on its own:
 *
 * - **Tab completion**: the completed word is echoed by the shell, invisible
 *   to onData. When Enter is pressed the caller therefore resolves the actual
 *   displayed line and prefers it over the editor text when it is longer.
 * - **History recall (↑/↓/Ctrl+R)**: the recalled line is drawn by the shell.
 *   The prompt prefix is snapshotted at recall time so the caller can strip it
 *   from the displayed line on Enter.
 *
 * The caller passes `getPrompt`, which reads the prompt text currently under
 * the cursor from the terminal buffer. It is snapshotted once per input line.
 */
interface InputBuffer {
  text: string
  cursor: number
  prompt: string
  promptSet: boolean
  recalled: boolean
}

function newBuffer(): InputBuffer {
  return { text: '', cursor: 0, prompt: '', promptSet: false, recalled: false }
}

let nextEntryId = 1
function genEntryId(): string {
  return `cmd-${Date.now()}-${nextEntryId++}`
}

/** Consume a terminal escape sequence starting at data[i] (data[i] === '\x1b'). */
function consumeEscape(data: string, i: number): number {
  const next = data[i + 1]
  if (next === '[') {
    for (let j = i + 2; j < data.length; j++) {
      const c = data.charCodeAt(j)
      if (c >= 0x40 && c <= 0x7e) return j - i + 1
    }
    return data.length - i
  }
  if (next === ']') {
    for (let j = i + 2; j < data.length; j++) {
      if (data[j] === '\x07') return j - i + 1
      if (data[j] === '\x1b' && data[j + 1] === '\\') return j - i + 2
    }
    return data.length - i
  }
  if (next === 'O') return i + 2 < data.length ? 3 : 2
  return next !== undefined ? 2 : 1
}

/** A history-recalling key (↑/↓/Ctrl+R) with an empty input line: snapshot the prompt. */
function onRecall(buf: InputBuffer, getPrompt: () => string) {
  if (buf.text !== '' || buf.cursor !== 0) return
  if (!buf.promptSet) {
    buf.prompt = getPrompt()
    buf.promptSet = true
  }
  buf.recalled = true
}

export const useCommandHistoryStore = defineStore('commandHistory', () => {
  const histories = ref<StoredHistories>(loadHistories())
  const buffers = new Map<string, InputBuffer>()
  const paneKeys = new Map<string, string>()

  function keyForPane(tabId: string): string {
    return paneKeys.get(tabId) ?? tabId
  }

  function bindPane(tabId: string, tab: TerminalTab) {
    const key = commandHistoryConnectionKey(tab)
    const legacyEntries = histories.value[tabId]
    if (tabId !== key && legacyEntries?.length && !histories.value[key]?.length) {
      histories.value[key] = legacyEntries
      delete histories.value[tabId]
      persistHistories(histories.value)
    }
    paneKeys.set(tabId, key)
  }

  function addEntry(tabId: string, command: string) {
    const key = keyForPane(tabId)
    const list = (histories.value[key] ?? []).filter(e => e.command !== command)
    list.unshift({ id: genEntryId(), command, timestamp: Date.now() })
    if (list.length > MAX_HISTORY) list.length = MAX_HISTORY
    histories.value[key] = list
    persistHistories(histories.value)
  }

  function recordCommand(tabId: string, command: string) {
    addEntry(tabId, command)
  }

  /**
   * Track user keystrokes for a pane. Returns the commands submitted with
   * Enter / newline in this chunk and whether Enter closed a recalled line
   * (editor buffer empty, shell-redrawn text to resolve by the caller).
   */
  function feedInput(
    tabId: string,
    data: string,
    getPrompt: () => string
  ): { commands: string[]; recalled: boolean } {
    let buf = buffers.get(tabId)
    if (!buf) {
      buf = newBuffer()
      buffers.set(tabId, buf)
    }
    const commands: string[] = []
    let recalledCommit = false
    for (let i = 0; i < data.length; i++) {
      const ch = data[i]
      if (ch === '\x1b') {
        const seq = data.slice(i, i + consumeEscape(data, i))
        if (seq === '\x1b[D') buf.cursor = Math.max(0, buf.cursor - 1)
        else if (seq === '\x1b[C') buf.cursor = Math.min(buf.text.length, buf.cursor + 1)
        else if (seq === '\x1b[H' || seq === '\x1b[1~') buf.cursor = 0
        else if (seq === '\x1b[F' || seq === '\x1b[4~') buf.cursor = buf.text.length
        else if (
          seq === '\x1b[A' ||
          seq === '\x1bOA' ||
          seq === '\x1b[B' ||
          seq === '\x1bOB'
        )
          onRecall(buf, getPrompt)
        i += seq.length - 1
        continue
      }
      if (ch === '\r' || ch === '\n') {
        const cmd = buf.text.trim()
        if (cmd) commands.push(cmd)
        else if (buf.recalled) recalledCommit = true
        buf.text = ''
        buf.cursor = 0
        buf.recalled = false
        buf.promptSet = false
        continue
      }
      if (ch === '\x7f' || ch === '\x08') {
        // Backspace
        if (buf.cursor > 0) {
          buf.text = buf.text.slice(0, buf.cursor - 1) + buf.text.slice(buf.cursor)
          buf.cursor--
        }
        continue
      }
      if (ch === '\x03' || ch === '\x1a') {
        // Ctrl+C / Ctrl+Z: abort the current input line
        buf.text = ''
        buf.cursor = 0
        buf.recalled = false
        buf.promptSet = false
        continue
      }
      if (ch === '\x15') {
        // Ctrl+U: kill text before cursor
        buf.text = buf.text.slice(buf.cursor)
        buf.cursor = 0
        continue
      }
      if (ch === '\x17') {
        // Ctrl+W: kill word before cursor
        const before = buf.text.slice(0, buf.cursor)
        const after = buf.text.slice(buf.cursor)
        const cut = before.replace(/\s+$/, '').lastIndexOf(' ')
        const keep = cut === -1 ? '' : before.slice(0, cut + 1)
        buf.text = keep + after
        buf.cursor = keep.length
        continue
      }
      if (ch === '\x12') {
        // Ctrl+R: bash reverse-i-search redraws a recalled line.
        onRecall(buf, getPrompt)
        continue
      }
      if (ch === '\t') {
        // Tab completion rewrites the line via the shell; the completed text is
        // not visible to onData, so the editor keeps the pre-completion text and
        // the caller prefers the displayed line at commit.
        continue
      }
      const code = ch.charCodeAt(0)
      if (code >= 0x20) {
        if (buf.text === '' && buf.cursor === 0) {
          buf.prompt = getPrompt()
          buf.promptSet = true
          buf.recalled = false
        }
        buf.text = buf.text.slice(0, buf.cursor) + ch + buf.text.slice(buf.cursor)
        buf.cursor++
      }
    }
    buffers.set(tabId, buf)
    return { commands, recalled: recalledCommit }
  }

  /** The prompt prefix snapshotted for the current input line ('' when none). */
  function promptFor(tabId: string): string {
    return buffers.get(tabId)?.prompt ?? ''
  }

  function historyFor(tabId: string): CommandHistoryEntry[] {
    return histories.value[keyForPane(tabId)] ?? []
  }

  function removeEntry(tabId: string, entryId: string) {
    const key = keyForPane(tabId)
    const list = histories.value[key]
    if (!list) return
    histories.value[key] = list.filter(e => e.id !== entryId)
    persistHistories(histories.value)
  }

  function clear(tabId: string) {
    histories.value[keyForPane(tabId)] = []
    persistHistories(histories.value)
  }

  function resetInput(tabId: string) {
    buffers.delete(tabId)
  }

  function disposeTab(tabId: string) {
    paneKeys.delete(tabId)
    buffers.delete(tabId)
  }

  return {
    histories,
    bindPane,
    feedInput,
    recordCommand,
    promptFor,
    resetInput,
    historyFor,
    removeEntry,
    clear,
    disposeTab,
  }
})
