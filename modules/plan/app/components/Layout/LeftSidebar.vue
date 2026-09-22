<script setup lang="ts">
/**
 * The sidebar: create, the mini month, and the calendar list.
 *
 * Toggling a calendar hides its events everywhere at once — the crate filters
 * on `is_visible`, so the grid never has to know about visibility itself.
 */
import { useCalendarsStore } from '#plan/stores/calendars'
import type { CalendarColor } from '#plan/types'

const emit = defineEmits<{ (e: 'create'): void }>()

const calendars = useCalendarsStore()

/** The calendar whose row is open for editing, and the one awaiting a second
 * click on delete. */
const editing = ref<string | null>(null)
const confirming = ref<string | null>(null)
const error = ref('')

/** The calendar the dialog is asking about, or null when it is closed. */
const pendingDelete = computed(() => calendars.list.find((c) => c.id === confirming.value) ?? null)

function onKey(event: KeyboardEvent) {
  if (event.key === 'Escape') confirming.value = null
}

// Only while the dialog is up: a global Escape handler that outlives it would
// swallow the key for the rest of the module.
watch(confirming, (open) => {
  if (open) window.addEventListener('keydown', onKey)
  else window.removeEventListener('keydown', onKey)
})

onBeforeUnmount(() => window.removeEventListener('keydown', onKey))

function toggleEdit(id: string) {
  editing.value = editing.value === id ? null : id
  confirming.value = null
  error.value = ''
}

function commitName(cal: { id: string; name: string }, event: Event) {
  const value = (event.target as HTMLInputElement).value.trim()
  if (!value || value === cal.name) return
  void calendars.update(cal.id, { name: value })
}

async function removeCalendar(id: string) {
  confirming.value = null
  error.value = ''
  try {
    await calendars.remove(id)
    editing.value = null
  } catch (e) {
    // The crate refuses the last one — an event needs somewhere to live.
    error.value = String(e)
  }
}

const adding = ref(false)
const newName = ref('')
const COLORS: CalendarColor[] = [
  // Two rows of seven, warm across the top and cool below — the grid's column
  // count and this length are tied together; change one and change the other.
  'red', 'orange', 'amber', 'lime', 'green', 'mint', 'teal',
  'cyan', 'blue', 'indigo', 'purple', 'pink', 'brown', 'slate',
]
const newColor = ref<CalendarColor>('green')

async function addCalendar() {
  const name = newName.value.trim()
  if (!name) return
  await calendars.create(name, newColor.value)
  newName.value = ''
  adding.value = false
}
</script>

