/** Keep setup resources explicit: private code/data must never enter the bundle. */
import assert from 'node:assert/strict'
import { readFileSync, readdirSync, realpathSync, statSync } from 'node:fs'
import { dirname, isAbsolute, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const configDir = resolve(root, 'apps/src-tauri')
const pythonRoot = realpathSync(resolve(root, 'sidecars/python'))
const readConfig = name => JSON.parse(readFileSync(resolve(configDir, name), 'utf8'))

/** A Python source that registers indicators — a private script, never part
 *  of the engine. The new-script template writes `REGISTER = {{…}}` (a format
 *  string), which is not a declaration. */
export function declaresIndicators(text) {
  return /^REGISTER\s*(:[^=\n]*)?=\s*(\{(?!\{)|dict\()/m.test(text)
}

export function validateResources(resources) {
  assert(resources && !Array.isArray(resources), 'Resources must use explicit source/destination mappings')
  for (const [source, target] of Object.entries(resources)) {
    assert(!/[*?\[\]{}]/.test(source + target), `Resource globs are forbidden: ${source}`)
    assert(!target.includes('..') && !target.startsWith('/') && !target.includes(':'), `Unsafe destination: ${target}`)
    if (source === '../../vendor/conpty/' && target === 'conpty/') continue
    if (source === '../../docs/QUANTSCRIPT-FOLDER.txt' && target === 'QuantScript/indicators/README.txt') continue
    assert(source.startsWith('../../sidecars/python/'), `Unreviewed resource source: ${source}`)
    const name = source.slice('../../sidecars/python/'.length)
    assert.equal(target, `sidecars/python/${name}`, `Unexpected runtime destination: ${target}`)
    assert(!/(^|\/)(tests|__pycache__|versions|QuantScript|\.venv|collection|store)(\/|$)/i.test(name), `Private/generated resource: ${source}`)
    // The private library's files: its ledger, the Collection's state, a Store-era state file.
    assert(!/(^|\/)(library|collection|store|catalog)\.json$/i.test(name), `Private library state must never ship: ${source}`)
    assert(!/(^|\/)(research_|verify_|trend_benchmark|benchmark)/.test(name), `Research-only resource: ${source}`)
    assert(/\.(py|txt)$/.test(name), `Unexpected runtime file type: ${source}`)
    if (name.startsWith('smithery/indicators/')) {
      assert(['smithery/indicators/__init__.py', 'smithery/indicators/_discover.py'].includes(name),
        `Private indicator must never ship: ${source}`)
    }
    const path = realpathSync(resolve(configDir, source))
    const rel = relative(pythonRoot, path)
    assert(rel && !rel.startsWith('..') && !isAbsolute(rel), `Resource escapes runtime: ${source}`)
    assert(statSync(path).isFile(), `Recursive resource directories are forbidden: ${source}`)
    if (name.endsWith('.py')) {
      assert(!declaresIndicators(readFileSync(path, 'utf8')), `An indicator script (REGISTER) must never ship: ${source}`)
    }
  }
}

const config = readConfig('tauri.conf.json')
// The CLI imports sibling helpers at runtime. A missing mapping works in dev
// but crashes installed builds before any indexing command can run.
for (const name of readdirSync(resolve(pythonRoot, 'codebase_index')).filter(name => name.endsWith('.py'))) {
  assert.equal(config.bundle.resources[`../../sidecars/python/codebase_index/${name}`],
    `sidecars/python/codebase_index/${name}`, `Missing code-index runtime resource: ${name}`)
}
for (const name of ['tauri.conf.json', 'tauri.windows.conf.json', 'tauri.dev.conf.json']) {
  const resources = readConfig(name).bundle?.resources
  if (resources) validateResources(resources)
}
assert.equal(config.bundle.resources['../../docs/QUANTSCRIPT-FOLDER.txt'], 'QuantScript/indicators/README.txt')
assert(config.build.beforeBuildCommand.includes('check:distribution'), 'Installer must run the distribution guard')
console.log(`Distribution guard passed: ${Object.keys(config.bundle.resources).length - 1} explicit Python runtime files; private library excluded.`)
