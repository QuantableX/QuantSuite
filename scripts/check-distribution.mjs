/** Keep setup resources explicit: private code/data must never enter the bundle. */
import assert from 'node:assert/strict'
import { readFileSync, realpathSync, statSync } from 'node:fs'
import { dirname, isAbsolute, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const configDir = resolve(root, 'apps/src-tauri')
const pythonRoot = realpathSync(resolve(root, 'sidecars/python'))
const readConfig = name => JSON.parse(readFileSync(resolve(configDir, name), 'utf8'))

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
    assert(!/(^|\/)(tests|__pycache__|versions|QuantScript|\.venv)(\/|$)/i.test(name), `Private/generated resource: ${source}`)
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
  }
}

const config = readConfig('tauri.conf.json')
for (const name of ['tauri.conf.json', 'tauri.windows.conf.json', 'tauri.dev.conf.json']) {
  const resources = readConfig(name).bundle?.resources
  if (resources) validateResources(resources)
}
assert.equal(config.bundle.resources['../../docs/QUANTSCRIPT-FOLDER.txt'], 'QuantScript/indicators/README.txt')
assert(config.build.beforeBuildCommand.includes('check:distribution'), 'Installer must run the distribution guard')
console.log(`Distribution guard passed: ${Object.keys(config.bundle.resources).length - 1} explicit Python runtime files; private library excluded.`)