<template>
  <div class="qp-ls">
    <button class="qp-ls__create" @click="emit('create')">
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true">
        <path d="M12 5v14 M5 12h14" />
      </svg>
      <span>Create</span>
    </button>

    <PlanGridMiniMonth />

    <section class="qp-ls__block">
      <header class="qp-ls__head">
        <span>Calendars</span>
        <button aria-label="Add calendar" title="Add calendar" @click="adding = !adding">+</button>
      </header>

      <div v-if="adding" class="qp-ls__new">
        <input
          v-model="newName"
          class="qp-input"
          placeholder="Calendar name"
          @keydown.enter.prevent="addCalendar"
          @keydown.esc="adding = false"
        />
        <div class="qp-ls__swatches">
          <button
            v-for="c in COLORS"
            :key="c"
            class="qp-dot qp-ls__swatch"
            :data-color="c"
            :class="{ 'is-on': newColor === c }"
            :aria-label="c"
            @click="newColor = c"
          />
        </div>
        <button class="qp-btn" @click="addCalendar">Add</button>
      </div>

      <div v-for="cal in calendars.list" :key="cal.id" class="qp-ls__cal-wrap">
        <div class="qp-ls__cal" :class="{ 'is-editing': editing === cal.id }">
          <input
            :id="`cal-${cal.id}`"
            type="checkbox"
            :checked="cal.isVisible"
            :aria-label="`Show ${cal.name}`"
            @change="calendars.update(cal.id, { isVisible: ($event.target as HTMLInputElement).checked })"
          />
          <span class="qp-dot" :data-color="cal.color" />

          <input
            v-if="editing === cal.id"
            class="qp-input qp-ls__cal-input"
            :value="cal.name"
            placeholder="Calendar name"
            @blur="commitName(cal, $event)"
            @keydown.enter.prevent="($event.target as HTMLInputElement).blur()"
          />
          <label v-else class="qp-ls__cal-name" :for="`cal-${cal.id}`">{{ cal.name }}</label>

          <button
            class="qp-ls__pill"
            :aria-label="editing === cal.id ? `Done editing ${cal.name}` : `Edit ${cal.name}`"
            @click="toggleEdit(cal.id)"
          >
            {{ editing === cal.id ? 'done' : 'edit' }}
          </button>
        </div>

        <div v-if="editing === cal.id" class="qp-ls__cal-edit">
          <div class="qp-ls__swatches">
            <button
              v-for="c in COLORS"
              :key="c"
              class="qp-dot qp-ls__swatch"
              :data-color="c"
              :class="{ 'is-on': cal.color === c }"
              :aria-label="c"
              @click="calendars.update(cal.id, { color: c })"
            />
          </div>

          <!-- Deleting a calendar takes its events with it, so it asks first
               — in a dialog, because the question does not fit a 220px rail. -->
          <button class="qp-ls__trash" :aria-label="`Delete ${cal.name}`" @click="confirming = cal.id">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M4 7h16 M9 7V5h6v2 M6 7l1 13h10l1-13" />
            </svg>
          </button>
        </div>

      </div>
    </section>

    <Teleport to="body">
      <div v-if="pendingDelete" class="qp-ls__scrim" @click.self="confirming = null">
        <div class="qp-ls__dialog" role="alertdialog" aria-labelledby="qp-del-title">
          <h2 id="qp-del-title">Delete “{{ pendingDelete.name }}”?</h2>
          <p>Its events go with it. This cannot be undone.</p>
          <p v-if="error" class="qp-ls__dialog-error">{{ error }}</p>
          <footer>
            <button class="qp-btn" @click="confirming = null">Cancel</button>
            <button class="qp-btn qp-ls__dialog-danger" @click="removeCalendar(pendingDelete.id)">Delete</button>
          </footer>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.qp-ls {
  height: 100%;
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 10px 8px;
  overflow-y: auto;
}

.qp-ls__create {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  margin-bottom: 12px;
  padding: 8px;
  border: 1px solid var(--qp-border);
  border-radius: var(--qp-radius-lg);
  background: var(--qp-bg-card);
  color: var(--qp-text);
  font-size: 13px;
  cursor: pointer;
}

.qp-ls__create:hover {
  border-color: var(--qp-accent);
  background: var(--qp-bg-hover);
}

.qp-ls__block {
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--qp-border-subtle);
}

.qp-ls__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 4px 6px;
  color: var(--qp-text-muted);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
}

.qp-ls__head button {
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qp-text-muted);
  font-size: 14px;
  line-height: 1;
  cursor: pointer;
}

.qp-ls__head button:hover {
  background: var(--qp-bg-hover);
  color: var(--qp-text);
}

.qp-ls__new {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 8px;
  padding: 8px;
  border: 1px solid var(--qp-border-subtle);
  border-radius: var(--qp-radius);
  background: var(--qp-bg-raised);
}

/* Fourteen colours as a fixed 7×2 grid. Left to wrap on available width they
   came out seven-and-one, which reads as a mistake rather than a palette. The
   column count and COLORS.length / 2 are tied — change one, change the other. */
.qp-ls__swatches {
  display: grid;
  grid-template-columns: repeat(7, 16px);
  gap: 4px;
}

.qp-ls__swatch {
  width: 16px;
  height: 16px;
  border: none;
  border-radius: 5px;
  cursor: pointer;
}

