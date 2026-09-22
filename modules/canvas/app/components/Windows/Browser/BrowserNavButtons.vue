<script setup lang="ts">
defineProps<{
  canGoBack: boolean
  canGoForward: boolean
  isLoading: boolean
}>()

const emit = defineEmits<{
  back: []
  forward: []
  reload: []
}>()
</script>

<template>
  <div class="flex items-center gap-0.5">
    <!-- Back -->
    <button
      class="w-5 h-5 flex items-center justify-center rounded transition-colors"
      :class="canGoBack ? 'hover:brightness-125' : 'opacity-30 cursor-default'"
      :style="{ color: 'var(--qc-text-dim)' }"
      :disabled="!canGoBack"
      title="Back"
      @mousedown.stop
      @click.stop="emit('back')"
    >
      <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M15 18l-6-6 6-6" />
      </svg>
    </button>

    <!-- Forward -->
    <button
      class="w-5 h-5 flex items-center justify-center rounded transition-colors"
      :class="canGoForward ? 'hover:brightness-125' : 'opacity-30 cursor-default'"
      :style="{ color: 'var(--qc-text-dim)' }"
      :disabled="!canGoForward"
      title="Forward"
      @mousedown.stop
      @click.stop="emit('forward')"
    >
      <svg class="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M9 18l6-6-6-6" />
      </svg>
    </button>

    <!-- Reload -->
    <button
      class="w-5 h-5 flex items-center justify-center rounded transition-colors hover:brightness-125"
      :style="{ color: 'var(--qc-text-dim)' }"
      title="Reload"
      @mousedown.stop
      @click.stop="emit('reload')"
    >
      <svg class="w-3.5 h-3.5" :class="{ 'animate-spin': isLoading }" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
        <path d="M21 3v5h-5" />
      </svg>
    </button>
  </div>
</template>
