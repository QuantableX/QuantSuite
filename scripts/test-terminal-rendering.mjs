/** Real DOM-renderer regression: a TUI redraw split across PTY events must
 * not paint the cursor at intermediate output positions (DEC mode 2026).
 * Run with node --test; set CHROME_BIN if Chrome is not in its standard path.
 * XTERM_TEST_DIR can point at a directory containing xterm.js and xterm.css
 * to check an older release against the same regression.
 */
import assert from 'node:assert/strict'
import { spawn } from 'node:child_process'
import { mkdtemp, readFile, writeFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'
import { test } from 'node:test'
import { parse, compileStyle } from '@vue/compiler-sfc'

test('terminal cursor stays steady through redraws and the console dock overlays the stage', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'qs-terminal-render-'))
  const base = process.env.XTERM_TEST_DIR
  const script = await readFile(base ? join(base, 'xterm.js') : new URL('../node_modules/@xterm/xterm/lib/xterm.js', import.meta.url), 'utf8')
  const css = await readFile(base ? join(base, 'xterm.css') : new URL('../node_modules/@xterm/xterm/css/xterm.css', import.meta.url), 'utf8')
  const consoleCss = await readFile(new URL('../modules/console/app/assets/css/main.css', import.meta.url), 'utf8')
  const paneSource = await readFile(new URL('../modules/console/app/components/Pane.vue', import.meta.url), 'utf8')
  const { descriptor } = parse(paneSource)
  const paneCss = compileStyle({ source: descriptor.styles[0].content, filename: 'Pane.vue', id: 'data-v-cursor-test', scoped: true }).code
  const chrome = process.env.CHROME_BIN ?? (process.platform === 'win32'
    ? 'C:/Program Files/Google/Chrome/Application/chrome.exe' : 'chromium')
  let browser, socket, call
  const pause = () => new Promise(resolve => setTimeout(resolve, 100))
  try {
    await writeFile(join(dir, 'xterm.js'), script)
    await writeFile(join(dir, 'test.html'), `<!doctype html>
<meta charset="utf-8"><style>${css}\n${consoleCss}\n${paneCss}</style>
<div data-module="console" style="position:absolute;left:900px;top:0;height:500px;width:800px">
  <div class="console-page"><div class="console-body"><div class="console-main">
    <main class="console-stage"></main>
    <div class="console-dock" style="display:none;height:150px"></div>
  </div></div></div>
</div>
<div id="terminal" class="console-term" data-v-cursor-test style="width:800px;height:400px"></div><pre id="result">pending</pre>
<script src="xterm.js"></script><script>
(async () => {
  const term = new Terminal({ cols: 80, rows: 24, cursorBlink: false, allowProposedApi: true });
  term.open(document.querySelector('#terminal'));
  term.focus();
  const write = data => new Promise(resolve => term.write(data, resolve));
  const settle = () => new Promise(resolve => setTimeout(resolve, 80));
  const check = (ok, message) => { if (!ok) throw new Error(message); };
  const cursor = () => {
    const rows = [...document.querySelectorAll('.xterm-rows > div')];
    return rows.findIndex(row => row.querySelector('.xterm-cursor'));
  };
  try {
    const stage = document.querySelector('.console-stage');
    const dock = document.querySelector('.console-dock');
    const initial = stage.getBoundingClientRect();
    for (const height of [150, 300, 90]) {
      dock.style.display = 'block';
      dock.style.height = height + 'px';
      const stageRect = stage.getBoundingClientRect();
      const dockRect = dock.getBoundingClientRect();
      check(stageRect.height === initial.height && stageRect.width === initial.width, 'dock resized the main stage');
      check(dockRect.bottom === stageRect.bottom && dockRect.top < stageRect.bottom, 'dock does not overlay stage bottom');
    }
    dock.style.display = 'none';
    check(stage.getBoundingClientRect().height === initial.height, 'closing dock resized stage');
    await write('\\x1b[10;5H');
    await settle();
    check(cursor() === 9, 'initial cursor must be on input row 10');
    let renders = 0;
    const listener = term.onRender(() => renders++);
    // Separate writes model output chunks arriving on different event-loop ticks.
    await write('\\x1b[?2026h\\x1b[2;1HUpdating status');
    await settle();
    check(renders === 0 && cursor() === 9, 'intermediate status cursor was painted');
    await write('\\x1b[4;1HMore output');
    await settle();
    check(renders === 0 && cursor() === 9, 'intermediate output cursor was painted');
    await write('\\x1b[10;5H\\x1b[?2026l');
    await settle();
    check(renders > 0 && cursor() === 9, 'completed frame must restore input cursor');
    check(term.buffer.active.getLine(1).translateToString(true) === 'Updating status', 'status output lost');
    await write('abc');
    await settle();
    check(term.buffer.active.cursorX === 7 && cursor() === 9, 'subsequent input cursor incorrect');
    check(term.buffer.active.getLine(9).translateToString(true) === '    abc', 'subsequent text out of order');
    // An unfinished update must eventually stop suppressing rendering.
    await write('\\x1b[?2026h\\x1b[12;1Htimeout recovery');
    await new Promise(resolve => setTimeout(resolve, 1200));
    check(cursor() === 11, 'unterminated update froze rendering');
    listener.dispose();

    // A TUI can enable cursor blinking independently of cursorBlink:false.
    // Check the real pane stylesheet under both requests and frequent redraws.
    term.options.theme = { background: '#000000', foreground: '#ffffff', cursor: '#ff0000', cursorAccent: '#000000' };
    await write('\\x1b[2J\\x1b[10;5H\\x1b[?12h\\x1b[1 q');
    await settle();
    function cursorPainted() {
      const el = document.querySelector('.xterm-cursor');
      return el && getComputedStyle(el).backgroundColor === 'rgb(255, 0, 0)';
    }
    check(cursorPainted(), 'cursor must initially be visible');
    // Stay visible across multiple normal blink intervals, then across redraws.
    for (let i = 0; i < 16; i++) {
      await settle();
      check(cursorPainted(), 'application blink request hid the steady cursor');
    }
    for (let i = 0; i < 8; i++) {
      term.refresh(0, term.rows - 1);
      await settle();
      check(cursorPainted(), 'redraw hid the steady cursor');
    }
    for (const style of [3, 5]) {
      await write('\\x1b[' + style + ' q');
      await settle();
      check(getComputedStyle(document.querySelector('.xterm-cursor')).animationName === 'none', 'underline/bar cursor still animates');
    }
    await write('\\x1b[?25l');
    await settle();
    check(cursor() === -1, 'explicit cursor hide was ignored');
    await write('\\x1b[?25h');
    await settle();
    check(cursor() === 9, 'explicit cursor show was ignored');
    document.querySelector('#result').textContent = 'PASS';
  } catch (error) {
    document.querySelector('#result').textContent = 'FAIL: ' + error.message;
  } finally { term.dispose(); }
})();
</script>`)
    browser = spawn(chrome, [
      '--headless=new', '--disable-gpu', '--no-first-run', '--no-default-browser-check',
      '--disable-background-timer-throttling', '--run-all-compositor-stages-before-draw',
      '--remote-debugging-port=0', `--user-data-dir=${join(dir, 'profile')}`,
      pathToFileURL(join(dir, 'test.html')).href,
    ], { windowsHide: true, stdio: 'ignore' })
    let launchError
    browser.on('error', error => { launchError = error })
    let port
    for (let i = 0; i < 100 && !port; i++) {
      if (launchError) throw launchError
      try { port = (await readFile(join(dir, 'profile', 'DevToolsActivePort'), 'utf8')).split('\n')[0] } catch { await pause() }
    }
    assert.ok(port, 'Chrome failed to start')
    const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()
    socket = new WebSocket(pages.find(page => page.type === 'page').webSocketDebuggerUrl)
    await new Promise((resolve, reject) => {
      socket.addEventListener('open', resolve, { once: true })
      socket.addEventListener('error', reject, { once: true })
    })
    let sequence = 0
    call = (method, params = {}) => new Promise((resolve, reject) => {
      const id = ++sequence
      const timer = setTimeout(() => { socket.removeEventListener('message', receive); reject(new Error(`${method} timed out`)) }, 5000)
      function receive(event) {
        const response = JSON.parse(event.data)
        if (response.id !== id) return
        clearTimeout(timer)
        socket.removeEventListener('message', receive)
        if (response.error) reject(new Error(response.error.message))
        else resolve(response.result)
      }
      socket.addEventListener('message', receive)
      socket.send(JSON.stringify({ id, method, params }))
    })
    let result
    for (let i = 0; i < 100; i++) {
      const response = await call('Runtime.evaluate', { expression: 'document.querySelector("#result")?.textContent', returnByValue: true })
      result = response.result.value
      if (result && result !== 'pending') break
      await pause()
    }
    assert.equal(result, 'PASS', result ?? 'browser did not finish the rendering test')
  } finally {
    if (call) await call('Browser.close').catch(() => {})
    socket?.close()
    browser?.kill()
    assert.equal(dirname(resolve(dir)), resolve(tmpdir()))
    await rm(dir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 })
  }
})