.qp-ls__swatch.is-on {
  outline: 2px solid var(--qp-accent);
  outline-offset: 1px;
}

.qp-ls__cal-wrap + .qp-ls__cal-wrap {
  margin-top: 1px;
}

.qp-ls__cal {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 6px;
  border-radius: var(--qp-radius);
  color: var(--qp-text-secondary);
  font-size: 12px;
}

.qp-ls__cal:hover,
.qp-ls__cal.is-editing {
  background: var(--qp-bg-hover);
  color: var(--qp-text);
}

.qp-ls__cal-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  padding: 2px 6px;
  border: 1px solid transparent;
  border-radius: var(--qp-radius);
  font-size: 12px;
  line-height: 1.5;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: pointer;
}

/* The same box as the static name — without the transparent border and the
   padding the label jumps sideways the moment edit mode opens. */
.qp-ls__cal-input {
  flex: 1;
  min-width: 0;
  padding: 2px 6px;
  border-color: var(--qp-border);
  background: var(--qp-bg-input);
  font-size: 12px;
  line-height: 1.5;
}

.qp-ls__pill {
  flex-shrink: 0;
  min-width: 38px;
  padding: 1px 5px;
  border: 1px solid var(--qp-border);
  border-radius: 999px;
  background: transparent;
  color: var(--qp-text-muted);
  font-family: inherit;
  font-size: 9px;
  letter-spacing: 0.05em;
  text-align: center;
  text-transform: uppercase;
  cursor: pointer;
  /* Hidden until wanted: eight of these down a 220px rail is noise, and the
     row's job is showing which calendars are on. */
  opacity: 0;
  transition: opacity 120ms ease, border-color 120ms ease, color 120ms ease;
}

.qp-ls__cal:hover .qp-ls__pill,
.qp-ls__cal.is-editing .qp-ls__pill,
.qp-ls__pill:focus-visible {
  opacity: 1;
}

.qp-ls__pill:hover {
  border-color: var(--qp-accent);
  color: var(--qp-text);
}

.qp-ls__cal-edit {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin: 2px 0 6px;
  padding: 8px;
  border: 1px solid var(--qp-border-subtle);
  border-radius: var(--qp-radius);
  background: var(--qp-bg-raised);
}

.qp-ls__trash {
  flex-shrink: 0;
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qp-text-muted);
  cursor: pointer;
}

.qp-ls__trash:hover {
  background: var(--qp-bg-card);
  color: var(--qss-error, #ff4757);
}

/* Teleported to body, so it covers the module instead of being clipped inside
   the 220px rail — the one place this module uses fixed rather than absolute
   positioning, and it is outside the stage's containing block. */
.qp-ls__scrim {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: grid;
  place-items: center;
  background: rgba(0, 0, 0, 0.5);
}

.qp-ls__dialog {
  width: min(360px, calc(100vw - 32px));
  padding: 18px;
  border: 1px solid var(--qp-border);
  border-radius: var(--qp-radius-lg);
  background: var(--qp-bg-raised);
  box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
}

.qp-ls__dialog h2 {
  margin: 0 0 8px;
  color: var(--qp-text);
  font-size: 15px;
  font-weight: 600;
}

.qp-ls__dialog p {
  margin: 0 0 14px;
  color: var(--qp-text-secondary);
  font-size: 12px;
  line-height: 1.5;
}

.qp-ls__dialog-error {
  color: var(--qss-error, #ff4757) !important;
}

.qp-ls__dialog footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

.qp-ls__dialog-danger {
  border-color: var(--qss-error, #ff4757);
  color: var(--qss-error, #ff4757);
}

.qp-ls__dialog-danger:hover {
  background: color-mix(in srgb, var(--qss-error, #ff4757) 14%, transparent);
  border-color: var(--qss-error, #ff4757);
  color: var(--qss-error, #ff4757);
}
</style>
