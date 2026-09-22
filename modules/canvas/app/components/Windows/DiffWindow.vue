<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import type { CanvasWindow, FileDiff, DiffHunk, GitStatus, GitStatusFile } from '../../../shared/types'
import { useWorkspacesStore } from '../../../stores/workspaces'
import { parseUnifiedDiff, resetHunkIdCounter } from '../../../lib/diff/parser'

const props = defineProps<{
  window: CanvasWindow
}>()

const workspacesStore = useWorkspacesStore()

const viewMode = ref<'inline' | 'side-by-side'>('inline')
const activeFileIndex = ref(0)
const diffScope = ref<'all' | 'staged' | 'unstaged'>('all')
const gitDiffs = ref<FileDiff[]>([])
const loading = ref(false)
const gitBranch = ref('')
const errorMsg = ref('')

const fileDiffs = computed<FileDiff[]>(() => gitDiffs.value)

const activeFile = computed(() => fileDiffs.value[activeFileIndex.value])

function filterStatusFiles(files: GitStatusFile[], scope: 'all' | 'staged' | 'unstaged'): GitStatusFile[] {
  if (scope === 'staged') return files.filter(f => f.staged)
  if (scope === 'unstaged') return files.filter(f => !f.staged)
  // 'all': deduplicate by path (a file can appear twice — once staged, once unstaged)
  const seen = new Set<string>()
  return files.filter(f => {
    if (seen.has(f.path)) return false
    seen.add(f.path)
    return true
  })
}

async function loadGitDiff() {
  const workspace = workspacesStore.contentWorkspace
  if (!workspace) {
    errorMsg.value = 'No workspace open'
    return
  }

  loading.value = true
  errorMsg.value = ''

  try {
    const [diffText, statusResult] = await Promise.all([
      invoke<string>('plugin:canvas|git_diff', {
        repoPath: workspace.path,
        mode: diffScope.value,
      }),
      invoke<GitStatus>('plugin:canvas|git_status', {
        repoPath: workspace.path,
      }),
    ])

    gitBranch.value = statusResult.branch
    resetHunkIdCounter()

    // Parse diff content
    const parsedDiffs = parseUnifiedDiff(diffText)

    // Use git_status as the source of truth for which files changed,
    // then attach parsed diff content where available
    const changedFiles = filterStatusFiles(statusResult.files, diffScope.value)
    const parsedByPath = new Map(parsedDiffs.map(d => [d.filePath, d]))

    const merged: FileDiff[] = []
    const readQueue: { sf: GitStatusFile; isNew: boolean; isDeleted: boolean; index: number }[] = []

    for (const sf of changedFiles) {
      const parsed = parsedByPath.get(sf.path)
      if (parsed) {
        merged.push(parsed)
        parsedByPath.delete(sf.path)
      } else {
        // Both compared as string: GitFileStatus has neither 'new' nor 'untracked',
        // so both halves are dead today. Cast preserves that exactly rather than
        // guessing which status was meant.
        const isNew = (sf.status as string) === 'new' || (sf.status as string) === 'untracked'
        const isDeleted = sf.status === 'deleted'

        // For new/modified files without parsed diff, read the file content
        // and show it as a full-file addition
        if (!isDeleted) {
          readQueue.push({ sf, isNew, isDeleted, index: merged.length })
        }

        merged.push({
          filePath: sf.path,
          hunks: [],
          isNew,
          isDeleted,
        })
      }
    }

    // Also include any parsed diffs that weren't matched to status files
    for (const remaining of parsedByPath.values()) {
      merged.push(remaining)
    }

    // Read file contents for files missing from diff output
    const workspace_ = workspace
    await Promise.all(readQueue.map(async ({ sf, isNew, isDeleted, index }) => {
      try {
        const content = await invoke<string>('plugin:canvas|read_file', {
          path: workspace_.path + '/' + sf.path,
        })
        if (content) {
          const lines = content.split('\n')
          merged[index] = {
            filePath: sf.path,
            hunks: [{
              id: `read-${sf.path}`,
              filePath: sf.path,
              oldStart: 0,
              oldLines: 0,
              newStart: 1,
              newLines: lines.length,
              oldContent: '',
              newContent: content,
              accepted: undefined,
            }],
            isNew,
            isDeleted,
          }
        }
      } catch {
        // File couldn't be read — keep empty hunks
      }
    }))

    gitDiffs.value = merged
    activeFileIndex.value = 0
  } catch (err) {
    errorMsg.value = err instanceof Error ? err.message : String(err)
    gitDiffs.value = []
  } finally {
    loading.value = false
  }
}

