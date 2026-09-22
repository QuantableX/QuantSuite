#!/usr/bin/env node
// End-to-end test of the QuantMCP server (PLAN-WORKSPACE-UNIFY §6).
//
// Talks to a RUNNING QuantSuite instance over streamable HTTP exactly like an
// external MCP client (Claude Code etc.) would: initialize → tools/list →
// tools/call. Exercises the workspace-unified tool families end to end —
// codebase index (with auto-register), kanban lifecycle (claim → worktree →
// diff → complete/approve/reject/cancel), agent worktrees, AgentOS, and the
// get_instructions contract (plus the connect-time `instructions` brief). The bridge capabilities (quantsuite.*) are
// asserted present in tools/list but not called: they dispatch through the
// shell webview, which a headless test cannot assume.
//
//   node scripts/test-mcp-server.mjs [--url http://127.0.0.1:3101/mcp]
//        [--workspace-dir <folder>]   test workspace folder (created if missing)
//        [--core-db <path>]           dev core.db — enables registry cleanup
//        [--keep]                     keep the test workspace + registry entry
//
// Safe by design: only ever writes inside the test workspace folder and the
// suite's own kanban/settings for that one workspace. Never touches
// `core / workspace.active`. Default target is the DEV port 3101 — never run
// this against a production 3100 you care about.

import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'

// ── args ──────────────────────────────────────────────────────────────────
const args = process.argv.slice(2)
function argOf(flag, fallback) {
  const i = args.indexOf(flag)
  return i >= 0 && args[i + 1] ? args[i + 1] : fallback
}
const URL_ = argOf('--url', 'http://127.0.0.1:3101/mcp')
const WS_DIR = path.resolve(argOf('--workspace-dir', path.join(os.tmpdir(), 'qs-mcp-testws')))
const CORE_DB = argOf('--core-db', null)
const KEEP = args.includes('--keep')
const WS_NAME = path.basename(WS_DIR)

// ── tiny harness ──────────────────────────────────────────────────────────
let passed = 0
let failed = 0
const failures = []
function check(name, cond, detail = '') {
  if (cond) {
    passed++
    console.log(`  ok    ${name}`)
  } else {
    failed++
    failures.push(name)
    console.log(`  FAIL  ${name}${detail ? ` — ${String(detail).slice(0, 300)}` : ''}`)
  }
}
function section(title) {
  console.log(`\n── ${title} ──`)
}

// ── MCP client (streamable HTTP, plain JSON responses) ────────────────────
let sessionId = null
let rpcId = 0
async function rpc(method, params) {
  const headers = { 'Content-Type': 'application/json' }
  if (sessionId) headers['mcp-session-id'] = sessionId
  const res = await fetch(URL_, {
    method: 'POST',
    headers,
    body: JSON.stringify({ jsonrpc: '2.0', id: ++rpcId, method, params }),
  })
  const sid = res.headers.get('mcp-session-id')
  if (sid) sessionId = sid
  if (res.status === 202) return null // notification accepted
  return res.json()
}

/** tools/call → { text, isError }. Every QuantMCP tool returns one text block. */
async function call(name, callArgs = {}) {
  const res = await rpc('tools/call', { name, arguments: callArgs })
  if (res.error) return { text: JSON.stringify(res.error), isError: true }
  const text = res.result?.content?.[0]?.text ?? ''
  return { text, isError: !!res.result?.isError }
}

// ── test workspace folder (sample code + git repo) ────────────────────────
function git(cwd, ...a) {
  return execFileSync('git', a, { cwd, encoding: 'utf8' }).trim()
}

function setupWorkspaceFolder() {
  fs.mkdirSync(WS_DIR, { recursive: true })
  fs.writeFileSync(
    path.join(WS_DIR, 'alpha.py'),
    'def alpha_function(x):\n    """The alpha marker symbol."""\n    return x * 2\n\n\nclass AlphaThing:\n    pass\n',
  )
  fs.writeFileSync(
    path.join(WS_DIR, 'beta.ts'),
    'export function betaHelper(n: number): number {\n  return n + 1\n}\n',
  )
  if (!fs.existsSync(path.join(WS_DIR, '.git'))) {
    git(WS_DIR, 'init', '-b', 'main')
    git(WS_DIR, 'config', 'user.name', 'MCP Test')
    git(WS_DIR, 'config', 'user.email', 'mcp-test@localhost')
  }
  git(WS_DIR, 'add', '-A')
  try {
    git(WS_DIR, 'commit', '-m', 'test fixture')
  } catch {
    /* nothing to commit on re-runs */
  }
}

