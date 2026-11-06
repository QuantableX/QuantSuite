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
// The board's approval mode as the user sets it in the UI: the core.db row the
// MCP server reads on every kanban call (settings scope 'mcp').
let coreDb = null
if (CORE_DB) {
  const { DatabaseSync } = await import('node:sqlite')
  coreDb = new DatabaseSync(CORE_DB)
}
function setModeAsUser(mode) {
  coreDb.prepare(
    "INSERT INTO settings (scope, key, value) VALUES ('mcp', ?, ?) ON CONFLICT(scope, key) DO UPDATE SET value = excluded.value",
  ).run(`workspace.${b36}.approval_mode`, JSON.stringify(mode))
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

/** A second, independent agent session: its own session id, so its own folder. */
async function agentSession(clientName, capabilities = {}) {
  let sid = null
  let id = 0
  const post = async (body) => {
    const headers = { 'Content-Type': 'application/json' }
    if (sid) headers['mcp-session-id'] = sid
    const res = await fetch(URL_, { method: 'POST', headers, body: JSON.stringify(body) })
    const got = res.headers.get('mcp-session-id')
    if (got) sid = got
    return res.status === 202 ? null : res.json()
  }
  await post({ jsonrpc: '2.0', id: ++id, method: 'initialize', params: {
    protocolVersion: '2025-03-26', capabilities, clientInfo: { name: clientName, version: '1.0.0' },
  } })
  return {
    post,
    get sid() { return sid },
    async call(name, callArgs = {}) {
      const res = await post({ jsonrpc: '2.0', id: ++id, method: 'tools/call', params: { name, arguments: callArgs } })
      if (res.error) return { text: JSON.stringify(res.error), isError: true }
      return { text: res.result?.content?.[0]?.text ?? '', isError: !!res.result?.isError }
    },
  }
}

const cardIdIn = (text) => text.match(/\(id: ([0-9a-f-]{36})\)/)?.[1]

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

// ── the 34 inbuilt tools the server must list ─────────────────────────────
const INBUILT = [
  // codebase index (8)
  'get_instructions', 'answer_workspace_question', 'index_codebase', 'search_code', 'lookup_symbol',
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
  // The connect-time brief (MCP `instructions`): a short pointer that sends
  // the agent to get_instructions first. The full global AGENT.md comes from
  // that tool (or the client's own import), so it is never loaded twice.
  const brief = init?.result?.instructions ?? ''
  check('initialize points the agent at get_instructions',
    brief.includes('Call get_instructions now') && brief.includes('include_global') && brief.length < 2000, brief.slice(0, 200))
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
  const memoryCaps = JSON.parse(fs.readFileSync(new URL('../modules/memory/module.json', import.meta.url), 'utf8')).capabilities.length
  const memoryTools = bridge.filter((t) => t.name.startsWith('quantsuite.memory.')).length
  check(`quantsuite.memory.* present (${memoryCaps} tools, as modules/memory/module.json declares)`, memoryTools === memoryCaps, `got ${memoryTools}`)

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

  // The agent's folder decides, never the app's selection. This session has
  // not named one yet, so it is refused rather than sent to a guess.
  const noArg = await call('get_codebase_stats', {})
  check('an unknown folder is refused and names the fix',
    noArg.isError && noArg.text.includes('cwd='), noArg.text)

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
  if (modeOf(modeDefault.text) !== 'auto_apply' && coreDb) {
    // A run that died between the approval section and its reset leaves the
    // row behind (cleanup drops it now); carry on from the state this needs.
    setModeAsUser('auto_apply')
  }
  const setByAgent = await call('set_kanban_approval_mode', { workspace: WS_NAME, mode: 'approval' })
  check('set_kanban_approval_mode refused for agents (the user switches it in the UI)',
    setByAgent.isError && setByAgent.text.includes('Only the user changes the board approval mode'), setByAgent.text)

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
  // Agents cannot switch the mode, so this section stands in for the user's
  // switch in the UI by writing the same core.db setting the UI writes. It
  // needs --core-db; without it the section is skipped, not failed.
  if (!coreDb || !b36) {
    console.log("  skip  approval lifecycle needs --core-db <the running instance's core.db>")
  } else {
  // Card B is claimed while the board is still auto_apply (in approval mode a
  // claim needs the user's "Approve work" click, which no agent can give);
  // then the user switches the board to approval mode mid-work.
  const cardB = await mkCard('Test card B (review flow)')
  const cardD = await mkCard('Test card D (needs work approval)')
  const claimB = await call('claim_kanban_card', { card_id: cardB, agent_id: 'test-agent' })
  const worktreeB = claimB.text.match(/Worktree: (.+)/)?.[1]?.trim() ?? null
  check('claim card B', !claimB.isError && !!worktreeB, claimB.text)
  if (!worktreeB) throw new Error('card B has no worktree; the approval section cannot go on')
  fs.writeFileSync(path.join(worktreeB, 'feature_b.md'), '# delivered by test-agent (card B)\n')
  git(worktreeB, 'add', '-A')
  git(worktreeB, 'commit', '-m', 'card B work')

  setModeAsUser('approval')
  const modeNow = await call('get_kanban_approval_mode', { workspace: WS_NAME })
  check('the board reads approval mode once the user switched it', modeOf(modeNow.text) === 'approval', modeNow.text)
  const claimD = await call('claim_kanban_card', { card_id: cardD, agent_id: 'test-agent' })
  check('approval mode: a claim waits for the user\'s "Approve work"', claimD.isError && claimD.text.includes('Start approval required'), claimD.text)

  const doneB = await call('complete_kanban_card', { card_id: cardB })
  check('complete parks in review in approval mode', !doneB.isError && doneB.text.includes('AWAITING_REVIEW'), doneB.text)

  const rejB = await call('reject_kanban_card', { card_id: cardB, reason: 'needs polish' })
  check('reject sends the card back to work, worktree preserved',
    !rejB.isError && rejB.text.includes('IN_PROGRESS') && fs.existsSync(worktreeB), rejB.text)

  const doneB2 = await call('complete_kanban_card', { card_id: cardB })
  check('complete parks it in review again', !doneB2.isError && doneB2.text.includes('AWAITING_REVIEW'), doneB2.text)
  const selfAppr = await call('approve_kanban_card', { card_id: cardB })
  check('an agent cannot approve its own result in approval mode',
    selfAppr.isError && selfAppr.text.includes('Approval mode') && fs.existsSync(worktreeB), selfAppr.text)

  // Back to auto_apply (the user's switch): approve is then the merge retry.
  setModeAsUser('auto_apply')
  const apprB = await call('approve_kanban_card', { card_id: cardB })
  check('approve merges and cleans up', !apprB.isError && apprB.text.includes('MERGED') && fs.existsSync(path.join(WS_DIR, 'feature_b.md')) && !fs.existsSync(worktreeB), apprB.text)
  const modeBack = await call('get_kanban_approval_mode', { workspace: WS_NAME })
  check('approval mode back to auto_apply', modeOf(modeBack.text) === 'auto_apply', modeBack.text)
  }

  // ── kanban: cancel ──
  section('kanban (cancel cleans up)')
  const cardC = await mkCard('Test card C (cancel)')
  const claimC = await call('claim_kanban_card', { card_id: cardC, agent_id: 'test-agent' })
  const worktreeC = claimC.text.match(/Worktree: (.+)/)?.[1]?.trim() ?? null
  const cancC = await call('cancel_kanban_card', { card_id: cardC })
  check('cancel removes worktree and branch', !cancC.isError && cancC.text.includes('CANCELLED') && (!worktreeC || !fs.existsSync(worktreeC)), cancC.text)

  const log = await call('get_kanban_activity_log', { limit: 50 })
  const lifecycle = coreDb && b36 ? ['claimed', 'completed', 'approved', 'cancelled'] : ['claimed', 'completed', 'cancelled']
  check('activity log recorded the lifecycle', lifecycle.every((a) => log.text.includes(a)), log.text.slice(0, 200))

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
  check('approval mode on General refused for agents too', setModeG.isError && setModeG.text.includes('Only the user changes the board approval mode'), setModeG.text)

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

  // ── the folder the agent works in decides ──
  section('working folder')
  {
  const agentA = await agentSession('qs-folder-a')
  const refused = await agentA.call('create_kanban_card', { title: 'never created' })
  check('before it names a folder, a write is refused with the fix', refused.isError && refused.text.includes('cwd='), refused.text)

  const giA = await agentA.call('get_instructions', { cwd: path.join(WS_DIR, 'some', 'subfolder') })
  check('a subfolder of a workspace is that workspace', giA.text.includes(`workspace '${WS_NAME}'`), giA.text.slice(0, 300))
  check('the listing marks this session', giA.text.includes(`${WS_NAME} [this session]`), giA.text.slice(0, 900))
  check('no question for a registered folder', !giA.text.includes('ASK THE USER'))
  const statsA = await agentA.call('get_codebase_stats', {})
  check('a tool without a workspace uses the folder', !statsA.isError, statsA.text.slice(0, 200))
  const cardA = await agentA.call('create_kanban_card', { title: 'folder test A' })
  const cardAId = cardIdIn(cardA.text)
  if (cardAId) cardIds.push(cardAId)
  check('a card without a workspace lands on the folder\'s board', cardA.text.includes(`'${WS_NAME}'`), cardA.text)

  const worktreeSession = await agentSession('qs-folder-wt')
  const giWt = await worktreeSession.call('get_instructions', { cwd: path.join(WS_DIR, '.qs-worktrees', 'abc') })
  check('a card worktree path is its repository', giWt.text.includes(`workspace '${WS_NAME}'`), giWt.text.slice(0, 300))
  const bashStyle = await agentSession('qs-folder-bash')
  if (process.platform === 'win32') {
    const unix = '/' + WS_DIR[0].toLowerCase() + WS_DIR.slice(2).replaceAll('\\', '/')
    const giBash = await bashStyle.call('get_instructions', { cwd: unix })
    check('a Git Bash path is understood', giBash.text.includes(`workspace '${WS_NAME}'`), giBash.text.slice(0, 300))
  }

  const agentB = await agentSession('qs-folder-b')
  const giB = await agentB.call('get_instructions', { cwd: os.tmpdir() })
  check('the temp folder is General without a question', giB.text.includes('works in General') && !giB.text.includes('ASK THE USER'), giB.text.slice(0, 300))
  check('General has no "this session" workspace', !giB.text.includes('[this session]'))
  const cardB = await agentB.call('create_kanban_card', { title: 'folder test B' })
  const cardBId = cardIdIn(cardB.text)
  if (cardBId) cardIds.push(cardBId)
  check('a card in General lands on the General board', cardB.text.includes("'General'"), cardB.text)
  const needsFolder = await agentB.call('get_codebase_stats', {})
  check('a tool that needs a folder explains itself in General', needsFolder.isError && needsFolder.text.includes('General'), needsFolder.text)
  const explicitB = await agentB.call('get_codebase_stats', { workspace: WS_NAME })
  check('an explicit workspace wins in General', !explicitB.isError, explicitB.text.slice(0, 200))
  const explicitA = await agentA.call('list_kanban_cards', { workspace: 'general' })
  check('an explicit "general" wins in a workspace', explicitA.text.includes('General') && !explicitA.isError, explicitA.text.slice(0, 120))
  const cardA2 = await agentA.call('create_kanban_card', { title: 'folder test A2' })
  const cardA2Id = cardIdIn(cardA2.text)
  if (cardA2Id) cardIds.push(cardA2Id)
  check('two agents stay apart', cardA2.text.includes(`'${WS_NAME}'`), cardA2.text)

  if (coreDb) {
    const read = () => coreDb.prepare("SELECT value FROM settings WHERE scope = 'core' AND key = 'workspace.active'").get()?.value
    const before = read()
    coreDb.prepare("INSERT INTO settings (scope, key, value) VALUES ('core', 'workspace.active', ?) ON CONFLICT(scope, key) DO UPDATE SET value = excluded.value")
      .run(JSON.stringify({ path: 'C:/somewhere/selected-by-accident' }))
    const cardA3 = await agentA.call('create_kanban_card', { title: 'folder test A3' })
    const cardA3Id = cardIdIn(cardA3.text)
    if (cardA3Id) cardIds.push(cardA3Id)
    check('changing the selection in the app changes nothing', cardA3.text.includes(`'${WS_NAME}'`), cardA3.text)
    if (before === undefined) coreDb.prepare("DELETE FROM settings WHERE scope = 'core' AND key = 'workspace.active'").run()
    else coreDb.prepare("UPDATE settings SET value = ? WHERE scope = 'core' AND key = 'workspace.active'").run(before)
  } else {
    console.log('  skip  selection test needs --core-db')
  }

  // A folder that is no workspace yet: the question, and both answers.
  const newFolder = path.join(os.homedir(), `qs-mcp-testnew-${process.pid}`)
  fs.mkdirSync(newFolder, { recursive: true })
  const agentC = await agentSession('qs-folder-c')
  const giC = await agentC.call('get_instructions', { cwd: newFolder })
  check('a new folder gets the question', giC.text.includes('ASK THE USER') && giC.text.includes(newFolder), giC.text.slice(0, 400))
  const no = await agentC.call('answer_workspace_question', { folder: newFolder, create: false })
  check('a no is accepted', !no.isError && no.text.includes('not be asked again'), no.text)
  const agentC2 = await agentSession('qs-folder-c2')
  const giC2 = await agentC2.call('get_instructions', { cwd: newFolder })
  check('after a no, a new session is not asked again', giC2.text.includes('works in General') && !giC2.text.includes('ASK THE USER'), giC2.text.slice(0, 300))
  const bad = await agentC2.call('answer_workspace_question', { folder: newFolder })
  check('an answer without create is refused', bad.isError, bad.text)
  const yes = await agentC2.call('answer_workspace_question', { folder: newFolder, create: true })
  check('a yes registers the folder', !yes.isError && yes.text.includes('is a workspace now'), yes.text)
  const giC3 = await agentC2.call('get_instructions', { cwd: newFolder })
  check('after a yes, the session works in it', giC3.text.includes('this session works in workspace'), giC3.text.slice(0, 300))
  const newName = path.basename(newFolder)
  const statsNew = await agentC2.call('get_codebase_stats', {})
  check('a yes does not index the folder', statsNew.text.includes('no code index yet') || statsNew.isError === false, statsNew.text.slice(0, 200))
  if (coreDb) {
    coreDb.prepare("DELETE FROM entities WHERE kind = 'workspace' AND title = ?").run(newName)
    coreDb.prepare("DELETE FROM settings WHERE scope = 'mcp' AND key = 'agentFolders.declined'").run()
  }
  fs.rmSync(newFolder, { recursive: true, force: true })

  // A client that offers roots tells us by itself over its GET stream.
  const agentR = await agentSession('qs-folder-roots', { roots: { listChanged: true } })
  const aborter = new AbortController()
  const stream = await fetch(URL_, { headers: { 'mcp-session-id': agentR.sid, Accept: 'text/event-stream' }, signal: aborter.signal })
  const reader = stream.body.getReader()
  let buffered = ''
  let rootsRequest = null
  const deadline = Date.now() + 5000
  while (!rootsRequest && Date.now() < deadline) {
    const { value, done } = await reader.read()
    if (done) break
    buffered += new TextDecoder().decode(value)
    const line = buffered.split('\n').find((l) => l.startsWith('data:') && l.includes('roots/list'))
    if (line) rootsRequest = JSON.parse(line.slice(5))
  }
  check('the server asks a client that offers roots', rootsRequest?.method === 'roots/list', buffered.slice(0, 200))
  if (rootsRequest) {
    const uri = 'file:///' + WS_DIR.replaceAll('\\', '/').replace(/^\//, '')
    const accepted = await agentR.post({ jsonrpc: '2.0', id: rootsRequest.id, result: { roots: [{ uri, name: 'x' }] } })
    check('a roots answer is accepted', accepted === null)
    const giR = await agentR.call('get_instructions', {})
    check('a client\'s root decides without cwd', giR.text.includes(`workspace '${WS_NAME}'`), giR.text.slice(0, 300))
    const giR2 = await agentR.call('get_instructions', { cwd: os.tmpdir() })
    check('the agent\'s own cwd outranks its roots', giR2.text.includes('works in General'), giR2.text.slice(0, 300))
  }
  aborter.abort()
  }

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