function switchScope(scope: 'all' | 'staged' | 'unstaged') {
  diffScope.value = scope
  loadGitDiff()
}

function getOldLines(hunk: DiffHunk): string[] {
  return hunk.oldContent.split('\n')
}

function getNewLines(hunk: DiffHunk): string[] {
  return hunk.newContent.split('\n')
}

interface DiffLine {
  type: 'unchanged' | 'added' | 'removed'
  content: string
  oldLineNum: number | null
  newLineNum: number | null
}

function computeInlineDiff(hunk: DiffHunk): DiffLine[] {
  const oldLines = getOldLines(hunk)
  const newLines = getNewLines(hunk)
  const lines: DiffLine[] = []

  const maxLen = Math.max(oldLines.length, newLines.length)
  let oldIdx = 0
  let newIdx = 0

  for (let i = 0; i < maxLen; i++) {
    const oldLine = oldIdx < oldLines.length ? oldLines[oldIdx] : null
    const newLine = newIdx < newLines.length ? newLines[newIdx] : null

    if (oldLine !== null && newLine !== null && oldLine === newLine) {
      lines.push({
        type: 'unchanged',
        content: oldLine,
        oldLineNum: hunk.oldStart + oldIdx,
        newLineNum: hunk.newStart + newIdx,
      })
      oldIdx++
      newIdx++
    } else {
      if (newLine !== null && oldLine !== null && newLines.indexOf(oldLine, newIdx) > newIdx) {
        lines.push({
          type: 'added',
          content: newLine,
          oldLineNum: null,
          newLineNum: hunk.newStart + newIdx,
        })
        newIdx++
        i--
      } else if (oldLine !== null && newLine !== null && oldLines.indexOf(newLine, oldIdx) > oldIdx) {
        lines.push({
          type: 'removed',
          content: oldLine,
          oldLineNum: hunk.oldStart + oldIdx,
          newLineNum: null,
        })
        oldIdx++
        i--
      } else {
        if (oldLine !== null) {
          lines.push({
            type: 'removed',
            content: oldLine,
            oldLineNum: hunk.oldStart + oldIdx,
            newLineNum: null,
          })
          oldIdx++
        }
        if (newLine !== null) {
          lines.push({
            type: 'added',
            content: newLine,
            oldLineNum: null,
            newLineNum: hunk.newStart + newIdx,
          })
          newIdx++
        }
      }
    }
  }

  return lines
}

// Memoized per hunk id. Called straight from the template, computeInlineDiff
// re-ran for every rendered hunk on any state change (accept, reject, loading
// flags) — and it is quadratic in hunk size.
const inlineDiffs = computed(() => {
  const byHunk = new Map<string, DiffLine[]>()
  if (viewMode.value !== 'inline') return byHunk
  for (const hunk of activeFile.value?.hunks ?? []) {
    byHunk.set(hunk.id, computeInlineDiff(hunk))
  }
  return byHunk
})

function extractFileName(path: string): string {
  return path.split(/[/\\]/).pop() ?? path
}

onMounted(() => {
  loadGitDiff()
})
</script>

