<script setup lang="ts">
import {
  ChevronRight, Swords, Zap, Trophy, RotateCcw, Star,
} from 'lucide-vue-next'
import { useDojoStore } from '#terminal/stores/dojo'
import type { Lesson, Track, Module } from '#terminal/stores/dojo'

const props = defineProps<{
  lessonId: string
}>()

const emit = defineEmits<{
  (e: 'back'): void
  (e: 'back-to-overview'): void
  (e: 'select-lesson', lessonId: string): void
}>()

const dojo = useDojoStore()

const lesson = computed(() => dojo.allLessons.find(l => l.id === props.lessonId) as Lesson)
const track = computed(() => dojo.tracks.find(t => t.id === lesson.value?.trackId) as Track)
const module_ = computed(() => track.value?.modules.find(m => m.id === lesson.value?.moduleId) as Module)
const challenge = computed(() => lesson.value?.challenge)

const selectedOption = ref<number | null>(null)
const submitted = ref(false)
const chosenOutcome = computed(() => {
  if (selectedOption.value === null || !challenge.value) return null
  return challenge.value.options[selectedOption.value]
})

function submit(idx: number) {
  if (submitted.value) return
  selectedOption.value = idx
  submitted.value = true
  const score = challenge.value?.options[idx]?.score ?? 0
  dojo.submitChallenge(props.lessonId, score)
}

function retry() {
  selectedOption.value = null
  submitted.value = false
}

// Navigation
const moduleLessons = computed(() => module_.value?.lessons ?? [])
const currentIndex = computed(() => moduleLessons.value.findIndex(l => l.id === props.lessonId))
const nextLesson = computed(() => currentIndex.value < moduleLessons.value.length - 1 ? moduleLessons.value[currentIndex.value + 1] : null)

function getScoreColor(score: number): string {
  if (score >= 40) return 'var(--positive)'
  if (score >= 25) return 'var(--warning)'
  return 'var(--negative)'
}

function getStars(score: number): number {
  if (score >= 45) return 3
  if (score >= 30) return 2
  if (score >= 15) return 1
  return 0
}
</script>

