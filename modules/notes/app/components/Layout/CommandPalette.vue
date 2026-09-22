<script setup lang="ts">
import { watchDebounced } from '@vueuse/core'
import { useAppStore } from '#notes/stores/app'
import { useSearchStore } from '#notes/stores/search'
import type { SearchHit } from '#notes/types'

const app = useAppStore()
const search = useSearchStore()
const router = useRouter()

const query = ref('')

watch(
  () => app.commandPaletteOpen,
  open => {
    if (open) {
      query.value = ''
      search.reset()
    }
  },
)

// One plugin:notes|search round-trip per keystroke; wait for a pause instead.
watchDebounced(query, value => {
  if (!app.commandPaletteOpen) return
  void search.run(value)
}, { debounce: 200 })

async function openResult(hit: SearchHit) {
  await router.push(`/notes/n/${hit.noteId}`)
  app.commandPaletteOpen = false
}
</script>

<template>
  <Teleport to="body">
    <div v-if="app.commandPaletteOpen" class="qn-palette-backdrop" @click.self="app.commandPaletteOpen = false">
      <div class="qn-palette">
        <input
          v-model="query"
          class="qn-palette__input"
          type="text"
          placeholder="Search notes…"
          autofocus
        />
        <div class="qn-palette__results">
          <button
            v-for="hit in search.results"
            :key="hit.noteId"
            class="qn-palette__row"
            @click="openResult(hit)"
          >
            <div class="qn-palette__title">{{ hit.title || 'Untitled' }}</div>
            <div class="qn-palette__meta mono">note</div>
            <div v-if="hit.snippet" class="qn-palette__snippet">{{ hit.snippet }}</div>
          </button>
          <div v-if="search.loading" class="qn-palette__empty">Searching...</div>
          <div v-else-if="query && !search.results.length" class="qn-palette__empty">No matches.</div>
          <div v-else-if="!query" class="qn-palette__empty">Type to search every note.</div>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.qn-palette-backdrop {
  position: fixed;
  inset: 0;
  display: grid;
  place-items: start center;
  padding-top: 10vh;
  background: rgba(0, 0, 0, 0.45);
  z-index: 1000;
}

.qn-palette {
  width: min(720px, calc(100vw - 32px));
  padding: 12px;
  background: var(--qn-bg-card);
  border: 1px solid var(--qn-border);
  border-radius: 12px;
  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
}

.qn-palette__input {
  width: 100%;
  margin-bottom: 10px;
  padding: 10px 12px;
  border: 1px solid var(--qn-border);
  border-radius: var(--qn-radius);
  background: var(--qn-bg-input);
  color: var(--qn-text);
  font-size: 14px;
  outline: none;
}

.qn-palette__input:focus {
  border-color: var(--qn-accent);
}

.qn-palette__results {
  display: flex;
  flex-direction: column;
  max-height: 420px;
  overflow: auto;
}

.qn-palette__row {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 4px 12px;
  width: 100%;
  padding: 10px 12px;
  border: none;
  border-radius: var(--qn-radius);
  background: transparent;
  color: inherit;
  text-align: left;
  cursor: pointer;
}

.qn-palette__row:hover {
  background: var(--qn-bg-hover);
}

.qn-palette__title {
  font-weight: 600;
}

.qn-palette__meta,
.qn-palette__snippet,
.qn-palette__empty {
  color: var(--qn-text-secondary);
  font-size: 12px;
}

.qn-palette__meta {
  text-transform: uppercase;
  font-size: 10px;
  letter-spacing: 0.05em;
}

/* A content hit brings a whole line of surrounding text; clipping it keeps
   every result row the same height. */
.qn-palette__snippet {
  grid-column: 1 / -1;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.qn-palette__empty {
  padding: 16px 12px;
}
</style>
