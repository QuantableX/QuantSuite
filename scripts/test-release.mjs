import assert from 'node:assert/strict'
import { test } from 'node:test'
import { mkdtempSync, rmSync, writeFileSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { prepareRelease } from './prepare-release.mjs'

const artifacts = ['QuantSuite_1.0.0_x64-setup.exe', 'QuantSuite.app.tar.gz', 'QuantSuite_1.0.0_amd64.AppImage']
function fixture(t, missing) {
  const root = mkdtempSync(join(tmpdir(), 'quantsuite-release-'))
  t.after(() => rmSync(root, { recursive: true, force: true }))
  for (const asset of artifacts) {
    if (missing === asset) continue
    writeFileSync(join(root, asset), 'installer')
    if (missing !== asset + '.sig') writeFileSync(join(root, asset + '.sig'), 'test-signature-for-release-manifest')
  }
  return root
}
test('one complete manifest serves all supported architectures from QuantSuite', t => {
  const input = fixture(t)
  const manifest = prepareRelease(input, join(input, 'out'), '1.0.0')
  assert.deepEqual(Object.keys(manifest.platforms).sort(), ['darwin-aarch64', 'darwin-x86_64', 'linux-x86_64', 'windows-x86_64'])
  assert.equal(manifest.platforms['darwin-aarch64'].url, manifest.platforms['darwin-x86_64'].url)
  for (const platform of Object.values(manifest.platforms)) assert(platform.url.startsWith('https://github.com/QuantableX/QuantSuite/releases/download/v1.0.0/'))
  assert.equal(readFileSync(join(input, 'out/SHA256SUMS.txt'), 'utf8').trim().split('\n').length, 7)
})
test('a missing platform or signature prevents publishing', t => {
  for (const missing of [...artifacts, ...artifacts.map(name => name + '.sig')]) {
    const input = fixture(t, missing)
    assert.throws(() => prepareRelease(input, join(input, 'out'), '1.0.0'), /Expected exactly one|Missing signature/)
  }
})
