/**
 * QuantScript's workbench (docs/PLAN-QUANTSCRIPT.md): the listing the
 * registry gives of every script, the scripts open in the editor with their
 * buffers, diagnostics, checks, versions and compares, and the chrome state.
 *
 * The store owns the buffers; `QCodeEditor` holds a Monaco model per path and
 * echoes what it is given. A save goes through the crate, which checks the
 * candidate in a sandbox copy of the package before it writes — a script that
 * would not import comes back unsaved with its check attached. While the
 * user types, a debounced lint (syntax, the contract's shape; no import)
 * feeds the editor's squiggles.
 */
import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { computed, ref, watch } from 'vue'
import type {
  ArchivedScript,
  CheckDepth,
  CheckResult,
  LintResult,
  NewScript,
  SaveResult,
  ScriptDocument,
  ScriptEntry,
  ScriptKind,
  ScriptListing,
  VersionMeta,
} from '#script/types'

export interface OpenScript {
  file: string
  path: string
  kind: ScriptKind
  editable: boolean
  /** The buffer the editor shows. */
  content: string
  /** What the file held when it was last read or saved; dirty = content !== saved. */
  saved: string
  sha256: string
  version: VersionMeta | null
  versions: VersionMeta[]
  versionsLoaded: boolean
  check: CheckResult | null
  /** The exact buffer described by `check`; edits make that result stale. */
  checkContent: string | null
  checking: CheckDepth | null
  saving: boolean
  /** The editor's diagnostics for the buffer (syntax, contract shape). */
  lint: LintResult | null
  linting: boolean
  lintError: string | null
  /** A recorded version opened for comparison with the buffer. */
  compare: { meta: VersionMeta; content: string } | null
}

export interface Notice {
  text: string
  tone: 'ok' | 'warn' | 'error'
}

const LINT_DEBOUNCE_MS = 600

function readFiles(key: string): string[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(key) ?? '[]')
    return Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : []
  } catch { return [] }
}

function readBool(key: string, fallback: boolean): boolean {
  try {
    const raw = localStorage.getItem(key)
    return raw === null ? fallback : raw === '1'
  } catch {
    return fallback
  }
}

function writeBool(key: string, value: boolean) {
  try {
    localStorage.setItem(key, value ? '1' : '0')
  } catch {
    /* private mode */
  }
}

