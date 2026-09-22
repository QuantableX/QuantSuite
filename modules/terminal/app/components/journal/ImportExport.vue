<script setup lang="ts">
import { Download, Upload, X, FileJson, FileSpreadsheet, AlertTriangle } from 'lucide-vue-next'
import { useJournalStore } from '#terminal/stores/journal'
import type { JournalTrade } from '#terminal/stores/journal'

const store = useJournalStore()

const emit = defineEmits<{
  close: []
}>()

const importMode = ref<'merge' | 'replace'>('merge')
const importData = ref<JournalTrade[] | null>(null)
const importError = ref('')
const fileInput = ref<HTMLInputElement | null>(null)

function exportJSON() {
  const json = store.exportJSON()
  downloadFile(json, 'quantjournal-trades.json', 'application/json')
}

function exportCSV() {
  const csv = store.exportCSV()
  downloadFile(csv, 'quantjournal-trades.csv', 'text/csv')
}

function downloadFile(content: string, filename: string, mime: string) {
  const blob = new Blob([content], { type: mime })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

function onFileSelect(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0]
  if (!file) return
  importError.value = ''
  importData.value = null

  const reader = new FileReader()
  reader.onload = (e) => {
    try {
      const data = JSON.parse(e.target?.result as string)
      if (!Array.isArray(data)) {
        importError.value = 'Invalid format: expected an array of trades'
        return
      }
      importData.value = data as JournalTrade[]
    } catch {
      importError.value = 'Invalid JSON file'
    }
  }
  reader.readAsText(file)
}

function confirmImport() {
  if (!importData.value) return
  store.importJSON(importData.value, importMode.value === 'merge')
  importData.value = null
  emit('close')
}

function onOverlayClick(e: MouseEvent) {
  if ((e.target as HTMLElement).classList.contains('modal-overlay')) {
    emit('close')
  }
}
</script>

<template>
  <div class="modal-overlay fixed inset-0 z-50 flex items-center justify-center p-4" style="background-color: rgba(0,0,0,0.6); backdrop-filter: blur(4px)" @mousedown="onOverlayClick">
    <div class="w-full max-w-md rounded-lg" style="background-color: var(--surface-1); border: 1px solid var(--border)">
      <!-- Header -->
      <div class="flex items-center justify-between px-4 py-3" style="border-bottom: 1px solid var(--border)">
        <h3 class="text-sm font-bold" style="color: var(--text-primary)">Import / Export</h3>
        <button class="p-1 rounded hover:brightness-125" style="color: var(--muted)" @click="emit('close')">
          <X class="w-4 h-4" />
        </button>
      </div>

      <div class="p-4 space-y-4">
        <!-- Export Section -->
        <div>
          <h4 class="text-xs font-semibold mb-2" style="color: var(--text-primary)">Export ({{ store.trades.length }} trades)</h4>
          <div class="flex gap-2">
            <button class="flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded text-xs font-medium transition-all hover:brightness-110" style="background-color: var(--surface-2); color: var(--text-primary)" @click="exportJSON">
              <FileJson class="w-3.5 h-3.5" />
              Export JSON
            </button>
            <button class="flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded text-xs font-medium transition-all hover:brightness-110" style="background-color: var(--surface-2); color: var(--text-primary)" @click="exportCSV">
              <FileSpreadsheet class="w-3.5 h-3.5" />
              Export CSV
            </button>
          </div>
        </div>

        <!-- Divider -->
        <div class="h-px" style="background-color: var(--border)" />

        <!-- Import Section -->
        <div>
          <h4 class="text-xs font-semibold mb-2" style="color: var(--text-primary)">Import from JSON</h4>
          <input
            ref="fileInput"
            type="file"
            accept=".json"
            class="hidden"
            @change="onFileSelect"
          />
          <button
            class="w-full flex items-center justify-center gap-2 px-3 py-3 rounded text-xs font-medium transition-all hover:brightness-110 border-2 border-dashed"
            style="background-color: var(--surface-2); color: var(--text-secondary); border-color: var(--border)"
            @click="fileInput?.click()"
          >
            <Upload class="w-4 h-4" />
            {{ importData ? importData.length + ' trades loaded' : 'Select JSON file...' }}
          </button>

          <div v-if="importError" class="flex items-center gap-2 mt-2 text-xs" style="color: var(--negative)">
            <AlertTriangle class="w-3.5 h-3.5" />
            {{ importError }}
          </div>

          <!-- Import Options -->
          <div v-if="importData" class="mt-3 space-y-3">
            <div class="flex gap-2">
              <button
                class="flex-1 py-2 rounded text-xs font-medium transition-all"
                :style="{
                  backgroundColor: importMode === 'merge' ? 'var(--accent)' : 'var(--surface-2)',
                  color: importMode === 'merge' ? '#fff' : 'var(--text-secondary)',
                }"
                @click="importMode = 'merge'"
              >Merge (add new)</button>
              <button
                class="flex-1 py-2 rounded text-xs font-medium transition-all"
                :style="{
                  backgroundColor: importMode === 'replace' ? 'var(--accent)' : 'var(--surface-2)',
                  color: importMode === 'replace' ? '#fff' : 'var(--text-secondary)',
                }"
                @click="importMode = 'replace'"
              >Replace all</button>
            </div>

            <div v-if="importMode === 'replace'" class="flex items-center gap-2 text-[11px] px-2 py-1.5 rounded" style="background-color: rgba(239, 68, 68, 0.1); color: var(--negative)">
              <AlertTriangle class="w-3.5 h-3.5 shrink-0" />
              This will delete all existing trades
            </div>

            <button
              class="w-full py-2 rounded text-xs font-medium"
              style="background-color: var(--accent); color: #fff"
              @click="confirmImport"
            >
              Import {{ importData.length }} trades
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
