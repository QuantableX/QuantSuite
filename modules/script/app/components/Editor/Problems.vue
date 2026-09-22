<script setup lang="ts">
/**
 * The check panel under the editor: what the sandbox said about the buffer
 * — the syntax error with its line, the import that failed, the script that
 * cannot register, and every registered indicator with the contract's laws.
 */
import { useWorkbenchStore } from '#script/stores/workbench'
import { ChevronDown, ChevronUp, CircleAlert, CircleCheck } from 'lucide-vue-next'
import type { CheckIndicator, CheckLaw } from '#script/types'

const wb = useWorkbenchStore()

const check = computed(() => wb.active?.check ?? null)
const markers = computed(() => wb.active?.lint?.markers ?? [])
function toggle(tab: 'problems' | 'check') {
  if (wb.resultsTab === tab) wb.problemsOpen = !wb.problemsOpen
  else wb.showResults(tab)
}
const stem = computed(() => wb.active?.file.replace(/\.py$/, '') ?? '')

const headline = computed(() => {
  const c = check.value
  if (!c) return ''
  if (wb.checkStale) return 'Code changed since this check — run again'
  if (!c.syntax.ok) return 'Syntax error'
  if (!c.import.ok) return 'The script would not import'
  if (c.discovery_errors[stem.value]) return 'The script cannot register'
  if (c.ok) return 'Check passed'
  return 'A law failed'
})

const tone = computed(() => {
  const c = check.value
  if (!c) return ''
  if (wb.checkStale) return 'is-warn'
  if (c.blocking) return 'is-error'
  return c.ok ? 'is-ok' : 'is-warn'
})

const LAW_LABELS: Record<CheckLaw['law'], string> = {
  contract: 'Contract',
  causality: 'Causality',
  scale: 'Scale invariance',
}

function lawClass(law: CheckLaw): string {
  if (!law.ok) return 'is-fail'
  return law.warning ? 'is-warn' : 'is-ok'
}

function rowClass(row: CheckIndicator): string {
  if (!row.ok) return 'is-fail'
  return row.checks.some((c) => c.warning) ? 'is-warn' : 'is-ok'
}

function jump(line: number | null) {
  if (line && wb.active) wb.reveal(wb.active.file, line)
}
</script>

