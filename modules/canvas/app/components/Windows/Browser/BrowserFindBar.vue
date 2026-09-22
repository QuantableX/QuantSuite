<script setup lang="ts">
const props = defineProps<{
  modelValue: string
}>()

const emit = defineEmits<{
  'update:modelValue': [value: string]
  findNext: []
  findPrev: []
  close: []
}>()

const inputRef = ref<HTMLInputElement | null>(null)

const inputValue = computed({
  get: () => props.modelValue,
  set: (v: string) => emit('update:modelValue', v),
})

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    e.preventDefault()
    if (e.shiftKey) {
      emit('findPrev')
    } else {
      emit('findNext')
    }
  } else if (e.key === 'Escape') {
    emit('close')
  }
}

onMounted(() => {
  nextTick(() => {
    inputRef.value?.focus()
  })
})
</script>

<template>
  <div
    class="absolute top-2 right-2 z-50 flex items-center gap-1 px-2 py-1 rounded-md"
    :style="{
      background: 'var(--qc-bg-surface)',
      border: '1px solid var(--qc-border)',
      boxShadow: '0 2px 8px rgba(0, 0, 0, 0.3)',
    }"
  >
    <!-- Search input -->
    <input
      ref="inputRef"
      v-model="inputValue"
      type="text"
      class="text-xs px-1.5 py-0.5 rounded outline-none w-40"
      :style="{
        background: 'var(--qc-bg)',
        color: 'var(--qc-text)',
        border: '1px solid var(--qc-border)',
      }"
      placeholder="Find in page..."
      spellcheck="false"
      @keydown="onKeydown"
      @mousedown.stop
    />

    <!-- Previous -->
    <button
      class="w-5 h-5 flex items-center justify-center rounded transition-colors hover:brightness-125"
      :style="{ color: 'var(--qc-text-dim)' }"
      title="Previous match"
      @click="emit('findPrev')"
      @mousedown.stop
    >
      <svg class="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M18 15l-6-6-6 6" />
      </svg>
    </button>

    <!-- Next -->
    <button
      class="w-5 h-5 flex items-center justify-center rounded transition-colors hover:brightness-125"
      :style="{ color: 'var(--qc-text-dim)' }"
      title="Next match"
      @click="emit('findNext')"
      @mousedown.stop
    >
      <svg class="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M6 9l6 6 6-6" />
      </svg>
    </button>

    <!-- Close -->
    <button
      class="w-5 h-5 flex items-center justify-center rounded transition-colors hover:brightness-125"
      :style="{ color: 'var(--qc-text-dim)' }"
      title="Close"
      @click="emit('close')"
      @mousedown.stop
    >
      <svg class="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M18 6L6 18" />
        <path d="M6 6l12 12" />
      </svg>
    </button>
  </div>
</template>
