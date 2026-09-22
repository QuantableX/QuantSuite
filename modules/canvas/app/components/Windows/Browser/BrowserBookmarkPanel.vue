<script setup lang="ts">
import type { BrowserBookmark } from '../../../../shared/types'

const props = defineProps<{
  bookmarks: BrowserBookmark[]
}>()

const emit = defineEmits<{
  navigate: [url: string]
  close: []
  remove: [id: string]
}>()

const searchQuery = ref('')

const filteredBookmarks = computed(() => {
  const query = searchQuery.value.toLowerCase().trim()
  if (!query) return props.bookmarks
  return props.bookmarks.filter(
    (b) =>
      b.url.toLowerCase().includes(query) ||
      b.title.toLowerCase().includes(query),
  )
})

interface GroupedBookmarks {
  folder: string
  bookmarks: BrowserBookmark[]
}

const groupedBookmarks = computed<GroupedBookmarks[]>(() => {
  const groups = new Map<string, BrowserBookmark[]>()

  for (const b of filteredBookmarks.value) {
    const folder = b.folder || 'Unsorted'
    if (!groups.has(folder)) groups.set(folder, [])
    groups.get(folder)!.push(b)
  }

  return Array.from(groups.entries())
    .map(([folder, bookmarks]) => ({ folder, bookmarks }))
    .sort((a, b) => {
      if (a.folder === 'Unsorted') return 1
      if (b.folder === 'Unsorted') return -1
      return a.folder.localeCompare(b.folder)
    })
})

// Context menu for individual bookmarks
const ctxMenu = ref<{ visible: boolean; x: number; y: number; bookmarkId: string }>({
  visible: false, x: 0, y: 0, bookmarkId: '',
})

function onContextMenu(e: MouseEvent, bookmarkId: string) {
  e.preventDefault()
  ctxMenu.value = { visible: true, x: e.clientX, y: e.clientY, bookmarkId }
}

function removeBookmark() {
  emit('remove', ctxMenu.value.bookmarkId)
  ctxMenu.value.visible = false
}

onMounted(() => {
  const handler = () => { ctxMenu.value.visible = false }
  window.addEventListener('click', handler)
  onUnmounted(() => window.removeEventListener('click', handler))
})

function formatDate(dateStr: string): string {
  return new Date(dateStr).toLocaleDateString()
}
</script>

<template>
  <div
    class="absolute inset-0 z-40 flex flex-col overflow-hidden"
    :style="{ background: 'var(--qc-bg)' }"
  >
    <!-- Header -->
    <div
      class="flex items-center justify-between px-3 py-2 flex-shrink-0"
      :style="{ borderBottom: '1px solid var(--qc-border)' }"
    >
      <span class="text-xs font-medium" :style="{ color: 'var(--qc-text)' }">Bookmarks</span>
      <button
        class="w-5 h-5 flex items-center justify-center rounded transition-colors hover:brightness-125"
        :style="{ color: 'var(--qc-text-dim)' }"
        title="Close"
        @click="emit('close')"
        @mousedown.stop
      >
        <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M18 6L6 18" />
          <path d="M6 6l12 12" />
        </svg>
      </button>
    </div>

    <!-- Search input -->
    <div class="px-3 py-2 flex-shrink-0">
      <input
        v-model="searchQuery"
        type="text"
        class="w-full text-xs px-2.5 py-1 rounded outline-none"
        :style="{
          background: 'var(--qc-bg-surface)',
          color: 'var(--qc-text)',
          border: '1px solid var(--qc-border)',
        }"
        placeholder="Search bookmarks..."
        spellcheck="false"
        @mousedown.stop
      />
    </div>

    <!-- Bookmarks list -->
    <div class="flex-1 min-h-0 overflow-y-auto px-3 pb-2">
      <div v-if="groupedBookmarks.length === 0" class="flex items-center justify-center py-8">
        <span class="text-xs" :style="{ color: 'var(--qc-text-dim)' }">No bookmarks found</span>
      </div>

      <div v-for="group in groupedBookmarks" :key="group.folder" class="mb-3">
        <!-- Folder label -->
        <div
          class="text-[10px] font-medium uppercase tracking-wide px-1 py-1"
          :style="{ color: 'var(--qc-text-dim)' }"
        >
          {{ group.folder }}
        </div>

        <!-- Bookmark entries -->
        <div
          v-for="bookmark in group.bookmarks"
          :key="bookmark.id"
          class="flex items-start justify-between gap-2 px-2 py-1.5 rounded cursor-pointer transition-colors hover:brightness-110"
          @click="emit('navigate', bookmark.url)"
          @contextmenu="onContextMenu($event, bookmark.id)"
        >
          <div class="min-w-0 flex-1">
            <div class="text-[11px] truncate" :style="{ color: 'var(--qc-text)' }">
              {{ bookmark.title || bookmark.url }}
            </div>
            <div class="text-[9px] truncate" :style="{ color: 'var(--qc-text-dim)' }">
              {{ bookmark.url }}
            </div>
          </div>
          <span class="text-[9px] flex-shrink-0 pt-0.5" :style="{ color: 'var(--qc-text-dim)' }">
            {{ formatDate(bookmark.createdAt) }}
          </span>
        </div>
      </div>
    </div>

    <!-- Context menu -->
    <Teleport to="body">
      <div
        v-if="ctxMenu.visible"
        class="fixed z-[9999] rounded-lg shadow-2xl py-1 min-w-[140px]"
        :style="{
          left: ctxMenu.x + 'px',
          top: ctxMenu.y + 'px',
          background: 'var(--qc-bg-header)',
          border: '1px solid var(--qc-border)',
        }"
        @click.stop
      >
        <button
          class="w-full text-left px-3 py-1.5 text-xs transition-colors hover:brightness-125"
          :style="{ color: '#ef4444' }"
          @click="removeBookmark"
        >
          Remove Bookmark
        </button>
      </div>
    </Teleport>
  </div>
</template>
