<script setup lang="ts">
/**
 * QuantHabit → General — entry in the unified settings modal (V3). The module
 * is small enough that the modal holds its whole configuration.
 */
import { useAppStore } from '#habit/stores/app'
import { useHabitsStore } from '#habit/stores/habits'
import type { Grouping } from '#habit/types'

const app = useAppStore()
const habits = useHabitsStore()

function setGrouping(event: Event) {
  app.setGrouping((event.target as HTMLSelectElement).value as Grouping)
}
</script>

<template>
  <div>
    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Row grouping</p>
        <p class="qsu-hint">How the year page groups its day rows.</p>
      </div>
      <select class="qsu-select" :value="app.grouping" @change="setGrouping">
        <option value="week">Calendar weeks</option>
        <option value="month">Months</option>
      </select>
    </div>

    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Habits</p>
        <p class="qsu-hint">
          {{ habits.habits.length }} habit(s) ·
          {{ habits.habits.filter((h) => h.isPaused).length }} paused
        </p>
      </div>
    </div>
  </div>
</template>
