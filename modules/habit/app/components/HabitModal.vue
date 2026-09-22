<script setup lang="ts">
/**
 * The one habit dialog. Create mode: name + weekday schedule. Edit mode: the
 * same form, plus the lifecycle — pause/resume and the two-click delete.
 *
 * Opened via the store (`habits.editor`), mounted once in the module layout.
 * `position: fixed` is correct here — overlays are the documented exception
 * to the no-viewport rule (ARCHITECTURE.md §1), and the module root's
 * transform contains it inside the panel.
 */
import { useHabitsStore } from '#habit/stores/habits'
import { WEEKDAYS } from '#habit/utils/dates'
import { ALL_DAYS } from '#habit/types'

const habits = useHabitsStore()

const mode = computed(() => habits.editor?.mode ?? null)
const editing = computed(() =>
  habits.editor?.mode === 'edit'
    ? habits.habits.find((h) => h.id === (habits.editor as { id: string }).id) ?? null
    : null
)

const name = ref('')
const mask = ref(ALL_DAYS)
const error = ref<string | null>(null)
const nameEl = ref<HTMLInputElement | null>(null)

watch(
  () => habits.editor,
  async (e) => {
    if (!e) return
    error.value = null
    if (e.mode === 'edit') {
      name.value = editing.value?.name ?? ''
      mask.value = editing.value?.daysMask ?? ALL_DAYS
    } else {
      name.value = ''
      mask.value = ALL_DAYS
    }
    await nextTick()
    nameEl.value?.focus()
  },
  { immediate: true }
)

const dayOn = (i: number) => ((mask.value >> i) & 1) === 1

function toggleDay(i: number) {
  mask.value ^= 1 << i
  error.value = null
}

const everyDay = computed(() => mask.value === ALL_DAYS)

