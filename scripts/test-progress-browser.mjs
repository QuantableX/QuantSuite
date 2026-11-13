// Real Chromium regression tests; no npm/browser installation required.
// PROGRESS_BROWSER may name a Chrome/Edge executable. Optional PROGRESS_TEST_ARTIFACTS
// selects the output directory; the default is QuantSuite's private artifact folder.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import fs from 'node:fs/promises'
import { existsSync } from 'node:fs'
import http from 'node:http'
import os from 'node:os'
import path from 'node:path'
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const root = fileURLToPath(new URL('../', import.meta.url))
const browser = [process.env.PROGRESS_BROWSER,
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  '/usr/bin/google-chrome', '/usr/bin/chromium',
  '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
].find(p => p && existsSync(p))
const artifactRoot = process.env.PROGRESS_TEST_ARTIFACTS ?? path.join(process.env.QUANTSUITE_HOME ?? path.join(os.homedir(), '.quantsuite'), 'artifacts', 'progress-scroll-tests')

test('live updates preserve real scrolling and DOM interaction', { skip: !browser && 'Set PROGRESS_BROWSER to an installed Chromium browser', timeout: 40_000 }, async () => {
  await fs.mkdir(artifactRoot, { recursive: true })
  const artifacts = await fs.mkdtemp(path.join(artifactRoot, 'run-'))
  let report
  const reported = new Promise(resolve => { report = resolve })
  const routes = new Map([
    ['/', ['scripts/fixtures/progress-browser.html', 'text/html']],
    ['/board.js', ['packages/ui/src/progress/board.js', 'text/javascript']],
    ['/board.css', ['packages/ui/src/progress/board.css', 'text/css']],
  ])
  const server = http.createServer(async (req, res) => {
    if (req.url === '/result' && req.method === 'POST') {
      let body = ''
      for await (const chunk of req) body += chunk
      report(JSON.parse(body)); res.end('ok'); return
    }
    const route = routes.get(req.url)
    if (!route) { res.writeHead(404).end(); return }
    res.setHeader('Content-Type', `${route[1]}; charset=utf-8`)
    res.end(await fs.readFile(path.join(root, route[0])))
  })
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
  const profile = path.join(artifacts, 'profile')
  const child = spawn(browser, ['--headless=new', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
    '--remote-debugging-port=0', `--user-data-dir=${profile}`, `http://127.0.0.1:${server.address().port}/`], { windowsHide: true, stdio: 'ignore' })
  const startupError = new Promise((_, reject) => child.once('error', reject))
  let timeout
  try {
    const result = await Promise.race([reported, startupError, new Promise((_, reject) => { timeout = setTimeout(() => reject(new Error('Browser test timed out')), 30_000) })])
    await fs.writeFile(path.join(artifacts, 'result.json'), JSON.stringify(result, null, 2))
    console.log(`Browser results: ${path.join(artifacts, 'result.json')}`)
    assert.deepEqual(result.failures, [])
    assert.ok(result.passed.length >= 6)
    console.log(result.passed.join('\n'))
  } finally {
    clearTimeout(timeout)
    // Close only this test's browser through its private debugging endpoint.
    try {
      const [port] = (await fs.readFile(path.join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')
      const { webSocketDebuggerUrl } = await fetch(`http://127.0.0.1:${port}/json/version`).then(r => r.json())
      const ws = new WebSocket(webSocketDebuggerUrl)
      await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject })
      ws.send(JSON.stringify({ id: 1, method: 'Browser.close' }))
      await new Promise(resolve => { ws.onclose = resolve; setTimeout(resolve, 1500) })
      ws.close()
    } catch { child.kill() }
    server.closeAllConnections()
    await new Promise(resolve => server.close(resolve))
  }
})