<template>
  <section class="qsc-problems" :class="tone" aria-label="Script diagnostics">
    <header class="qsc-problems-head">
      <button class="qsc-result-tab" :class="{ 'is-active': wb.resultsTab === 'problems' && wb.problemsOpen }" :aria-expanded="wb.problemsOpen && wb.resultsTab === 'problems'" @click="toggle('problems')"><CircleAlert :size="13" /> Problems <span v-if="markers.length" class="qsc-chip">{{ markers.length }}</span></button>
      <button class="qsc-result-tab" :class="{ 'is-active': wb.resultsTab === 'check' && wb.problemsOpen }" :aria-expanded="wb.problemsOpen && wb.resultsTab === 'check'" @click="toggle('check')"><CircleCheck :size="13" /> Check results<span v-if="wb.checkStale" class="qsc-chip is-warn">outdated</span></button>
      <span class="qsc-problems-spacer" />
      <button class="qsc-icon-btn" :aria-label="wb.problemsOpen ? 'Collapse results' : 'Expand results'" @click="wb.problemsOpen = !wb.problemsOpen"><component :is="wb.problemsOpen ? ChevronDown : ChevronUp" :size="14" /></button>
    </header>

    <div v-if="wb.problemsOpen && wb.resultsTab === 'problems'" class="qsc-problems-body">
      <p v-if="wb.active?.lintError" class="qsc-note is-warn">Live diagnostics unavailable. {{ wb.active.lintError }}</p>
      <p v-else-if="!wb.python?.ok" class="muted">Python is unavailable. Configure the interpreter in Settings to enable diagnostics.</p>
      <p v-else-if="wb.active?.linting || !wb.active?.lint" class="muted qsc-pulse">Checking syntax…</p>
      <p v-else-if="!markers.length" class="qsc-clean"><CircleCheck :size="16" /> No syntax problems. Run a full check to validate the indicator contract.</p>
      <button v-for="(m, i) in markers" :key="i" class="qsc-diagnostic" :class="m.severity === 'error' ? 'qsc-err' : 'qsc-warn'" @click="jump(m.line)"><CircleAlert :size="13" /><span>{{ m.message }}</span><span class="mono muted">Ln {{ m.line }}:{{ m.column }}</span></button>
    </div>
    <div v-if="wb.problemsOpen && wb.resultsTab === 'check' && !check" class="qsc-problems-body qsc-check-empty">
      <p>Test syntax, imports and the indicator contract before saving. A full check also tests causality and scale invariance.</p>
      <div><button class="qsc-btn is-sm" :disabled="!!wb.active?.checking || !!wb.active?.saving || !wb.python?.ok" @click="wb.check(undefined, 'quick')">Quick check</button><button class="qsc-btn is-sm" :disabled="!!wb.active?.checking || !!wb.active?.saving || !wb.python?.ok" @click="wb.check()">Full check · Ctrl+Enter</button></div>
    </div>
    <div v-if="wb.problemsOpen && wb.resultsTab === 'check' && check" class="qsc-problems-body">
      <div class="qsc-check-summary"><span class="qsc-problems-title">{{ headline }}</span><span class="muted">{{ check.depth }} · {{ check.elapsed_s ?? '?' }}s</span><button class="qsc-btn is-sm is-ghost" :disabled="!!wb.active?.checking || !!wb.active?.saving || !wb.python?.ok" @click="wb.check()">{{ wb.active?.checking ? 'Checking…' : 'Run again' }}</button></div>
      <p v-if="check.depth === 'quick'" class="muted">Quick check: syntax, imports and contract. Run a full check for causality and scale invariance.</p>
      <div v-if="!check.syntax.ok" class="qsc-problem is-fail">
        <button class="qsc-link mono" @click="jump(check.syntax.line)">line {{ check.syntax.line ?? '?' }}:{{ check.syntax.column ?? '?' }}</button>
        <span>{{ check.syntax.message }}</span>
        <pre v-if="check.syntax.text" class="qsc-pre">{{ check.syntax.text }}</pre>
      </div>

      <div v-if="!check.import.ok" class="qsc-problem is-fail">
        <span>{{ check.import.message }}</span>
        <pre v-if="check.import.traceback" class="qsc-pre">{{ check.import.traceback }}</pre>
      </div>

      <div v-for="(message, file) in check.discovery_errors" :key="file" class="qsc-problem" :class="file === stem ? 'is-fail' : 'is-warn'">
        <span class="mono">{{ file }}.py</span>
        <span>{{ message }}</span>
      </div>

      <table v-if="check.indicators.length" class="qsc-laws">
        <tbody>
          <tr v-for="row in check.indicators" :key="row.key" :class="rowClass(row)">
            <td class="qsc-laws-key">
              <span class="mono">{{ row.key }}</span>
              <span class="muted">{{ row.name }}</span>
            </td>
            <td class="qsc-laws-checks">
              <span v-for="law in row.checks" :key="law.law" class="qsc-law" :class="lawClass(law)" :title="law.message">
                <span class="qsc-law-mark">{{ law.ok ? (law.warning ? '!' : '✓') : '✗' }}</span>
                {{ LAW_LABELS[law.law] }}
              </span>
              <span v-if="row.error" class="qsc-law is-fail" :title="row.traceback ?? row.error">
                <span class="qsc-law-mark">✗</span>{{ row.error }}
              </span>
              <span v-if="row.committed_at != null" class="qsc-law-note muted">
                commits at bar {{ row.committed_at }} of {{ row.bars }} · warm-up {{ row.warmup_bars }}
              </span>
            </td>
            <td class="qsc-laws-time mono muted">{{ row.elapsed_s != null ? `${row.elapsed_s}s` : '' }}</td>
          </tr>
        </tbody>
      </table>
      <p v-else-if="check.import.ok && check.syntax.ok && !check.discovery_errors[stem]" class="qsc-problems-empty muted">
        {{ wb.active?.kind === 'registry' ? 'The registry imports.' : 'No registered indicator in this file — it imports.' }}
      </p>

      <details v-if="check.stderr" class="qsc-stderr">
        <summary class="muted">stderr</summary>
        <pre class="qsc-pre">{{ check.stderr }}</pre>
      </details>
    </div>
  </section>
