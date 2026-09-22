import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { copyFileSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { basename, dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

function files(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap(entry =>
    entry.isDirectory() ? files(resolve(dir, entry.name)) : [resolve(dir, entry.name)])
}

export function prepareRelease(input, output, version, repo = 'QuantableX/QuantSuite') {
  assert.match(version, /^\d+\.\d+\.\d+$/)
  assert.equal(repo, 'QuantableX/QuantSuite')
  const assets = files(input).filter(path => /\.(exe|msi|dmg|deb|rpm|AppImage|app\.tar\.gz|sig)$/.test(path))
  const names = new Set(assets.map(path => basename(path)))
  assert.equal(names.size, assets.length, 'Duplicate release filenames')
  const asset = pattern => {
    const matches = assets.filter(path => pattern.test(basename(path)))
    assert.equal(matches.length, 1, `Expected exactly one ${pattern} updater asset`)
    const name = basename(matches[0])
    const sig = assets.find(path => basename(path) === `${name}.sig`)
    assert(sig, `Missing signature for ${name}`)
    const signature = readFileSync(sig, 'utf8').trim()
    assert(signature.length > 20, `Empty or invalid signature for ${name}`)
    return { signature, url: `https://github.com/${repo}/releases/download/v${version}/${encodeURIComponent(name)}` }
  }
  const windows = asset(/_x64-setup\.exe$/i)
  const mac = asset(/\.app\.tar\.gz$/i)
  const linux = asset(/_amd64\.AppImage$/i)
  const manifest = {
    version,
    notes: `QuantSuite ${version}. See https://github.com/${repo}/releases/tag/v${version} for release notes.`,
    pub_date: new Date().toISOString(),
    platforms: { 'windows-x86_64': windows, 'darwin-aarch64': mac, 'darwin-x86_64': mac, 'linux-x86_64': linux },
  }
  mkdirSync(output, { recursive: true })
  for (const path of assets) copyFileSync(path, resolve(output, basename(path)))
  writeFileSync(resolve(output, 'latest.json'), JSON.stringify(manifest, null, 2) + '\n')
  const checksums = [...names, 'latest.json'].sort().map(name =>
    `${createHash('sha256').update(readFileSync(resolve(output, name))).digest('hex')}  ${name}`)
  writeFileSync(resolve(output, 'SHA256SUMS.txt'), checksums.join('\n') + '\n')
  return manifest
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
  const version = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8')).version
  assert(process.argv[2] && process.argv[3], 'Usage: node scripts/prepare-release.mjs <artifacts> <output>')
  prepareRelease(resolve(process.argv[2]), resolve(process.argv[3]), version)
  console.log(`Prepared signed QuantSuite ${version} release for Windows, macOS (Intel + Apple Silicon), and Linux.`)
}
