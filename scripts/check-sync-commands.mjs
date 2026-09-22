#!/usr/bin/env node
/**
 * Guard for ARCHITECTURE.md §11: no command may run on the main thread unless
 * it is on the list below, with a reason.
 *
 * Tauri executes a `#[tauri::command]` **on the main thread** unless the
 * function is `async` or the attribute says `#[tauri::command(async)]`. The
 * main thread is also the event loop: it pumps every window, the tray icon and
 * the global-hotkey target. A command that blocks there — a `git status`, a
 * native file picker, a contended `Mutex<Connection>` — freezes all of them at
 * once, and Windows files it as `AppHangB1` with no stack.
 *
 * That is not hypothetical. The suite hung repeatedly in release builds; the
 * watchdog in `crates/qs-core/src/diagnostics.rs` caught the stalls but nothing
 * stopped the next blocking command from being written. At the time this check
 * landed, 153 of 326 commands — 47% — were still synchronous.
 *
 * The fix for a violation is one word: `#[tauri::command]` →
 * `#[tauri::command(async)]`. The function body does not change, and neither
 * does the contract the frontend sees.
 */

import { readdirSync, readFileSync, statSync } from 'node:fs'
import { join, dirname, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const SCAN = ['crates', 'modules', 'apps']
const SKIP = new Set(['target', 'node_modules', '.nuxt', '.output', 'dist', '.git'])

/**
 * Commands that stay on the main thread deliberately.
 *
 * The bar is high: the work must *need* the event loop thread, and it must be
 * bounded. "It is probably fast" is not a reason — a contended lock is fast
 * until the moment it is not.
 */
const ALLOWED = new Map([
  [
    'set_circular_window',
    'mutates the main window\'s Win32 shape (SetWindowLong / SetWindowPos / ' +
      'SetWindowRgn) and belongs on the thread that owns the HWND; a fixed ' +
      'number of non-blocking calls',
  ],
])

const FN = /^\s*(?:pub(?:\([^)]*\))?\s+)?(async\s+)?fn\s+([A-Za-z0-9_]+)/

function* rustFiles(dir) {
  let entries
  try {
    entries = readdirSync(dir)
  } catch {
    return
  }
  for (const name of entries) {
    if (SKIP.has(name)) continue
    const full = join(dir, name)
    if (statSync(full).isDirectory()) yield* rustFiles(full)
    else if (name.endsWith('.rs')) yield full
  }
}

const problems = []
let total = 0
let allowed = 0

for (const root of SCAN) {
  for (const file of rustFiles(join(ROOT, root))) {
    const lines = readFileSync(file, 'utf8').split('\n')
    for (let i = 0; i < lines.length; i++) {
      const attr = lines[i].trim()
      if (attr !== '#[tauri::command]' && attr !== '#[tauri::command(async)]') continue
      // Attributes and doc comments may sit between the attribute and the fn.
      for (let j = i + 1; j < Math.min(i + 12, lines.length); j++) {
        const m = FN.exec(lines[j])
        if (!m) continue
        total++
        const [, isAsync, name] = m
        // `(async)` moves a synchronous body onto a worker; `async fn` is
        // already there. Only a bare attribute on a bare fn stays on the loop.
        if (attr === '#[tauri::command]' && !isAsync) {
          if (ALLOWED.has(name)) allowed++
          else
            problems.push(
              `${relative(ROOT, file).replace(/\\/g, '/')}:${j + 1}: ` +
                `\`${name}\` is synchronous, so Tauri runs it on the main thread — ` +
                `write \`#[tauri::command(async)]\``
            )
        }
        break
      }
    }
  }
}

for (const p of problems) console.error(`  ERROR ${p}`)

if (problems.length) {
  console.error(
    `\n${problems.length} command(s) would block the event loop — see docs/ARCHITECTURE.md §11.\n` +
      `If one genuinely needs the main thread, add it to ALLOWED in this script with a reason.`
  )
  process.exit(1)
}

console.log(
  `  ok    ${total} command(s) off the main thread` +
    (allowed ? ` (${allowed} allowed exception${allowed === 1 ? '' : 's'})` : '')
)