export const useWorkbenchStore = defineStore('script/workbench', () => {
  // ── The listing ──
  const listing = ref<ScriptListing | null>(null)
  const listingLoading = ref(false)
  const listingError = ref<string | null>(null)

  // ── Open scripts ──
  const open = ref<OpenScript[]>([])
  const activeFile = ref<string | null>(null)

  // ── Chrome ──
  const search = ref('')
  const libraryOpen = ref(true)
  const libraryFilter = ref<'all' | 'custom' | 'favorites' | 'attention'>('all')
  const favorites = ref(readFiles('qsc-favorites'))
  const recentFiles = ref(readFiles('qsc-recent'))
  const inspectorTab = ref<'outline' | 'details' | 'history' | 'guide'>('outline')
  const focusMode = ref(false)
  const minimap = ref(readBool('qsc-minimap', false))
  const wordWrap = ref(readBool('qsc-wrap', false))
  const resultsTab = ref<'problems' | 'check'>('problems')
  const openingFile = ref<string | null>(null)
  const expanded = ref<Record<string, boolean>>({})
  const archiveOpen = ref(false)
  const problemsOpen = ref(false)
  const sidebarLeftOpen = ref(readBool('qsc-sidebar-left', true))
  // The inspector now belongs to the library as well as the editor. Start
  // visible; keep subsequent choices independent of the former hidden panel.
  const sidebarRightOpen = ref(readBool('qsc-inspector-open', true))
  const newScriptOpen = ref(false)
  const notice = ref<Notice | null>(null)
  const cursor = ref({ line: 1, column: 1 })
  /** A one-shot "show this line" request the editor page consumes. */
  const revealRequest = ref<{ file: string; line: number; nonce: number } | null>(null)

  // ── Getters ──
  const scripts = computed<ScriptEntry[]>(() => listing.value?.scripts ?? [])
  const reference = computed<ScriptEntry[]>(() => listing.value?.reference ?? [])
  const archived = computed<ArchivedScript[]>(() => listing.value?.archived ?? [])
  const python = computed(() => listing.value?.python ?? null)
  const active = computed<OpenScript | null>(() => open.value.find((o) => o.file === activeFile.value) ?? null)
  const activeEntry = computed<ScriptEntry | null>(() => entryFor(activeFile.value))
  const indicatorCount = computed(() => scripts.value.reduce((n, s) => n + s.registered, 0))
  const certifiedCount = computed(() => listing.value?.certified.length ?? 0)
  const dirtyFiles = computed(() => open.value.filter((o) => o.content !== o.saved).map((o) => o.file))
  const checkStale = computed(() => !!active.value?.check && active.value.checkContent !== active.value.content)
  const libraryScripts = computed(() => scripts.value.filter((s) => s.kind === 'script'))
  const recentScripts = computed(() => recentFiles.value.map((file) => entryFor(file)).filter((s): s is ScriptEntry => !!s).slice(0, 4))
  const filterCounts = computed(() => ({
    all: libraryScripts.value.length,
    custom: libraryScripts.value.filter((s) => s.registration === 'discovered').length,
    favorites: libraryScripts.value.filter((s) => favorites.value.includes(s.file)).length,
    attention: libraryScripts.value.filter(needsAttention).length,
  }))
  const filtered = computed<ScriptEntry[]>(() => {
    const q = search.value.trim().toLowerCase()
    return scripts.value.filter(
      (s) =>
        (!q || s.file.toLowerCase().includes(q) || s.summary.toLowerCase().includes(q) ||
        s.classes.some(
          (c) =>
            (c.key ?? '').toLowerCase().includes(q) ||
            c.class_name.toLowerCase().includes(q) ||
            (c.name ?? '').toLowerCase().includes(q),
        )) && (libraryFilter.value === 'all' ||
          (libraryFilter.value === 'custom' && s.registration === 'discovered') ||
          (libraryFilter.value === 'favorites' && favorites.value.includes(s.file)) ||
          (libraryFilter.value === 'attention' && needsAttention(s))),
    )
  })

  function needsAttention(s: ScriptEntry): boolean {
    return !!(s.syntax_error || s.discovery_error || s.versions.external || isDirty(s.file))
  }

  watch(favorites, (value) => { try { localStorage.setItem('qsc-favorites', JSON.stringify(value)) } catch { /* private mode */ } })
  watch(recentFiles, (value) => { try { localStorage.setItem('qsc-recent', JSON.stringify(value)) } catch { /* private mode */ } })
  watch(minimap, (value) => writeBool('qsc-minimap', value))
  watch(wordWrap, (value) => writeBool('qsc-wrap', value))
  watch(sidebarLeftOpen, (value) => writeBool('qsc-sidebar-left', value))
  watch(sidebarRightOpen, (value) => writeBool('qsc-inspector-open', value))

  function toggleFavorite(file: string) {
    favorites.value = favorites.value.includes(file) ? favorites.value.filter((f) => f !== file) : [...favorites.value, file]
  }

  function showLibrary() {
    libraryOpen.value = true
    focusMode.value = false
    openingFile.value = null
    openIntent++
  }

  function showInspector(tab: typeof inspectorTab.value) {
    inspectorTab.value = tab
    sidebarRightOpen.value = true
    focusMode.value = false
  }

  function showResults(tab: typeof resultsTab.value) {
    resultsTab.value = tab
    problemsOpen.value = true
  }

  function entryFor(file: string | null): ScriptEntry | null {
    if (!file) return null
    return scripts.value.find((s) => s.file === file) ?? reference.value.find((s) => s.file === file) ?? null
  }

  /** Where a registry key is defined: its file and class line. */
  function locationOf(key: string): { file: string; line: number } | null {
    for (const s of scripts.value) {
      const c = s.classes.find((cl) => cl.key === key)
      if (c) return { file: s.file, line: c.line }
    }
    return null
  }

  function isDirty(file: string): boolean {
    const o = open.value.find((s) => s.file === file)
    return !!o && o.content !== o.saved
  }

  /** A script the user may delete: made here (REGISTER) or registering nothing. */
  function deletable(entry: ScriptEntry | null): boolean {
    return !!entry && entry.kind === 'script' && (entry.registration === 'discovered' || entry.registration === 'none')
  }

  let noticeTimer: ReturnType<typeof setTimeout> | null = null
  function setNotice(text: string, tone: Notice['tone'] = 'ok', ttl = 5000) {
    notice.value = { text, tone }
    if (noticeTimer) clearTimeout(noticeTimer)
    noticeTimer = setTimeout(() => {
      notice.value = null
      noticeTimer = null
    }, ttl)
  }

  // ── Listing ──
  async function loadListing(refresh = false) {
    listingLoading.value = true
    listingError.value = null
    try {
      // The workbench wants the full registry view; agents get the compact one.
      listing.value = await invoke<ScriptListing>('plugin:script|list_scripts', { refresh, compact: false })
    } catch (err) {
      listingError.value = String(err)
      console.error('[script] Failed to list the scripts:', err)
    } finally {
      listingLoading.value = false
    }
  }

  // ── Open / close / switch ──
  let openIntent = 0
  const pendingOpens = new Map<string, Promise<ScriptDocument>>()
  async function openScript(file: string, line?: number): Promise<OpenScript | null> {
    const intent = ++openIntent
    const existing = open.value.find((o) => o.file === file)
    if (existing) {
      activate(file)
      if (line) reveal(file, line)
      return existing
    }
    openingFile.value = file
    try {
      let pending = pendingOpens.get(file)
      if (!pending) {
        pending = invoke<ScriptDocument>('plugin:script|read_script', { file })
        pendingOpens.set(file, pending)
      }
      const doc = await pending
      const alreadyOpened = open.value.find((o) => o.file === file)
      if (alreadyOpened) {
        if (intent === openIntent) {
          activate(file)
          if (line) reveal(file, line)
        }
        return alreadyOpened
      }
      const script: OpenScript = {
        file: doc.file,
        path: doc.path,
        kind: doc.kind,
        editable: doc.editable,
        content: doc.content,
        saved: doc.content,
        sha256: doc.sha256,
        version: doc.version,
        versions: [],
        versionsLoaded: false,
        check: null,
        checkContent: null,
        checking: null,
        saving: false,
        lint: null,
        linting: false,
        lintError: null,
        compare: null,
      }
      open.value = [...open.value, script]
      const selected = intent === openIntent
      if (selected) activate(file)
      if (doc.recorded === 'external' && doc.version) {
        setNotice(`${file} changed outside QuantScript — recorded as v${doc.version.version}.`, 'warn', 7000)
        void loadListing(false)
      }
      if (line && selected) reveal(file, line)
      if (script.editable) scheduleLint(file, 0)
      return script
    } catch (err) {
      setNotice(String(err), 'error', 8000)
      console.error('[script] Failed to open a script:', err)
      return null
    } finally {
      pendingOpens.delete(file)
      if (intent === openIntent) openingFile.value = null
    }
  }

  function activate(file: string) {
    if (open.value.some((o) => o.file === file)) {
      if (activeFile.value !== file) {
        cursor.value = { line: 1, column: 1 }
        problemsOpen.value = false
      }
      openIntent++
      openingFile.value = null
      activeFile.value = file
      libraryOpen.value = false
      recentFiles.value = [file, ...recentFiles.value.filter((f) => f !== file)].slice(0, 12)
    }
  }

  /** Close a tab. Returns false when the buffer is dirty and `force` is not set. */
  function closeScript(file: string, force = false): boolean {
    const o = open.value.find((s) => s.file === file)
    if (!o) return true
    if (o.saving || o.checking) return false
    if (!force && o.content !== o.saved) return false
    cancelLint(file)
    const index = open.value.indexOf(o)
    open.value = open.value.filter((s) => s.file !== file)
    if (activeFile.value === file) {
      const next = open.value[Math.min(index, open.value.length - 1)]
      if (next && !libraryOpen.value) activate(next.file)
      else activeFile.value = next?.file ?? null
      if (!next) { libraryOpen.value = true; cursor.value = { line: 1, column: 1 } }
    }
    return true
  }

  function setContent(file: string, text: string) {
    const o = open.value.find((s) => s.file === file)
    if (o && o.content !== text) {
      o.content = text
      o.lint = null
      o.lintError = null
      scheduleLint(file)
    }
  }

  function reveal(file: string, line: number) {
    const o = open.value.find((s) => s.file === file)
    if (o) o.compare = null
    libraryOpen.value = false
    revealRequest.value = { file, line, nonce: Date.now() }
  }

  // ── Lint (the editor's diagnostics) ──
  const lintTimers = new Map<string, ReturnType<typeof setTimeout>>()
  const lintRuns = new WeakMap<OpenScript, number>()

  function cancelLint(file: string) {
    const t = lintTimers.get(file)
    if (t) {
      clearTimeout(t)
      lintTimers.delete(file)
    }
  }

  function scheduleLint(file: string, delay = LINT_DEBOUNCE_MS) {
    if (!python.value?.ok && listing.value) return
    cancelLint(file)
    lintTimers.set(
      file,
      setTimeout(() => {
        lintTimers.delete(file)
        void lint(file)
      }, delay),
    )
  }

  async function lint(file: string) {
    const o = open.value.find((s) => s.file === file)
    if (!o || !o.editable) return
    const snapshot = o.content
    const run = (lintRuns.get(o) ?? 0) + 1
    lintRuns.set(o, run)
    o.linting = true
    o.lintError = null
    try {
      const result = await invoke<LintResult>('plugin:script|lint_script', { file, content: snapshot })
      // The buffer moved on while the interpreter ran: the result describes
      // old text, and a newer lint is already scheduled.
      if (o.content === snapshot && lintRuns.get(o) === run) o.lint = result
    } catch (err) {
      if (o.content === snapshot && lintRuns.get(o) === run) o.lintError = String(err)
      console.error('[script] Lint failed:', err)
    } finally {
      if (lintRuns.get(o) === run) o.linting = false
    }
  }

  // ── Check and save ──
  async function check(file = activeFile.value, depth: CheckDepth = 'full'): Promise<CheckResult | null> {
    const o = file ? open.value.find((s) => s.file === file) : null
    if (!o || !o.editable || o.checking || o.saving || !python.value?.ok) return null
    const snapshot = o.content
    o.checking = depth
    try {
      const result = await invoke<CheckResult>('plugin:script|check_script', { file: o.file, content: snapshot, depth })
      o.check = result
      o.checkContent = snapshot
      if (activeFile.value === o.file) showResults('check')
      if (o.content !== snapshot) setNotice(`${o.file}: check finished for an earlier edit. Run again to check the current code.`, 'warn')
      else if (result.ok) setNotice(`${o.file}: check passed (${depth}, ${result.elapsed_s ?? '?'}s).`, 'ok')
      else if (result.blocking) setNotice(`${o.file}: the script would not load — see the check.`, 'error', 7000)
      else setNotice(`${o.file}: a law failed — see the check.`, 'warn', 7000)
      return result
    } catch (err) {
      setNotice(String(err), 'error', 8000)
      console.error('[script] Check failed:', err)
      return null
    } finally {
      o.checking = null
    }
  }

  async function save(file = activeFile.value, message?: string): Promise<SaveResult | null> {
    const o = file ? open.value.find((s) => s.file === file) : null
    if (!o || !o.editable || o.saving || o.checking) return null
    // The user may keep typing while the sandbox runs: what gets written is
    // this snapshot, and only this snapshot counts as saved afterwards.
    const snapshot = o.content
    o.saving = true
    try {
      const result = await invoke<SaveResult>('plugin:script|save_script', {
        file: o.file,
        content: snapshot,
        message: message ?? null,
      })
      if (result.check) {
        o.check = result.check
        o.checkContent = snapshot
        if (!result.saved && activeFile.value === o.file) showResults('check')
      }
      if (result.saved && result.version) {
        o.saved = snapshot
        o.sha256 = result.sha256
        o.version = result.version
        if (o.versionsLoaded) o.versions = [result.version, ...o.versions]
        if (o.compare) o.compare = null
        setNotice(
          result.checked
            ? `${o.file} saved as v${result.version.version}.`
            : `${o.file} saved as v${result.version.version} — ${result.note ?? 'unchecked'}`,
          result.checked ? 'ok' : 'warn',
        )
        void loadListing(false)
      } else if (result.unchanged) {
        o.saved = snapshot
        setNotice(result.note ?? 'Nothing to save.', 'ok')
      } else {
        setNotice(result.note ?? `${o.file} was not saved.`, 'error', 8000)
      }
      return result
    } catch (err) {
      setNotice(String(err), 'error', 8000)
      console.error('[script] Save failed:', err)
      return null
    } finally {
      o.saving = false
    }
  }

  // ── Versions ──
  async function loadVersions(file: string) {
    const o = open.value.find((s) => s.file === file)
    if (!o || !o.editable) return
    try {
      o.versions = await invoke<VersionMeta[]>('plugin:script|list_versions', { file })
      o.versionsLoaded = true
    } catch (err) {
      console.error('[script] Failed to list versions:', err)
    }
  }

  async function compare(file: string, version: number) {
    const o = open.value.find((s) => s.file === file)
    if (!o) return
    try {
      const doc = await invoke<{ meta: VersionMeta; content: string }>('plugin:script|read_version', { file, version })
      o.compare = doc
      activate(file)
    } catch (err) {
      setNotice(String(err), 'error', 8000)
    }
  }

  function closeCompare(file: string) {
    const o = open.value.find((s) => s.file === file)
    if (o) o.compare = null
  }

  async function restore(file: string, version: number): Promise<boolean> {
    const o = open.value.find((s) => s.file === file)
    if (!o || !o.editable || o.saving || o.checking) return false
    const snapshot = o.content
    o.saving = true
    try {
      const doc = await invoke<{ meta: VersionMeta; content: string }>('plugin:script|read_version', { file, version })
      const result = await invoke<SaveResult>('plugin:script|restore_version', { file, version })
      if (result.check) { o.check = result.check; o.checkContent = doc.content }
      if ((result.saved && result.version) || result.unchanged) {
        const editedDuringRestore = o.content !== snapshot
        if (!editedDuringRestore) o.content = doc.content
        o.saved = doc.content
        o.sha256 = result.sha256
        if (result.version) o.version = result.version
        o.compare = null
        await loadVersions(file)
        setNotice(editedDuringRestore
          ? `${file}: version restored on disk. Your newer editor changes are kept unsaved.`
          : `${file}: v${version} restored${result.version ? ` as v${result.version.version}` : ''}.`, editedDuringRestore ? 'warn' : 'ok')
        void loadListing(false)
        scheduleLint(file, 0)
        return true
      }
      showResults('check')
      setNotice(result.note ?? `v${version} could not be restored.`, 'error', 8000)
      return false
    } catch (err) {
      setNotice(String(err), 'error', 8000)
      return false
    } finally {
      o.saving = false
    }
  }

  /** Bring a deleted script back: its last recorded version becomes the file again. */
  async function restoreArchived(entry: ArchivedScript): Promise<boolean> {
    if (entry.latest == null) return false
    try {
      const result = await invoke<SaveResult>('plugin:script|restore_version', { file: entry.file, version: entry.latest })
      if (result.saved && result.version) {
        await loadListing(false)
        setNotice(`${entry.file} is back as v${result.version.version}.`, 'ok')
        await openScript(entry.file)
        return true
      }
      setNotice(result.note ?? `${entry.file} could not be restored — its last version would not load.`, 'error', 9000)
      return false
    } catch (err) {
      setNotice(String(err), 'error', 8000)
      return false
    }
  }

  // ── New and deleted scripts ──
  async function create(request: NewScript): Promise<string | null> {
    try {
      const created = await invoke<{ file: string }>('plugin:script|create_script', {
        key: request.key,
        class_name: request.class_name,
        name: request.name ?? null,
      })
      await loadListing(false)
      newScriptOpen.value = false
      await openScript(created.file)
      setNotice(`${created.file} created and registered as "${request.key}".`, 'ok')
      return created.file
    } catch (err) {
      setNotice(String(err), 'error', 9000)
      return null
    }
  }

  /** Delete a script the user made; its content stays in the archive. */
  async function deleteScript(file: string): Promise<boolean> {
    const o = open.value.find((s) => s.file === file)
    if (o?.saving || o?.checking) return false
    try {
      const result = await invoke<{ file: string; keys: string[]; version: VersionMeta }>('plugin:script|delete_script', { file })
      closeScript(file, true)
      await loadListing(false)
      const keys = result.keys.length ? ` (${result.keys.join(', ')})` : ''
      setNotice(`${file} deleted${keys} — kept in the archive as v${result.version.version}.`, 'ok', 7000)
      return true
    } catch (err) {
      setNotice(String(err), 'error', 9000)
      return false
    }
  }

  // ── Chrome ──
  function toggleExpanded(file: string) {
    expanded.value = { ...expanded.value, [file]: !expanded.value[file] }
  }

  function toggleSidebar(side: 'left' | 'right') {
    if (focusMode.value) {
      focusMode.value = false
      if (side === 'right') sidebarRightOpen.value = true
      else sidebarLeftOpen.value = true
      return
    }
    if (side === 'right') {
      sidebarRightOpen.value = !sidebarRightOpen.value
    } else {
      sidebarLeftOpen.value = !sidebarLeftOpen.value
    }
  }

  return {
    listing,
    listingLoading,
    listingError,
    open,
    activeFile,
    search,
    libraryOpen,
    libraryFilter,
    libraryScripts,
    favorites,
    recentFiles,
    recentScripts,
    filterCounts,
    inspectorTab,
    focusMode,
    minimap,
    wordWrap,
    resultsTab,
    openingFile,
    checkStale,
    needsAttention,
    toggleFavorite,
    showLibrary,
    showInspector,
    showResults,
    expanded,
    archiveOpen,
    problemsOpen,
    sidebarLeftOpen,
    sidebarRightOpen,
    newScriptOpen,
    notice,
    cursor,
    revealRequest,
    scripts,
    reference,
    archived,
    python,
    active,
    activeEntry,
    indicatorCount,
    certifiedCount,
    dirtyFiles,
    filtered,
    entryFor,
    locationOf,
    isDirty,
    deletable,
    setNotice,
    loadListing,
    openScript,
    activate,
    closeScript,
    setContent,
    reveal,
    lint,
    check,
    save,
    loadVersions,
    compare,
    closeCompare,
    restore,
    restoreArchived,
    create,
    deleteScript,
    toggleExpanded,
    toggleSidebar,
  }
})
