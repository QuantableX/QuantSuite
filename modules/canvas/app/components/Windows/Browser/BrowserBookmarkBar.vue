<script setup lang="ts">
import type { BrowserBookmark } from '../../../../shared/types'
import type { Ref } from 'vue'

defineProps<{
  bookmarks: BrowserBookmark[]
}>()

const emit = defineEmits<{
  navigate: [url: string]
  remove: [id: string]
}>()

const contextMenu = ref<{ id: string; x: number; y: number } | null>(null)
const ctxMenuRef = ref<HTMLElement | null>(null)

// Overlay rects provided by BrowserWindow — punch holes in the webview clip region
const overlayRects = inject<Ref<{ x: number; y: number; w: number; h: number }[]>>(
  'browserOverlayRects',
  ref([]),
)

function truncate(text: string, max: number): string {
  return text.length > max ? text.slice(0, max) + '...' : text
}

function onContextMenu(e: MouseEvent, bookmark: BrowserBookmark) {
  e.preventDefault()
  contextMenu.value = { id: bookmark.id, x: e.clientX, y: e.clientY }

  // Wait for the menu to render, then register its rect as an overlay
  nextTick(() => {
    updateOverlayRect()
  })
}

function updateOverlayRect() {
  if (!ctxMenuRef.value) {
    overlayRects.value = []
    return
  }
  const rect = ctxMenuRef.value.getBoundingClientRect()
  // Add padding so the menu is fully visible
  overlayRects.value = [{
    x: rect.left - 2,
    y: rect.top - 2,
    w: rect.width + 4,
    h: rect.height + 4,
  }]
}

function removeBookmark() {
  if (contextMenu.value) {
    emit('remove', contextMenu.value.id)
    closeContextMenu()
  }
}

function closeContextMenu() {
  contextMenu.value = null
  overlayRects.value = []
}

onMounted(() => {
  document.addEventListener('click', closeContextMenu)
})

onUnmounted(() => {
  document.removeEventListener('click', closeContextMenu)
  overlayRects.value = []
})
</script>

<template>
  <div
    v-if="bookmarks.length > 0"
    class="flex items-center gap-1 px-2 flex-shrink-0 overflow-x-auto no-scrollbar"
    :style="{
      height: '24px',
      background: 'var(--qc-bg-header)',
      borderBottom: '1px solid var(--qc-border)',
    }"
  >
    <button
      v-for="bookmark in bookmarks"
      :key="bookmark.id"
      class="flex items-center px-2 py-0.5 rounded flex-shrink-0 transition-colors hover:brightness-125"
      :style="{
        background: 'var(--qc-bg-surface)',
        color: 'var(--qc-text-dim)',
        border: '1px solid var(--qc-border)',
        fontSize: '10px',
        lineHeight: '1',
      }"
      @click="emit('navigate', bookmark.url)"
      @contextmenu="onContextMenu($event, bookmark)"
      @mousedown.stop
    >
      {{ truncate(bookmark.title || bookmark.url, 25) }}
    </button>

    <!-- Context menu -->
    <Teleport to="body">
      <div
        v-if="contextMenu"
        ref="ctxMenuRef"
        class="fixed z-[9999] rounded overflow-hidden"
        :style="{
          left: contextMenu.x + 'px',
          top: contextMenu.y + 'px',
          background: 'var(--qc-bg-surface)',
          border: '1px solid var(--qc-border)',
          boxShadow: '0 2px 8px rgba(0, 0, 0, 0.3)',
        }"
        @click.stop
      >
        <button
          class="px-3 py-1.5 text-[10px] w-full text-left transition-colors hover:brightness-125"
          :style="{ color: 'var(--qc-text)', background: 'transparent' }"
          @click="removeBookmark"
          @mousedown.stop
        >
          Remove
        </button>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.no-scrollbar::-webkit-scrollbar {
  display: none;
}
.no-scrollbar {
  -ms-overflow-style: none;
  scrollbar-width: none;
}
</style>
