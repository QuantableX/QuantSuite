<script setup lang="ts">
import type { BrowserHistoryEntry } from '../../../../shared/types'

const props = defineProps<{
  modelValue: string
  currentUrl: string
  history: BrowserHistoryEntry[]
  isBookmarked: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [value: string]
  submit: [value: string]
  toggleBookmark: []
}>()

const inputRef = ref<HTMLInputElement | null>(null)
const dropdownOpen = ref(false)
const selectedIndex = ref(-1)

const inputValue = computed({
  get: () => props.modelValue,
  set: (v: string) => emit('update:modelValue', v),
})

const isHttps = computed(() => {
  try {
    return new URL(props.currentUrl).protocol === 'https:'
  } catch {
    return false
  }
})

const filteredHistory = computed(() => {
  const query = inputValue.value.toLowerCase().trim()
  if (!query) return props.history.slice(0, 8)
  return props.history
    .filter(
      (entry) =>
        entry.url.toLowerCase().includes(query) ||
        entry.title.toLowerCase().includes(query),
    )
    .slice(0, 8)
})

function onFocus(e: FocusEvent) {
  const target = e.target as HTMLInputElement
  target.select()
  if (inputValue.value.trim()) {
    dropdownOpen.value = true
  }
}

function onInput() {
  selectedIndex.value = -1
  dropdownOpen.value = inputValue.value.trim().length > 0 || filteredHistory.value.length > 0
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    if (!dropdownOpen.value) {
      dropdownOpen.value = true
    }
    if (selectedIndex.value < filteredHistory.value.length - 1) {
      selectedIndex.value++
    }
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    if (selectedIndex.value > 0) {
      selectedIndex.value--
    } else {
      selectedIndex.value = -1
    }
  } else if (e.key === 'Enter') {
    e.preventDefault()
    if (selectedIndex.value >= 0 && selectedIndex.value < filteredHistory.value.length) {
      const entry = filteredHistory.value[selectedIndex.value]
      inputValue.value = entry.url
      emit('submit', entry.url)
    } else {
      emit('submit', inputValue.value)
    }
    dropdownOpen.value = false
    selectedIndex.value = -1
  } else if (e.key === 'Escape') {
    dropdownOpen.value = false
    selectedIndex.value = -1
    inputRef.value?.blur()
  }
}

function selectItem(entry: BrowserHistoryEntry) {
  inputValue.value = entry.url
  emit('submit', entry.url)
  dropdownOpen.value = false
  selectedIndex.value = -1
}

function onClickOutside() {
  dropdownOpen.value = false
  selectedIndex.value = -1
}

onMounted(() => {
  document.addEventListener('click', onClickOutside)
})

onUnmounted(() => {
  document.removeEventListener('click', onClickOutside)
})
</script>

<template>
  <div class="flex-1 min-w-0 relative" @click.stop>
    <div class="flex items-center relative">
      <!-- Lock / Globe icon -->
      <div
        class="absolute left-2 flex items-center pointer-events-none"
        :style="{ color: isHttps ? '#22c55e' : 'var(--qc-text-dim)' }"
      >
        <!-- Lock (https) -->
        <svg v-if="isHttps" class="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
          <path d="M7 11V7a5 5 0 0 1 10 0v4" />
        </svg>
        <!-- Globe (http) -->
        <svg v-else class="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="10" />
          <path d="M2 12h20" />
          <path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z" />
        </svg>
      </div>

      <!-- Input -->
      <input
        ref="inputRef"
        v-model="inputValue"
        type="text"
        class="w-full text-xs py-1 rounded outline-none"
        :style="{
          paddingLeft: '26px',
          paddingRight: '28px',
          background: 'var(--qc-bg)',
          color: 'var(--qc-text)',
          border: '1px solid var(--qc-border)',
        }"
        placeholder="Search or enter URL..."
        spellcheck="false"
        @mousedown.stop
        @focus="onFocus"
        @input="onInput"
        @keydown="onKeydown"
      />

      <!-- Bookmark star -->
      <button
        class="absolute right-1.5 flex items-center justify-center w-4 h-4 rounded transition-colors hover:brightness-125"
        :style="{ color: isBookmarked ? '#f97316' : 'var(--qc-text-dim)' }"
        title="Toggle bookmark"
        @click.stop="emit('toggleBookmark')"
        @mousedown.stop
      >
        <svg class="w-3 h-3" viewBox="0 0 24 24" :fill="isBookmarked ? 'currentColor' : 'none'" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
        </svg>
      </button>
    </div>

    <!-- Dropdown -->
    <div
      v-if="dropdownOpen && filteredHistory.length > 0"
      class="absolute left-0 right-0 mt-0.5 rounded overflow-hidden z-50"
      :style="{
        background: 'var(--qc-bg-surface)',
        border: '1px solid var(--qc-border)',
        boxShadow: '0 4px 12px rgba(0, 0, 0, 0.3)',
      }"
    >
      <div
        v-for="(entry, index) in filteredHistory"
        :key="entry.url + entry.visitedAt"
        class="px-2.5 py-1.5 cursor-pointer transition-colors"
        :style="{
          background: index === selectedIndex ? 'var(--qc-bg-header)' : 'transparent',
          borderBottom: index < filteredHistory.length - 1 ? '1px solid var(--qc-border)' : 'none',
        }"
        @mousedown.prevent="selectItem(entry)"
        @mouseenter="selectedIndex = index"
      >
        <div class="text-[10px] truncate" :style="{ color: 'var(--qc-text)' }">
          {{ entry.title || entry.url }}
        </div>
        <div class="text-[9px] truncate" :style="{ color: 'var(--qc-text-dim)' }">
          {{ entry.url }}
        </div>
      </div>
    </div>
  </div>
</template>
