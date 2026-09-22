import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { resolve, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const read = path => readFileSync(resolve(root, path), 'utf8')
const json = path => JSON.parse(read(path))
const version = json('package.json').version
assert.match(version, /^\d+\.\d+\.\d+$/)
for (const path of ['apps/shell/package.json', 'packages/core/package.json', 'packages/ui/package.json', 'apps/src-tauri/tauri.conf.json', 'package-lock.json']) {
  assert.equal(json(path).version, version, `${path}: version mismatch`)
  for (const [name, dependencyVersion] of Object.entries(json(path).dependencies ?? {})) {
    if (name.startsWith('@quantsuite/')) assert.equal(dependencyVersion, version, `${path}: workspace dependency ${name}`)
  }
}
for (const path of ['', 'apps/shell', 'packages/core', 'packages/ui']) {
  assert.equal(json('package-lock.json').packages[path].version, version, `npm lock entry ${path}`)
}
assert.equal(read('Cargo.toml').match(/\[workspace.package\][\s\S]*?version = "([^"]+)"/)[1], version)
for (const entry of read('Cargo.lock').split('[[package]]').slice(1)) {
  if (!entry.includes('\nsource = ')) assert.equal(entry.match(/version = "([^"]+)"/)[1], entry.includes('name = "tao"') ? '0.35.3' : version, 'Cargo lock workspace version')
}
if (process.env.GITHUB_REF_TYPE === 'tag') assert.equal(process.env.GITHUB_REF_NAME, `v${version}`, 'Release tag must match app version')
const config = json('apps/src-tauri/tauri.conf.json')
assert.deepEqual(config.plugins.updater.endpoints, ['https://github.com/QuantableX/QuantSuite/releases/latest/download/latest.json'])
assert.equal(config.bundle.createUpdaterArtifacts, true)
assert(Buffer.from(config.plugins.updater.pubkey, 'base64').toString().includes('minisign public key'), 'Missing updater public key')

// A module may never grow a second application updater or an old release URL.
function scan(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (['node_modules', 'target', '.nuxt', '.output', 'permissions'].includes(entry.name)) continue
    const path = resolve(dir, entry.name)
    if (entry.isDirectory()) scan(path)
    else if (/\.(rs|ts|vue)$/.test(entry.name)) {
      assert(!/check_for_update|download_and_install_update|checkForUpdate|api\.github\.com\/repos\/QuantableX\/Quant\w+\/releases/.test(readFileSync(path, 'utf8')), `Module updater found: ${path}`)
    }
  }
}
scan(resolve(root, 'modules'))
console.log(`Release guard passed: QuantSuite ${version}; one signed suite update channel.`)
