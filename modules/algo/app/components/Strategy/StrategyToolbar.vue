<script setup lang="ts">
import { ref, watch } from 'vue'
import { useBotsStore } from '#algo/stores/bots'
import type { Strategy } from '#algo/types'

const botsStore = useBotsStore()

const props = withDefaults(
  defineProps<{
    strategy?: Strategy | null
    isSaving?: boolean
  }>(),
  {
    strategy: null,
    isSaving: false,
  },
)

const emit = defineEmits<{
  create: []
  remove: []
  save: []
  validate: []
  runBacktest: []
  deploy: []
  rename: [name: string]
}>()

const isEditing = ref(false)
const editName = ref('')
const nameInputRef = ref<HTMLInputElement | null>(null)

// Bots running this strategy — a strategy can be behind several bots at once.
const runningHere = computed(() =>
  props.strategy ? botsStore.running.filter((b) => b.strategy_id === props.strategy?.id).length : 0,
)

function startEditing() {
  if (!props.strategy) return
  editName.value = props.strategy.name
  isEditing.value = true
  nextTick(() => {
    nameInputRef.value?.focus()
    nameInputRef.value?.select()
  })
}

function finishEditing() {
  const trimmed = editName.value.trim()
  if (trimmed && trimmed !== props.strategy?.name) {
    emit('rename', trimmed)
  }
  isEditing.value = false
}

watch(
  () => props.strategy?.id,
  () => {
    isEditing.value = false
  },
)
</script>

<template>
  <div class="toolbar">
    <div class="toolbar-left">
      <template v-if="strategy">
        <input
          v-if="isEditing"
          ref="nameInputRef"
          v-model="editName"
          class="name-input"
          type="text"
          @blur="finishEditing"
          @keydown.enter="finishEditing"
          @keydown.escape="finishEditing"
        />
        <button
          v-else
          class="name-display"
          @click="startEditing"
        >
          {{ strategy.name }}
        </button>
        <span v-if="runningHere" class="mode-badge">{{ runningHere }} bot{{ runningHere === 1 ? '' : 's' }} running</span>
      </template>
      <span v-else class="name-placeholder">No strategy selected</span>
    </div>

    <div class="toolbar-right">
      <button class="btn btn-sm" @click="emit('create')">
        <span class="btn-icon">+</span>
        New
      </button>
      <button
        class="btn btn-sm"
        :disabled="!strategy"
        @click="emit('remove')"
      >
        <span class="btn-icon">&times;</span>
        Delete
      </button>
      <button
        class="btn btn-sm"
        :disabled="isSaving || !strategy"
        @click="emit('save')"
      >
        <span class="btn-icon">&#10003;</span>
        {{ isSaving ? 'Saving...' : 'Save' }}
      </button>
      <button
        class="btn btn-sm"
        :disabled="isSaving || !strategy"
        @click="emit('validate')"
      >
        <span class="btn-icon">&#10003;</span>
        Validate
      </button>
      <button
        class="btn btn-sm"
        :disabled="!strategy"
        @click="emit('runBacktest')"
      >
        <span class="btn-icon">&#10697;</span>
        Run Backtest
      </button>
      <button
        class="btn btn-sm btn-success"
        :disabled="!strategy"
        @click="emit('deploy')"
      >
        <span class="btn-icon">&#9654;</span>
        New Bot
      </button>
    </div>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 44px;
  padding: 0 16px;
  background: var(--qa-bg-card);
  border-bottom: 1px solid var(--qa-border);
  flex-shrink: 0;
}

.toolbar-left {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.toolbar-right {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.name-display {
  background: none;
  border: none;
  color: var(--qa-text);
  font-size: 14px;
  font-weight: 600;
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
  transition: background var(--qa-transition);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 300px;
}

.name-display:hover {
  background: var(--qa-bg-hover);
}

.name-input {
  background: var(--qa-bg-input);
  border: 1px solid var(--qa-accent);
  color: var(--qa-text);
  font-size: 14px;
  font-weight: 600;
  padding: 4px 8px;
  border-radius: 4px;
  outline: none;
  max-width: 300px;
}

.name-placeholder {
  font-size: 14px;
  color: var(--qa-text-muted);
}

.btn-icon {
  font-size: 12px;
  line-height: 1;
}

.mode-badge {
  padding: 2px 8px;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  border: 1px solid var(--qa-success);
  border-radius: 9999px;
  color: var(--qa-success);
  background: color-mix(in srgb, var(--qa-success) 10%, transparent);
  white-space: nowrap;
}
</style>
