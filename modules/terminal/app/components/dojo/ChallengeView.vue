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
  <div class="space-y-3" v-if="lesson && track && module_ && challenge">
    <!-- Breadcrumb -->
    <div class="flex items-center gap-1.5 flex-wrap">
      <button
        class="text-[11px] font-medium transition-colors hover:brightness-125"
        style="color: var(--accent)"
        @click="emit('back-to-overview')"
      >
        Dojo
      </button>
      <ChevronRight class="w-3 h-3" style="color: var(--muted)" />
      <button
        class="text-[11px] font-medium transition-colors hover:brightness-125"
        :style="{ color: track.color }"
        @click="emit('back')"
      >
        {{ track.name }}
      </button>
      <ChevronRight class="w-3 h-3" style="color: var(--muted)" />
      <span class="text-[11px]" style="color: var(--text-secondary)">{{ lesson.title }}</span>
    </div>

    <!-- Challenge header -->
    <div class="card p-4">
      <div class="flex items-center gap-3 mb-3">
        <div class="p-2.5 rounded-lg" :style="{ backgroundColor: track.color + '18' }">
          <Swords class="w-5 h-5" :style="{ color: track.color }" />
        </div>
        <div>
          <h2 class="text-sm font-bold" style="color: var(--text-primary)">{{ challenge.title }}</h2>
          <p class="text-[11px]" style="color: var(--text-secondary)">{{ challenge.description }}</p>
        </div>
      </div>
    </div>

    <!-- Scenario -->
    <div class="card p-4">
      <h4 class="text-[11px] font-semibold mb-2 uppercase tracking-wider" style="color: var(--accent)">Scenario</h4>
      <p class="text-[11px] leading-relaxed whitespace-pre-line" style="color: var(--text-secondary)">{{ challenge.scenario }}</p>
    </div>

    <!-- Outcome display (after submission) -->
    <template v-if="submitted && chosenOutcome">
      <div class="card p-4">
        <div class="flex items-center justify-between mb-3">
          <h4 class="text-[11px] font-semibold uppercase tracking-wider" style="color: var(--text-primary)">Outcome</h4>
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
          <h5 class="text-[10px] font-semibold mb-2 uppercase tracking-wider" style="color: var(--muted)">All Options</h5>
          <div class="space-y-1.5">
            <div
              v-for="(opt, idx) in challenge.options"
              :key="idx"
              class="flex items-center justify-between text-[10px] p-2 rounded"
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
            class="px-4 py-2 rounded-md text-[11px] font-semibold text-white flex items-center gap-1.5"
            :style="{ backgroundColor: track.color }"
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
      <div class="card p-4">
        <h4 class="text-[11px] font-semibold mb-3 uppercase tracking-wider" style="color: var(--text-primary)">What do you do?</h4>
        <div class="space-y-2">
          <button
            v-for="(opt, idx) in challenge.options"
            :key="idx"
            class="w-full p-3 rounded-lg text-left text-[11px] transition-all border leading-relaxed"
            style="border-color: var(--border); color: var(--text-primary); background-color: var(--surface-1)"
            @click="submit(idx)"
          >
            {{ opt.label }}
          </button>
        </div>
      </div>
    </template>
  </div>
</template>