<template>
  <div v-if="lesson && track && module_ && challenge" class="dojo-challenge">
    <!-- Breadcrumb -->
    <div class="flex items-center gap-1.5 flex-wrap">
      <button
        class="text-[12px] font-medium transition-colors hover:brightness-125"
        style="color: var(--accent)"
        @click="emit('back-to-overview')"
      >
        Dojo
      </button>
      <ChevronRight class="w-3 h-3" style="color: var(--muted)" />
      <button
        class="text-[12px] font-medium transition-colors hover:brightness-125"
        style="color: var(--text-secondary)"
        @click="emit('back')"
      >
        {{ track.name }}
      </button>
      <ChevronRight class="w-3 h-3" style="color: var(--muted)" />
      <span class="text-[12px]" style="color: var(--text-secondary)">{{ lesson.title }}</span>
    </div>

    <!-- Challenge header -->
    <div class="card dojo-challenge-head">
      <Swords class="w-4 h-4 flex-shrink-0" style="color: var(--text-secondary)" />
      <div class="min-w-0">
        <h2>{{ challenge.title }}</h2>
        <p>{{ challenge.description }}</p>
      </div>
    </div>

    <div class="dojo-challenge-body">
    <!-- Scenario -->
    <div class="card p-3">
      <h4>Scenario</h4>
      <p class="text-[12px] leading-relaxed whitespace-pre-line" style="color: var(--text-secondary)">{{ challenge.scenario }}</p>
    </div>

    <!-- Outcome display (after submission) -->
    <template v-if="submitted && chosenOutcome">
      <div class="card p-3">
        <div class="flex items-center justify-between mb-3">
          <h4 class="dojo-challenge-h">Outcome</h4>
          <div class="flex items-center gap-2">
            <div class="flex gap-0.5">
              <Star
                v-for="i in 3"
                :key="i"
                class="w-3.5 h-3.5"
                :style="{ color: i <= getStars(chosenOutcome.score) ? '#ffd700' : 'var(--surface-3)' }"
                :fill="i <= getStars(chosenOutcome.score) ? '#ffd700' : 'none'"
              />
            </div>
            <span class="text-[11px] font-bold" :style="{ color: getScoreColor(chosenOutcome.score) }">
              {{ chosenOutcome.score }}/50 XP
            </span>
          </div>
        </div>

        <div
          class="p-3 rounded-lg text-[11px] leading-relaxed mb-3"
          :style="{
            backgroundColor: getScoreColor(chosenOutcome.score) + '0a',
            borderLeft: '3px solid ' + getScoreColor(chosenOutcome.score),
            color: 'var(--text-secondary)',
          }"
        >
          <strong :style="{ color: 'var(--text-primary)' }">Your choice: </strong>{{ chosenOutcome.label }}
        </div>
        <p class="text-[11px] leading-relaxed" style="color: var(--text-secondary)">{{ chosenOutcome.outcome }}</p>

        <!-- All options summary -->
        <div class="mt-4 pt-3 border-t" style="border-color: var(--border)">
          <h5 class="dojo-challenge-h5">All Options</h5>
          <div class="space-y-1.5">
            <div
              v-for="(opt, idx) in challenge.options"
              :key="idx"
              class="flex items-center justify-between text-[12px] px-2 py-1 rounded"
              :style="{
                backgroundColor: idx === selectedOption ? getScoreColor(opt.score) + '0a' : 'transparent',
              }"
            >
              <span :style="{ color: idx === selectedOption ? 'var(--text-primary)' : 'var(--text-secondary)' }">
                {{ idx === selectedOption ? '> ' : '' }}{{ opt.label }}
              </span>
              <span class="font-semibold" :style="{ color: getScoreColor(opt.score) }">{{ opt.score }}/50</span>
            </div>
          </div>
        </div>

        <div class="mt-4 flex items-center justify-between">
          <button
            class="px-3 py-2 rounded-md text-[11px] font-medium flex items-center gap-1.5 transition-colors"
            style="background-color: var(--surface-3); color: var(--text-secondary)"
            @click="retry"
          >
            <RotateCcw class="w-3 h-3" />
            Try Again
          </button>
          <button
            v-if="nextLesson"
            class="btn-primary flex items-center gap-1.5"
            @click="emit('select-lesson', nextLesson.id)"
          >
            Next Lesson
            <ChevronRight class="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    </template>

    <!-- Options (before submission) -->
    <template v-else>
      <div class="card p-3">
        <h4>What do you do?</h4>
        <div class="space-y-2">
          <button
            v-for="(opt, idx) in challenge.options"
            :key="idx"
            class="w-full px-3 py-2 rounded-lg text-left text-[12px] transition-all border leading-relaxed"
            style="border-color: var(--border); color: var(--text-primary); background-color: var(--surface-1)"
            @click="submit(idx)"
          >
            {{ opt.label }}
          </button>
        </div>
      </div>
    </template>
  </div>
  </div>
</template>

<style scoped>
.dojo-challenge { display: flex; flex-direction: column; gap: var(--qss-gap); font-variant-numeric: tabular-nums; }
.dojo-challenge-head {
  display: flex;
  align-items: center;
  gap: var(--qss-gap);
  padding: var(--qss-gap-tight) var(--qss-panel-pad);
}
.dojo-challenge-head h2 { margin: 0; color: var(--text-primary); font-size: var(--qss-text-section); font-weight: 600; }
.dojo-challenge-head p { margin: 1px 0 0; color: var(--text-secondary); font-size: var(--qss-text-meta); }
/* The scenario next to what you decide. */
.dojo-challenge-body {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  align-items: start;
  gap: var(--qss-gap);
}
.dojo-challenge-body h4 { margin: 0 0 var(--qss-gap-tight); color: var(--text-primary); font-size: var(--qss-text-section); font-weight: 600; }
.dojo-challenge-body h4.dojo-challenge-h { margin: 0; }
.dojo-challenge-h5 { margin: 0 0 4px; color: var(--text-secondary); font-size: var(--qss-text-label); font-weight: 600; }
@container terminal-workspace (max-width: 760px) {
  .dojo-challenge-body { grid-template-columns: minmax(0, 1fr); }
}
</style>