// ── the 33 inbuilt tools the server must list ─────────────────────────────
const INBUILT = [
  // codebase index (7)
  'get_instructions', 'index_codebase', 'search_code', 'lookup_symbol',
  'list_codebases', 'reindex_codebase', 'get_codebase_stats',
  // agentos (3) — AGENT.md only since 2026-08-31; the concept tools are gone
  'get_agent_instructions', 'update_agent_instructions', 'init_agent_md',
  // worktrees (7)
  'create_worktree', 'list_worktrees', 'get_worktree_status', 'get_worktree_test_command',
  'get_worktree_diff', 'merge_worktree', 'remove_worktree',
  // kanban (16)
  'list_kanban_cards', 'get_kanban_card', 'create_kanban_card',
  'move_kanban_card', 'move_kanban_card_to_workspace',
  'update_kanban_card', 'delete_kanban_card',
  'claim_kanban_card', 'complete_kanban_card', 'approve_kanban_card',
  'reject_kanban_card', 'cancel_kanban_card', 'get_kanban_diff',
  'get_kanban_activity_log', 'get_kanban_approval_mode', 'set_kanban_approval_mode',
]

const cardIds = []
let b36 = null

async function main() {
  console.log(`QuantMCP server test — ${URL_}`)
  console.log(`Test workspace: ${WS_DIR} (name: ${WS_NAME})`)

  // ── handshake ──
  section('handshake')
  const init = await rpc('initialize', {
    protocolVersion: '2025-03-26',
    capabilities: {},
    clientInfo: { name: 'qs-mcp-test', version: '1.0.0' },
  })
  check('initialize returns serverInfo QuantMCP', init?.result?.serverInfo?.name === 'QuantMCP', JSON.stringify(init))
  check('initialize returns a session id', !!sessionId)
  // The connect-time brief (MCP `instructions`): the global AGENT.md — the
  // operator's rules for every agent — which the clients put into the system
  // prompt, so an agent reaches for the tools without being told about them.
  const brief = init?.result?.instructions ?? ''
  check('initialize carries the global AGENT.md as `instructions`',
    brief.length > 500 && brief.includes('QuantMCP') && brief.includes('Call get_instructions now'), brief.slice(0, 200))
  await rpc('notifications/initialized', {})

  // ── tools/list ──
  section('tools/list')
  const list = await rpc('tools/list', {})
  const tools = list?.result?.tools ?? []
  const names = new Set(tools.map((t) => t.name))
  const missing = INBUILT.filter((n) => !names.has(n))
  check(`all ${INBUILT.length} inbuilt tools listed`, missing.length === 0, `missing: ${missing.join(', ')}`)

  const byName = Object.fromEntries(tools.map((t) => [t.name, t]))
  const wsParam = (tool) => !!byName[tool]?.inputSchema?.properties?.workspace
  const notRequired = (tool, param) => !(byName[tool]?.inputSchema?.required ?? []).includes(param)
  for (const t of ['search_code', 'index_codebase', 'reindex_codebase', 'get_codebase_stats', 'lookup_symbol',
                   'list_kanban_cards', 'create_kanban_card', 'get_kanban_approval_mode', 'set_kanban_approval_mode',
                   'get_agent_instructions']) {
    check(`${t} schema has optional \`workspace\``, wsParam(t) && notRequired(t, 'workspace'))
  }
  check('no tool schema requires project_name anymore',
    tools.every((t) => !(t.inputSchema?.required ?? []).includes('project_name')))
  const bridge = tools.filter((t) => t.name.startsWith('quantsuite.'))
  check('bridge catalogue served (quantsuite.*)', bridge.length >= 20, `got ${bridge.length}`)
  check('quantsuite.memory.* present (13 tools)', bridge.filter((t) => t.name.startsWith('quantsuite.memory.')).length === 13)

  // ── codebase index + auto-register ──
  section('codebase index (auto-register, search, reindex)')
  setupWorkspaceFolder()

  const idx = await call('index_codebase', { path: WS_DIR, mode: 'structural' })
  let idxJson = null
  try { idxJson = JSON.parse(idx.text) } catch { /* handled below */ }
  check('index_codebase on unregistered folder succeeds', !idx.isError && idxJson?.status === 'ok', idx.text)
  b36 = idxJson?.codebase ?? null
  check('index DB is named by the workspace id (b36, not folder basename)',
    !!b36 && b36 !== WS_NAME.toLowerCase() && /^[0-9a-z]+$/.test(b36), `codebase: ${b36}`)
  check('index DB lives in the suite module dir', (idxJson?.db_path ?? '').includes(path.join('modules', 'mcp', 'indexes')), idxJson?.db_path)

  const lc = await call('list_codebases', {})
  check('list_codebases shows the auto-registered workspace', lc.text.includes(WS_NAME), lc.text)
  check('list_codebases shows it as indexed', new RegExp(`${WS_NAME}.*indexed \\(mode: structural\\)`).test(lc.text), lc.text)

  const sc = await call('search_code', { workspace: WS_NAME, query: 'alpha_function' })
  check('search_code by workspace NAME finds the marker symbol', !sc.isError && sc.text.includes('alpha_function'), sc.text)

  const scAlias = await call('search_code', { codebase: WS_NAME, query: 'betaHelper' })
  check('legacy `codebase` alias still resolves', !scAlias.isError && scAlias.text.includes('betaHelper'), scAlias.text)

  const scPath = await call('search_code', { workspace: WS_DIR, query: 'alpha_function' })
  check('workspace by folder PATH resolves too', !scPath.isError && scPath.text.includes('alpha_function'), scPath.text)

  const ls = await call('lookup_symbol', { workspace: WS_NAME, symbol_name: 'alpha_function' })
  check('lookup_symbol finds the exact symbol', !ls.isError && ls.text.includes('"count": 1') || ls.text.includes('alpha_function'), ls.text)

  const st1 = await call('get_codebase_stats', { workspace: WS_NAME })
  const st1Json = JSON.parse(st1.text)
  check('get_codebase_stats reports entries', !st1.isError && (st1Json.fts_entry_count ?? 0) > 0, st1.text)

  const ri = await call('reindex_codebase', { workspace: WS_NAME })
  const st2 = await call('get_codebase_stats', { workspace: WS_NAME })
  const st2Json = JSON.parse(st2.text)
  check('reindex keeps the index populated (regression: reindex-wipe bug)',
    !ri.isError && (st2Json.fts_entry_count ?? 0) > 0, `after reindex: ${st2.text}`)

  const bad = await call('search_code', { workspace: 'no-such-workspace-xyz', query: 'x' })
  check('unknown workspace errors and names the registered ones',
    bad.isError && bad.text.includes("Unknown workspace 'no-such-workspace-xyz'"), bad.text)

  // Three valid outcomes, all proving the default path works: the active
  // workspace answered with stats, the active workspace answered with the
  // not-indexed hint (resolution worked, index just absent), or there is no
  // active workspace and the error says exactly that.
  const noArg = await call('get_codebase_stats', {})
  const noArgOk =
    !noArg.isError ||
    noArg.text.includes('No active workspace') ||
    noArg.text.includes('has no code index yet')
  check('omitted workspace resolves to active or explains itself', noArgOk, noArg.text)
  console.log(
    `        (default resolution: ${
      noArg.text.includes('No active workspace')
        ? 'no active workspace — error path'
        : 'active workspace answered'
    })`,
  )

  // ── kanban: auto_apply lifecycle ──
  section('kanban (auto_apply: claim → worktree → diff → complete-merges)')
  const mkCard = async (title) => {
    const r = await call('create_kanban_card', { workspace: WS_NAME, title })
    const id = r.text.match(/id: ([0-9a-f-]{36})/)?.[1] ?? null
    check(`create_kanban_card "${title}"`, !r.isError && !!id, r.text)
    if (id) cardIds.push(id)
    return id
  }

  // The reply's legend names both modes, so read the reported one.
  const modeOf = (text) => text.match(/approval mode: (\w+)/)?.[1] ?? null
  const modeDefault = await call('get_kanban_approval_mode', { workspace: WS_NAME })
  check('approval mode defaults to auto_apply (fresh board)', modeOf(modeDefault.text) === 'auto_apply', modeDefault.text)
  if (modeOf(modeDefault.text) !== 'auto_apply') {
    // A run that died between the approval section and its reset leaves the
    // row behind (cleanup drops it now); carry on from the state this needs.
    await call('set_kanban_approval_mode', { workspace: WS_NAME, mode: 'auto_apply' })
  }

  const cardA = await mkCard('Test card A (auto merge)')
  const listWs = await call('list_kanban_cards', { workspace: WS_NAME })
  check('list_kanban_cards by workspace', listWs.text.includes('Test card A'), listWs.text)
  const listAlias = await call('list_kanban_cards', { project_name: WS_NAME })
  check('legacy `project_name` alias still lists', listAlias.text.includes('Test card A'), listAlias.text)

  const getA = await call('get_kanban_card', { card_id: cardA })
  check('get_kanban_card names the board', getA.text.includes(`Board: ${WS_NAME}`), getA.text)

  const claimA = await call('claim_kanban_card', { card_id: cardA, agent_id: 'test-agent' })
  const worktreeA = claimA.text.match(/Worktree: (.+)/)?.[1]?.trim() ?? null
  check('claim creates an isolated worktree under .qs-worktrees/',
    !claimA.isError && !!worktreeA && worktreeA.includes('.qs-worktrees') && fs.existsSync(worktreeA), claimA.text)
  check('.gitignore got the .qs-worktrees/ entry',
    fs.readFileSync(path.join(WS_DIR, '.gitignore'), 'utf8').includes('.qs-worktrees/'))
  // The user's run-the-app one-liner (PLAN-WORKTREES.md "Testing a worktree"):
  // quoted by the claim reply, returned again by its own tool, on the card.
  check('claim reply carries the user\'s test commands (shared main target, three shells)',
    claimA.text.includes('Test commands (for the USER') && claimA.text.includes('$env:CARGO_TARGET_DIR')
      && claimA.text.includes('cd /d ') && claimA.text.includes('CARGO_TARGET_DIR="')
      && claimA.text.includes('npm run tauri:dev'), claimA.text)
  const testCmdA = await call('get_worktree_test_command', { card_id: cardA })
  check('get_worktree_test_command by card_id',
    !testCmdA.isError && testCmdA.text.includes('npm run tauri:dev') && testCmdA.text.includes('.qs-worktrees'), testCmdA.text)
  const testCmdByWt = await call('get_worktree_test_command', { worktree: worktreeA, workspace: WS_NAME })
  check('get_worktree_test_command by worktree path',
    !testCmdByWt.isError && testCmdByWt.text.includes('$env:CARGO_TARGET_DIR'), testCmdByWt.text)
  const getClaimedA = await call('get_kanban_card', { card_id: cardA })
  check('get_kanban_card quotes the test command once claimed', getClaimedA.text.includes('Test command'), getClaimedA.text)

  fs.writeFileSync(path.join(worktreeA, 'feature_a.md'), '# delivered by test-agent (card A)\n')
  git(worktreeA, 'add', '-A')
  git(worktreeA, 'commit', '-m', 'card A work')

  const diffA = await call('get_kanban_diff', { card_id: cardA })
  check('get_kanban_diff shows the agent change', !diffA.isError && diffA.text.includes('feature_a.md'), diffA.text)

  const doneA = await call('complete_kanban_card', { card_id: cardA, agent_id: 'test-agent' })
  check('complete auto-merges in auto_apply mode', !doneA.isError && doneA.text.includes('MERGED'), doneA.text)
  check('merged file landed on main', fs.existsSync(path.join(WS_DIR, 'feature_a.md')))
  check('worktree cleaned up after merge', !fs.existsSync(worktreeA))

  // ── kanban: approval lifecycle ──
  section('kanban (approval: complete → review → reject → approve)')
  const setMode = await call('set_kanban_approval_mode', { workspace: WS_NAME, mode: 'approval' })
  check('set_kanban_approval_mode → approval', !setMode.isError && setMode.text.includes("'approval'"), setMode.text)

  const cardB = await mkCard('Test card B (review flow)')
  const claimB = await call('claim_kanban_card', { card_id: cardB, agent_id: 'test-agent' })
  const worktreeB = claimB.text.match(/Worktree: (.+)/)?.[1]?.trim() ?? null
  check('claim card B', !claimB.isError && !!worktreeB, claimB.text)
  fs.writeFileSync(path.join(worktreeB, 'feature_b.md'), '# delivered by test-agent (card B)\n')
  git(worktreeB, 'add', '-A')
  git(worktreeB, 'commit', '-m', 'card B work')

  const doneB = await call('complete_kanban_card', { card_id: cardB })
  check('complete parks in review in approval mode', !doneB.isError && doneB.text.includes('AWAITING_REVIEW'), doneB.text)

  const rejB = await call('reject_kanban_card', { card_id: cardB, reason: 'needs polish' })
  check('reject sends the card back to work, worktree preserved',
    !rejB.isError && rejB.text.includes('IN_PROGRESS') && fs.existsSync(worktreeB), rejB.text)

  const doneB2 = await call('complete_kanban_card', { card_id: cardB })
  const apprB = await call('approve_kanban_card', { card_id: cardB })
  check('approve merges and cleans up', !apprB.isError && apprB.text.includes('MERGED') && fs.existsSync(path.join(WS_DIR, 'feature_b.md')) && !fs.existsSync(worktreeB), apprB.text)

  const modeBack = await call('set_kanban_approval_mode', { workspace: WS_NAME, mode: 'auto_apply' })
  check('approval mode back to auto_apply', !modeBack.isError, modeBack.text)

  // ── kanban: cancel ──
  section('kanban (cancel cleans up)')
  const cardC = await mkCard('Test card C (cancel)')
  const claimC = await call('claim_kanban_card', { card_id: cardC, agent_id: 'test-agent' })
  const worktreeC = claimC.text.match(/Worktree: (.+)/)?.[1]?.trim() ?? null
  const cancC = await call('cancel_kanban_card', { card_id: cardC })
  check('cancel removes worktree and branch', !cancC.isError && cancC.text.includes('CANCELLED') && (!worktreeC || !fs.existsSync(worktreeC)), cancC.text)

  const log = await call('get_kanban_activity_log', { limit: 50 })
  check('activity log recorded the lifecycle', ['claimed', 'completed', 'approved', 'cancelled'].every((a) => log.text.includes(a)), log.text.slice(0, 200))

  // ── kanban: the General board (PLAN-KANBAN-UNIFY) ──
  section('kanban (General board: no folder, move to workspace to claim)')
  const mkG = await call('create_kanban_card', { workspace: 'general', title: 'Test card G (general)' })
  const cardG = mkG.text.match(/id: ([0-9a-f-]{36})/)?.[1] ?? null
  if (cardG) cardIds.push(cardG)
  check("create_kanban_card on board 'General'", !mkG.isError && !!cardG && mkG.text.includes("board 'General'"), mkG.text)

  const listG = await call('list_kanban_cards', { workspace: 'general' })
  check('list_kanban_cards general lists it', listG.text.includes('Test card G'), listG.text)
  const getG = await call('get_kanban_card', { card_id: cardG })
  check('get_kanban_card names the General board', getG.text.includes('Board: General'), getG.text)

  const claimG = await call('claim_kanban_card', { card_id: cardG, agent_id: 'test-agent' })
  check('claim on General refused (no folder)', claimG.isError && claimG.text.includes('General board'), claimG.text)
  const apprModeG = await call('get_kanban_approval_mode', { workspace: 'general' })
  check("approval mode readable on board 'General'", !apprModeG.isError && apprModeG.text.includes("Board 'General'"), apprModeG.text)
  const setModeG = await call('set_kanban_approval_mode', { workspace: 'general', mode: 'approval' })
  check('approval mode settable on General', !setModeG.isError && setModeG.text.includes("'approval'"), setModeG.text)
  const setModeGBack = await call('set_kanban_approval_mode', { workspace: 'general', mode: 'auto_apply' })
  check('…and back to auto_apply', !setModeGBack.isError, setModeGBack.text)

  const mvG = await call('move_kanban_card_to_workspace', { card_id: cardG, workspace: WS_NAME })
  check('move_kanban_card_to_workspace re-homes it', !mvG.isError && mvG.text.includes(`board '${WS_NAME}'`), mvG.text)
  const claimG2 = await call('claim_kanban_card', { card_id: cardG, agent_id: 'test-agent' })
  check('card claimable after the move', !claimG2.isError, claimG2.text)
  const mvBack = await call('move_kanban_card_to_workspace', { card_id: cardG, workspace: 'general' })
  check('a claimed card refuses to change boards', mvBack.isError, mvBack.text)
  const cancG = await call('cancel_kanban_card', { card_id: cardG })
  check('cancel cleans the moved card up', !cancG.isError, cancG.text)

  // ── agentos ──
  section('agentos (workspace-resolved AGENT.md files)')
  const marker = `test-marker-${Date.now()}`
  const up = await call('update_agent_instructions', { scope: 'project', workspace: WS_NAME, content: `# Test AGENT.md\n\n${marker}\n` })
  check('update_agent_instructions via workspace ident', !up.isError, up.text)
  check('AGENT.md written into the workspace folder', fs.existsSync(path.join(WS_DIR, 'AGENT.md')))
  const ga = await call('get_agent_instructions', { scope: 'project', workspace: WS_NAME })
  check('get_agent_instructions reads it back', ga.text.includes(marker), ga.text)

  const gGlobal = await call('get_agent_instructions', { scope: 'global' })
  check('global scope answers with the operator\'s brief', !gGlobal.isError && gGlobal.text.trim().length > 0, gGlobal.text)
  const gAll = await call('get_agent_instructions', { scope: 'all', workspace: WS_NAME })
  check('scope "all" labels global first, then the workspace file as an addition, and keeps both',
    gAll.text.startsWith('─── Global AGENT.md') && gAll.text.includes(gGlobal.text.trim().slice(0, 40))
      && gAll.text.includes(`─── Workspace AGENT.md: ${WS_NAME}`) && gAll.text.includes('never overrides the global one')
      && gAll.text.includes(marker) && gAll.text.indexOf('─── Global') < gAll.text.indexOf('─── Workspace'),
    gAll.text.slice(0, 300))

  // ── get_instructions contract ──
  section('get_instructions')
  const gi = await call('get_instructions', {})
  check('explains the workspace model', gi.text.includes('workspace is a registered project folder'), gi.text.slice(0, 200))
  check('lists the test workspace', gi.text.includes(WS_NAME))
  check('merges the AGENT.md brief', gi.text.includes('Agent Instructions (from AGENT.md)') && gi.text.includes(gGlobal.text.trim().slice(0, 40)))
  check('points agents at QuantMemory scopes', gi.text.includes('quantsuite.memory.search'))

  // ── cleanup ──
  section('cleanup')
  for (const id of cardIds) await call('delete_kanban_card', { card_id: id })
  console.log(`  ok    deleted ${cardIds.length} test card(s)`)
  if (!KEEP) {
    if (CORE_DB && b36) {
      try {
        const { DatabaseSync } = await import('node:sqlite')
        const db = new DatabaseSync(CORE_DB)
        db.prepare('DELETE FROM entities WHERE id = ?').run(`core:workspace:${b36}`)
        // The board's approval-mode row would otherwise outlive the workspace
        // and skew the next run's "defaults to auto_apply".
        db.prepare("DELETE FROM settings WHERE scope = 'mcp' AND key = ?").run(`workspace.${b36}.approval_mode`)
        db.close()
        console.log(`  ok    removed registry entry core:workspace:${b36} and its approval-mode row`)
        const idxDb = CORE_DB.replace(/core\.db$/, path.join('modules', 'mcp', 'indexes', `${b36}.db`))
        if (fs.existsSync(idxDb)) fs.rmSync(idxDb)
        console.log('  ok    removed test index DB')
      } catch (e) {
        console.log(`  note  registry cleanup skipped: ${e.message}`)
      }
    } else {
      console.log('  note  test workspace stays registered (pass --core-db to clean the registry)')
    }
    fs.rmSync(WS_DIR, { recursive: true, force: true })
    console.log('  ok    removed test workspace folder')
  }

  // ── summary ──
  console.log(`\n${'─'.repeat(50)}`)
  console.log(`${passed} passed, ${failed} failed`)
  if (failed) {
    console.log('Failed checks:')
    for (const f of failures) console.log(`  - ${f}`)
  }
  process.exit(failed ? 1 : 0)
}

main().catch((e) => {
  console.error(`\nFATAL: ${e.message}`)
  console.error('Is a QuantSuite dev instance running? (MCP server on the --url port)')
  process.exit(2)
})
