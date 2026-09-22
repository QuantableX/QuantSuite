<script setup lang="ts">
/**
 * The RRULE builder — a preset dropdown that opens into the parts the crate
 * actually supports (FREQ, INTERVAL, BYDAY, COUNT/UNTIL).
 *
 * A free-text RRULE field would be a trap: the crate rejects what it cannot
 * expand, and the user has no way to know which half of RFC 5545 that is.
 */
const model = defineModel<string | null>({ default: null })

const props = defineProps<{ start: Date }>()

type Freq = 'DAILY' | 'WEEKLY' | 'MONTHLY' | 'YEARLY'

const WEEKDAYS: Array<{ code: string; label: string; index: number }> = [
  { code: 'MO', label: 'M', index: 1 },
  { code: 'TU', label: 'T', index: 2 },
  { code: 'WE', label: 'W', index: 3 },
  { code: 'TH', label: 'T', index: 4 },
  { code: 'FR', label: 'F', index: 5 },
  { code: 'SA', label: 'S', index: 6 },
  { code: 'SU', label: 'S', index: 0 },
]

const repeats = ref(false)
const freq = ref<Freq>('WEEKLY')
const interval = ref(1)
const byday = ref<string[]>([])
const endMode = ref<'never' | 'count' | 'until'>('never')
const count = ref(10)
const until = ref('')

/** Read an existing rule back into the controls. */
function hydrate(raw: string | null) {
  if (!raw) {
    repeats.value = false
    byday.value = [WEEKDAYS.find((d) => d.index === props.start.getDay())?.code ?? 'MO']
    return
  }
  repeats.value = true
  for (const part of raw.replace(/^RRULE:/, '').split(';')) {
    const [key, value] = part.split('=')
    if (!key || !value) continue
    switch (key.toUpperCase()) {
      case 'FREQ':
        freq.value = value.toUpperCase() as Freq
        break
      case 'INTERVAL':
        interval.value = Number(value) || 1
        break
      case 'BYDAY':
        byday.value = value.split(',')
        break
      case 'COUNT':
        endMode.value = 'count'
        count.value = Number(value) || 10
        break
      case 'UNTIL': {
        endMode.value = 'until'
        const digits = value.slice(0, 8)
        until.value = `${digits.slice(0, 4)}-${digits.slice(4, 6)}-${digits.slice(6, 8)}`
        break
      }
    }
  }
}

function build(): string | null {
  if (!repeats.value) return null
  const parts = [`FREQ=${freq.value}`]
  if (interval.value > 1) parts.push(`INTERVAL=${interval.value}`)
  if (freq.value === 'WEEKLY' && byday.value.length) parts.push(`BYDAY=${byday.value.join(',')}`)
  if (endMode.value === 'count') parts.push(`COUNT=${Math.max(1, count.value)}`)
  if (endMode.value === 'until' && until.value) parts.push(`UNTIL=${until.value.replace(/-/g, '')}`)
  return parts.join(';')
}

// Rebuild on every control change, so the parent never has to ask.
watch([repeats, freq, interval, byday, endMode, count, until], () => {
  model.value = build()
}, { deep: true })

watch(() => model.value, (raw) => {
  if (raw !== build()) hydrate(raw)
}, { immediate: true })

function toggleDay(code: string) {
  const at = byday.value.indexOf(code)
  if (at === -1) byday.value = [...byday.value, code]
  else byday.value = byday.value.filter((d) => d !== code)
}

const summary = computed(() => {
  if (!repeats.value) return 'Does not repeat'
  const every = interval.value > 1 ? `every ${interval.value} ` : ''
  const unit =
    freq.value === 'DAILY' ? 'days' : freq.value === 'WEEKLY' ? 'weeks' : freq.value === 'MONTHLY' ? 'months' : 'years'
  const days = freq.value === 'WEEKLY' && byday.value.length ? ` on ${byday.value.join(', ')}` : ''
  const tail =
    endMode.value === 'count' ? `, ${count.value} times` : endMode.value === 'until' && until.value ? `, until ${until.value}` : ''
  return `Repeats ${every}${unit}${days}${tail}`
})

defineExpose({ build })
</script>

<template>
  <div class="qp-rf">
    <label class="qp-rf__toggle">
      <input v-model="repeats" type="checkbox" />
      <span>{{ summary }}</span>
    </label>

    <div v-if="repeats" class="qp-rf__body">
      <div class="qp-rf__row">
        <span class="qp-rf__label">Every</span>
        <input v-model.number="interval" class="qp-input qp-rf__num" type="number" min="1" max="99" />
        <select v-model="freq" class="qp-select">
          <option value="DAILY">day(s)</option>
          <option value="WEEKLY">week(s)</option>
          <option value="MONTHLY">month(s)</option>
          <option value="YEARLY">year(s)</option>
        </select>
      </div>

      <div v-if="freq === 'WEEKLY'" class="qp-rf__days">
        <button
          v-for="day in WEEKDAYS"
          :key="day.code"
          type="button"
          class="qp-rf__day"
          :class="{ 'is-on': byday.includes(day.code) }"
          :aria-label="day.code"
          @click="toggleDay(day.code)"
        >
          {{ day.label }}
        </button>
      </div>

      <div class="qp-rf__row">
        <span class="qp-rf__label">Ends</span>
        <select v-model="endMode" class="qp-select">
          <option value="never">Never</option>
          <option value="count">After N times</option>
          <option value="until">On a date</option>
        </select>
        <input v-if="endMode === 'count'" v-model.number="count" class="qp-input qp-rf__num" type="number" min="1" />
        <input v-if="endMode === 'until'" v-model="until" class="qp-input" type="date" />
      </div>

      <p v-if="freq === 'MONTHLY'" class="qp-rf__note">
        Monthly repeats on day {{ start.getDate() }}. Months without that day are skipped.
      </p>
    </div>
  </div>
</template>

<style scoped>
.qp-rf {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.qp-rf__toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  color: var(--qp-text-secondary);
  font-size: 12px;
  cursor: pointer;
}

.qp-rf__body {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  border: 1px solid var(--qp-border-subtle);
  border-radius: var(--qp-radius);
  background: var(--qp-bg-raised);
}

.qp-rf__row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}

/* Flex items default to `min-width: auto`, and a native date input's intrinsic
   width is wide enough to push the dialog sideways without this. */
.qp-rf__row > .qp-input,
.qp-rf__row > .qp-select {
  min-width: 0;
}

.qp-rf__label {
  flex-shrink: 0;
  width: 44px;
  color: var(--qp-text-muted);
  font-size: 11px;
}

.qp-rf__num {
  flex: 0 1 64px;
}

.qp-rf__days {
  display: flex;
  flex-wrap: wrap;
  gap: 3px;
  padding-left: 52px;
}

.qp-rf__day {
  width: 26px;
  height: 26px;
  border: 1px solid var(--qp-border);
  border-radius: 999px;
  background: transparent;
  color: var(--qp-text-muted);
  font-size: 11px;
  cursor: pointer;
}

.qp-rf__day.is-on {
  border-color: var(--qp-accent);
  background: var(--qp-bg-card);
  color: var(--qp-text);
}

.qp-rf__note {
  margin: 0;
  color: var(--qp-text-muted);
  font-size: 11px;
  line-height: 1.4;
}
</style>