async function save() {
  const trimmed = name.value.trim()
  if (!trimmed) {
    error.value = 'A habit needs a name.'
    return
  }
  if ((mask.value & ALL_DAYS) === 0) {
    error.value = 'Pick at least one weekday.'
    return
  }
  try {
    if (mode.value === 'create') {
      await habits.create(trimmed, mask.value)
    } else if (editing.value) {
      await habits.update(editing.value.id, { name: trimmed, daysMask: mask.value })
    }
    habits.closeEditor()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

async function togglePause() {
  if (!editing.value) return
  if (editing.value.isPaused) await habits.resume(editing.value.id)
  else await habits.pause(editing.value.id)
}

const armedDelete = ref(false)
watch(mode, () => (armedDelete.value = false))

async function del() {
  if (!editing.value) return
  if (!armedDelete.value) {
    armedDelete.value = true
    return
  }
  const id = editing.value.id
  armedDelete.value = false
  habits.closeEditor()
  await habits.remove(id)
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.stopPropagation()
    habits.closeEditor()
  }
}
</script>

<template>
  <div v-if="mode" class="qh-modal" @keydown="onKeydown" @click.self="habits.closeEditor()">
    <div class="qh-modal__panel" role="dialog" aria-modal="true">
      <header class="qh-modal__head">
        <h2 class="qh-modal__title">{{ mode === 'create' ? 'New habit' : 'Edit habit' }}</h2>
        <button class="qh-modal__x" title="Close" @click="habits.closeEditor()">×</button>
      </header>

      <label class="qh-modal__label" for="qh-modal-name">Name</label>
      <input
        id="qh-modal-name"
        ref="nameEl"
        v-model="name"
        class="qh-input"
        type="text"
        maxlength="80"
        :placeholder="mode === 'create' ? 'e.g. Read 20 minutes' : undefined"
        @keydown.enter.prevent="save"
      />

      <div class="qh-modal__label-row">
        <span class="qh-modal__label">Tracked on</span>
        <button
          v-if="!everyDay"
          class="qh-modal__minor"
          @click="mask = ALL_DAYS"
        >every day</button>
      </div>
      <div class="qh-modal__days">
        <button
          v-for="(label, i) in WEEKDAYS"
          :key="label"
          class="qh-modal__daybtn"
          :class="{ 'is-on': dayOn(i) }"
          :aria-pressed="dayOn(i)"
          @click="toggleDay(i)"
        >
          {{ label }}
        </button>
      </div>
      <p class="qh-modal__hint">
        Days outside the schedule don't count, can't be checked, and don't
        break a streak.
      </p>

      <p v-if="mode === 'create'" class="qh-modal__hint">
        The habit starts today — earlier days stay blank.
      </p>

      <template v-if="mode === 'edit' && editing">
        <div class="qh-modal__split" />
        <div class="qh-modal__life">
          <button class="qh-btn" @click="togglePause">
            <svg v-if="editing.isPaused" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 4l14 8-14 8z" /></svg>
            <svg v-else width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M8 5v14 M16 5v14" /></svg>
            {{ editing.isPaused ? 'Resume' : 'Pause' }}
          </button>
          <span class="qh-modal__life-hint">
            {{ editing.isPaused
              ? 'Paused — resume to make today trackable again.'
              : 'Pausing keeps the history; paused days don\'t count.' }}
          </span>
          <button
            class="qh-btn qh-modal__danger"
            :class="{ 'is-armed': armedDelete }"
            @mouseleave="armedDelete = false"
            @click="del"
          >
            {{ armedDelete ? 'Really delete?' : 'Delete' }}
          </button>
        </div>
      </template>

      <p v-if="error" class="qh-modal__error" role="alert">{{ error }}</p>

      <footer class="qh-modal__foot">
        <button class="qh-btn" @click="habits.closeEditor()">Cancel</button>
        <button class="qh-btn qh-modal__save" @click="save">
          {{ mode === 'create' ? 'Create' : 'Save' }}
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.qh-modal {
  position: fixed;
  inset: 0;
  z-index: 40;
  display: grid;
  place-items: center;
  background: color-mix(in srgb, #000 55%, transparent);
}

.qh-modal__panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: min(380px, calc(100% - 32px));
  padding: 16px;
  border: 1px solid var(--qh-border);
  border-radius: var(--qh-radius-lg);
  background: var(--qh-bg-raised);
  box-shadow: 0 18px 48px rgb(0 0 0 / 0.45);
}

.qh-modal__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.qh-modal__title {
  margin: 0;
  color: var(--qh-text);
  font-size: 14px;
  font-weight: 700;
}

.qh-modal__x {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  border: none;
  border-radius: var(--qh-radius);
  background: transparent;
  color: var(--qh-text-muted);
  font-size: 16px;
  line-height: 1;
  cursor: pointer;
}

.qh-modal__x:hover {
  background: var(--qh-bg-hover);
  color: var(--qh-text);
}

.qh-modal__label {
  color: var(--qh-text-muted);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.07em;
  text-transform: uppercase;
}

.qh-modal__label-row {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
  margin-top: 4px;
}

.qh-modal__minor {
  padding: 0;
  border: none;
  background: none;
  color: var(--qh-text-muted);
  font-size: 10px;
  text-decoration: underline;
  cursor: pointer;
}

.qh-modal__minor:hover {
  color: var(--qh-text);
}

.qh-modal__days {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 4px;
}

.qh-modal__daybtn {
  padding: 6px 0;
  border: 1px solid var(--qh-border-subtle);
  border-radius: var(--qh-radius);
  background: var(--qh-bg-input);
  color: var(--qh-text-muted);
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  transition: background 100ms ease, border-color 100ms ease, color 100ms ease;
}

.qh-modal__daybtn:hover {
  border-color: var(--qh-accent);
}

.qh-modal__daybtn.is-on {
  border-color: var(--qh-accent);
  background: var(--qh-bg-card);
  color: var(--qh-text);
}

.qh-modal__hint {
  margin: 0;
  color: var(--qh-text-muted);
  font-size: 11px;
  line-height: 1.5;
}

.qh-modal__split {
  height: 1px;
  margin: 4px 0;
  background: var(--qh-border-subtle);
}

.qh-modal__life {
  display: flex;
  align-items: center;
  gap: 8px;
}

.qh-modal__life-hint {
  flex: 1;
  min-width: 0;
  color: var(--qh-text-muted);
  font-size: 10px;
  line-height: 1.4;
}

.qh-modal__danger {
  flex-shrink: 0;
  color: var(--qh-text-muted);
}

.qh-modal__danger:hover,
.qh-modal__danger.is-armed {
  border-color: color-mix(in srgb, var(--qh-danger) 60%, var(--qh-border));
  color: var(--qh-danger);
}

.qh-modal__error {
  margin: 0;
  color: var(--qh-danger);
  font-size: 11px;
}

.qh-modal__foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 4px;
}

.qh-modal__save {
  border-color: var(--qh-accent);
  color: var(--qh-text);
}

.qh-modal__save:hover {
  border-color: var(--qh-text);
}
</style>