</template>

<style scoped>
.qsc-problems {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  max-height: min(300px, 42%);
  border-top: 1px solid var(--qss-border);
  background: var(--qss-bg-raised);
}
.qsc-result-tab { height: 100%; display: inline-flex; align-items: center; gap: 7px; border: none; border-bottom: 2px solid transparent; background: none; font-size: 11px; cursor: pointer; color: var(--qss-text-muted); }
.qsc-result-tab.is-active { border-bottom-color: var(--qss-text-secondary); color: var(--qss-text); }
.qsc-check-summary { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
.qsc-check-summary button { margin-left: auto; }
.qsc-clean { display: flex; gap: 9px; align-items: center; color: var(--qss-text-secondary); padding: 8px 0; }
.qsc-clean svg { color: var(--qss-success); flex-shrink: 0; }
.qsc-check-empty { color: var(--qss-text-secondary); }
.qsc-check-empty > div { display: flex; gap: 8px; }
.qsc-diagnostic { display: flex; align-items: center; gap: 8px; text-align: left; padding: 6px; border: none; background: transparent; cursor: pointer; font-size: 12px; }
.qsc-diagnostic > span:first-of-type { flex: 1; }
.qsc-diagnostic:hover { background: var(--qss-bg-hover); }
.qsc-diagnostic svg { flex-shrink: 0; }
.qsc-problems-head {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 32px;
  padding: 0 8px 0 12px;
  border-bottom: 1px solid var(--qss-border-subtle);
  flex-shrink: 0;
}
.qsc-problems-title {
  font-size: 12px;
  font-weight: 600;
}
.qsc-problems.is-error .qsc-problems-title { color: var(--qss-error); }
.qsc-problems.is-warn .qsc-problems-title { color: var(--qss-warning); }
.qsc-problems.is-ok .qsc-problems-title { color: var(--qss-success); }
.qsc-problems-spacer {
  flex: 1;
}

.qsc-problems-body {
  overflow: auto;
  padding: 8px 12px 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  font-size: 12px;
}
.qsc-problem {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 8px;
  color: var(--qss-text-secondary);
}
.qsc-problem.is-fail {
  color: var(--qss-error);
}
.qsc-problem.is-warn {
  color: var(--qss-warning);
}
.qsc-problem .qsc-pre {
  flex-basis: 100%;
}
.qsc-problems-empty {
  font-size: 12px;
}

.qsc-laws {
  border-collapse: collapse;
  width: 100%;
}
.qsc-laws td {
  padding: 4px 8px 4px 0;
  vertical-align: top;
  border-top: 1px solid var(--qss-border-subtle);
}
.qsc-laws tr:first-child td {
  border-top: none;
}
.qsc-laws-key {
  width: 25%;
  display: table-cell;
}
.qsc-laws-key > span { display: block; overflow-wrap: anywhere; }
.qsc-laws-key .mono {
  font-weight: 600;
  color: var(--qss-text);
}
.qsc-laws-key .muted {
  font-size: 11px;
}
.qsc-laws-checks {
  display: table-cell;
}
.qsc-law {
  margin: 2px 10px 4px 0;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: var(--qss-text-secondary);
}
.qsc-law-mark {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 700;
  background: var(--qss-bg-card);
}
.qsc-law.is-ok .qsc-law-mark { color: var(--qss-success); }
.qsc-law.is-warn .qsc-law-mark { color: var(--qss-warning); }
.qsc-law.is-fail { color: var(--qss-error); }
.qsc-law.is-fail .qsc-law-mark { color: var(--qss-error); }
.qsc-law-note {
  display: block;
  font-size: 11px;
}
.qsc-laws-time {
  width: 48px;
  text-align: right;
  font-size: 11px;
}

.qsc-stderr summary {
  cursor: pointer;
  font-size: 11px;
}
</style>
