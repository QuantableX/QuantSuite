// `npm run tauri:build`. Release builds sign the updater artifacts with the key in
// TAURI_SIGNING_PRIVATE_KEY. Without that key (any local machine) the build merges
// tauri.local-build.conf.json and produces unsigned installers instead of failing
// at the signing step after the whole compile.
import { spawnSync } from 'node:child_process'
import { createRequire } from 'node:module'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const tauri = createRequire(import.meta.url).resolve('@tauri-apps/cli/tauri.js')
const args = process.argv.slice(2)
if (!process.env.TAURI_SIGNING_PRIVATE_KEY) {
  console.warn('TAURI_SIGNING_PRIVATE_KEY is not set: building without updater signatures.')
  args.unshift('--config', resolve(root, 'apps/src-tauri/tauri.local-build.conf.json'))
}
const { status, error } = spawnSync(process.execPath, [tauri, 'build', ...args], { stdio: 'inherit' })
if (error) throw error
process.exit(status ?? 1)