<template>
  <div class="flex flex-col h-full">
    <!-- Toolbar -->
    <div class="flex items-center justify-between px-3 py-1.5 flex-shrink-0" :style="{ borderBottom: '1px solid var(--qc-border)', background: 'var(--qc-bg-header)' }">
      <div class="flex items-center gap-2">
        <!-- Diff scope toggle -->
        <button
          class="px-2 py-0.5 text-[10px] rounded transition-colors"
          :style="diffScope === 'all' ? { background: 'var(--qc-bg-surface)', color: 'var(--qc-text)' } : { color: 'var(--qc-text-dim)' }"
          @click="switchScope('all')"
        >
          All
        </button>
        <button
          class="px-2 py-0.5 text-[10px] rounded transition-colors"
          :style="diffScope === 'staged' ? { background: 'var(--qc-bg-surface)', color: 'var(--qc-text)' } : { color: 'var(--qc-text-dim)' }"
          @click="switchScope('staged')"
        >
          Staged
        </button>
        <button
          class="px-2 py-0.5 text-[10px] rounded transition-colors"
          :style="diffScope === 'unstaged' ? { background: 'var(--qc-bg-surface)', color: 'var(--qc-text)' } : { color: 'var(--qc-text-dim)' }"
          @click="switchScope('unstaged')"
        >
          Unstaged
        </button>
        <span class="text-[9px] px-1" :style="{ color: 'var(--qc-text-dim)' }">|</span>

        <!-- View mode toggle -->
        <button
          class="px-2 py-0.5 text-[10px] rounded transition-colors"
          :style="viewMode === 'inline' ? { background: 'var(--qc-bg-surface)', color: 'var(--qc-text)' } : { color: 'var(--qc-text-dim)' }"
          @click="viewMode = 'inline'"
        >
          Inline
        </button>
        <button
          class="px-2 py-0.5 text-[10px] rounded transition-colors"
          :style="viewMode === 'side-by-side' ? { background: 'var(--qc-bg-surface)', color: 'var(--qc-text)' } : { color: 'var(--qc-text-dim)' }"
          @click="viewMode = 'side-by-side'"
        >
          Side by Side
        </button>
      </div>

      <div class="flex items-center gap-2">
        <!-- File count + branch badge (git mode) -->
        <span
          v-if="fileDiffs.length > 0"
          class="text-[9px] px-1.5 py-0.5 rounded"
          :style="{ background: 'var(--qc-bg-surface)', color: 'var(--qc-text-dim)' }"
        >
          {{ fileDiffs.length }} file{{ fileDiffs.length !== 1 ? 's' : '' }}
        </span>
        <span
          v-if="gitBranch"
          class="text-[9px] px-1.5 py-0.5 rounded"
          :style="{ background: 'var(--qc-bg-surface)', color: 'var(--qc-text-dim)' }"
        >
          {{ gitBranch }}
        </span>

        <!-- Refresh (git mode) -->
        <button
          class="px-2 py-0.5 text-[10px] rounded transition-colors"
          :style="{ color: 'var(--qc-text-dim)' }"
          :disabled="loading"
          @click="loadGitDiff"
        >
          {{ loading ? '...' : 'Refresh' }}
        </button>

      </div>
    </div>

    <!-- File tabs -->
    <div
      v-if="fileDiffs.length > 0"
      class="flex items-center gap-0 overflow-x-auto flex-shrink-0"
      :style="{ borderBottom: '1px solid var(--qc-border)', background: 'var(--qc-bg)' }"
    >
      <button
        v-for="(file, idx) in fileDiffs"
        :key="file.filePath"
        class="px-3 py-1.5 text-[10px] transition-colors flex items-center gap-1.5 flex-shrink-0"
        :style="{
          borderRight: '1px solid var(--qc-border)',
          background: idx === activeFileIndex ? 'var(--qc-bg-window)' : 'transparent',
          color: idx === activeFileIndex ? 'var(--qc-text)' : 'var(--qc-text-dim)',
          borderBottom: idx === activeFileIndex ? '2px solid var(--color-accent)' : '2px solid transparent',
        }"
        @click="activeFileIndex = idx"
      >
        <span
          v-if="file.isNew"
          class="text-green-400"
        >N</span>
        <span
          v-else-if="file.isDeleted"
          class="text-red-400"
        >D</span>
        <span
          v-else
          class="text-amber-400"
        >M</span>
        {{ extractFileName(file.filePath) }}
      </button>
    </div>

    <!-- Loading state -->
    <div
      v-if="loading"
      class="flex-1 flex items-center justify-center text-xs"
      :style="{ color: 'var(--qc-text-dim)' }"
    >
      Loading diff...
    </div>

    <!-- Error state -->
    <div
      v-else-if="errorMsg"
      class="flex-1 flex items-center justify-center text-xs px-4 text-center"
      :style="{ color: 'var(--qc-text-dim)' }"
    >
      {{ errorMsg }}
    </div>

    <!-- Empty state -->
    <div
      v-else-if="fileDiffs.length === 0"
      class="flex-1 flex flex-col items-center justify-center gap-2 text-xs"
      :style="{ color: 'var(--qc-text-dim)' }"
    >
      <span class="text-2xl opacity-30">&#177;</span>
      <span>No {{ diffScope === 'all' ? '' : diffScope + ' ' }}changes</span>
    </div>

    <!-- Diff content -->
    <div
      v-else
      class="flex-1 min-h-0 overflow-auto font-mono text-xs"
    >
      <template v-if="activeFile">
        <!-- No diff content for this file (binary, empty, etc.) -->
        <div
          v-if="activeFile.hunks.length === 0"
          class="flex items-center justify-center h-full text-xs"
          :style="{ color: 'var(--qc-text-dim)' }"
        >
          {{ activeFile.isDeleted ? 'Deleted file' : 'Binary or empty diff' }}
        </div>
        <div
          v-for="hunk in activeFile.hunks"
          :key="hunk.id"
          :style="{ borderBottom: '1px solid var(--qc-border)' }"
        >
          <!-- Hunk header -->
          <div class="flex items-center justify-between px-3 py-1 text-[10px]" :style="{ background: 'var(--qc-bg-header)', color: 'var(--qc-text-dim)' }">
            <span>@@ -{{ hunk.oldStart }},{{ hunk.oldLines }} +{{ hunk.newStart }},{{ hunk.newLines }} @@</span>
          </div>

          <!-- Inline view -->
          <template v-if="viewMode === 'inline'">
            <div
              v-for="(line, lineIdx) in inlineDiffs.get(hunk.id) ?? []"
              :key="lineIdx"
              class="flex"
              :class="{
                'bg-red-500/10': line.type === 'removed',
                'bg-green-500/10': line.type === 'added',
              }"
            >
              <span class="w-10 text-right pr-2 select-none flex-shrink-0" :style="{ color: 'var(--qc-text-dim)', borderRight: '1px solid var(--qc-border)' }">
                {{ line.oldLineNum ?? '' }}
              </span>
              <span class="w-10 text-right pr-2 select-none flex-shrink-0" :style="{ color: 'var(--qc-text-dim)', borderRight: '1px solid var(--qc-border)' }">
                {{ line.newLineNum ?? '' }}
              </span>
              <span class="w-5 text-center flex-shrink-0" :class="{
                'text-red-400': line.type === 'removed',
                'text-green-400': line.type === 'added',
              }">
                {{ line.type === 'removed' ? '-' : line.type === 'added' ? '+' : ' ' }}
              </span>
              <span class="px-2 whitespace-pre">{{ line.content }}</span>
            </div>
          </template>

          <!-- Side by side view -->
          <template v-else>
            <div class="flex">
              <!-- Old side -->
              <div class="flex-1" :style="{ borderRight: '1px solid var(--qc-border)' }">
                <div
                  v-for="(line, lineIdx) in getOldLines(hunk)"
                  :key="'old-' + lineIdx"
                  class="flex bg-red-500/5"
                >
                  <span class="w-10 text-right pr-2 select-none flex-shrink-0" :style="{ color: 'var(--qc-text-dim)', borderRight: '1px solid var(--qc-border)' }">
                    {{ hunk.oldStart + lineIdx }}
                  </span>
                  <span class="px-2 whitespace-pre text-red-300">{{ line }}</span>
                </div>
              </div>
              <!-- New side -->
              <div class="flex-1">
                <div
                  v-for="(line, lineIdx) in getNewLines(hunk)"
                  :key="'new-' + lineIdx"
                  class="flex bg-green-500/5"
                >
                  <span class="w-10 text-right pr-2 select-none flex-shrink-0" :style="{ color: 'var(--qc-text-dim)', borderRight: '1px solid var(--qc-border)' }">
                    {{ hunk.newStart + lineIdx }}
                  </span>
                  <span class="px-2 whitespace-pre text-green-300">{{ line }}</span>
                </div>
              </div>
            </div>
          </template>
        </div>
      </template>
    </div>
  </div>
</template>
