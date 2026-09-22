<script setup lang="ts">
import {
  ArrowLeft, ChevronRight, ChevronLeft, BookOpen, HelpCircle,
  Swords, Dumbbell, Clock, BarChart3, Check, Zap,
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

const isCompleted = computed(() => dojo.progress.completedLessons.includes(props.lessonId))
const justCompleted = ref(false)

// Navigation
const moduleLessons = computed(() => module_.value?.lessons ?? [])
const currentIndex = computed(() => moduleLessons.value.findIndex(l => l.id === props.lessonId))
const prevLesson = computed(() => currentIndex.value > 0 ? moduleLessons.value[currentIndex.value - 1] : null)
const nextLesson = computed(() => currentIndex.value < moduleLessons.value.length - 1 ? moduleLessons.value[currentIndex.value + 1] : null)

const difficultyColors: Record<string, string> = {
  beginner: 'var(--positive)',
  intermediate: 'var(--warning)',
  advanced: 'var(--negative)',
}

const typeIcons: Record<string, any> = {
  theory: BookOpen,
  quiz: HelpCircle,
  challenge: Swords,
  exercise: Dumbbell,
}

const xpMap: Record<string, number> = {
  theory: 10,
  exercise: 15,
  quiz: 25,
  challenge: 50,
}

function markComplete() {
  if (!isCompleted.value) {
    dojo.completeLesson(props.lessonId)
    justCompleted.value = true
    setTimeout(() => { justCompleted.value = false }, 2000)
  }
}
</script>

<template>
  <div class="space-y-3" v-if="lesson && track && module_">
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
      <span class="text-[11px]" style="color: var(--text-secondary)">{{ module_.name }}</span>
    </div>

    <!-- Lesson header -->
    <div class="card p-4">
      <div class="flex items-start gap-3">
        <div
          class="p-2.5 rounded-lg flex-shrink-0"
          :style="{ backgroundColor: track.color + '18' }"
        >
          <component :is="typeIcons[lesson.type]" class="w-5 h-5" :style="{ color: track.color }" />
        </div>
        <div class="flex-1 min-w-0">
          <h2 class="text-sm font-bold" style="color: var(--text-primary)">{{ lesson.title }}</h2>
          <p class="text-[11px] mt-0.5" style="color: var(--text-secondary)">{{ lesson.description }}</p>
          <div class="flex items-center gap-3 mt-2 flex-wrap">
            <span
              class="text-[9px] px-1.5 py-0.5 rounded font-semibold uppercase"
              :style="{ backgroundColor: track.color + '18', color: track.color }"
            >
              {{ lesson.type }}
            </span>
            <span
              class="text-[9px] px-1.5 py-0.5 rounded font-semibold uppercase"
              :style="{ backgroundColor: difficultyColors[lesson.difficulty] + '18', color: difficultyColors[lesson.difficulty] }"
            >
              {{ lesson.difficulty }}
            </span>
            <span class="text-[10px] flex items-center gap-1" style="color: var(--muted)">
              <Clock class="w-3 h-3" />
              {{ lesson.estimatedMinutes }} min
            </span>
            <span class="text-[10px] flex items-center gap-1" style="color: var(--accent)">
              <Zap class="w-3 h-3" />
              +{{ xpMap[lesson.type] }} XP
            </span>
            <span v-if="isCompleted" class="text-[10px] flex items-center gap-1" style="color: var(--positive)">
              <Check class="w-3 h-3" />
              Completed
            </span>
          </div>
        </div>
      </div>
    </div>

    <!-- Content -->
    <div class="card p-4 lesson-content" v-html="lesson.content" />

    <!-- Complete button -->
    <div class="card p-3 flex items-center justify-between" v-if="lesson.type === 'theory' || lesson.type === 'exercise'">
      <div v-if="justCompleted" class="flex items-center gap-2">
        <Check class="w-4 h-4" style="color: var(--positive)" />
        <span class="text-xs font-medium" style="color: var(--positive)">+{{ xpMap[lesson.type] }} XP earned!</span>
      </div>
      <div v-else-if="isCompleted" class="flex items-center gap-2">
        <Check class="w-4 h-4" style="color: var(--positive)" />
        <span class="text-xs" style="color: var(--text-secondary)">Lesson completed</span>
      </div>
      <div v-else />
      <button
        v-if="!isCompleted"
        class="px-4 py-2 rounded-md text-xs font-semibold text-white transition-colors"
        :style="{ backgroundColor: track.color }"
        @click="markComplete"
      >
        Mark as Complete
      </button>
    </div>

    <!-- Navigation -->
    <div class="flex items-center justify-between">
      <button
        v-if="prevLesson"
        class="card px-3 py-2 flex items-center gap-2 text-[11px] font-medium transition-colors hover:brightness-110"
        style="color: var(--text-secondary)"
        @click="emit('select-lesson', prevLesson.id)"
      >
        <ChevronLeft class="w-3.5 h-3.5" />
        {{ prevLesson.title }}
      </button>
      <div v-else />
      <button
        v-if="nextLesson"
        class="card px-3 py-2 flex items-center gap-2 text-[11px] font-medium transition-colors hover:brightness-110"
        style="color: var(--text-secondary)"
        @click="emit('select-lesson', nextLesson.id)"
      >
        {{ nextLesson.title }}
        <ChevronRight class="w-3.5 h-3.5" />
      </button>
    </div>
  </div>
</template>

<style>
.lesson-content h3 {
  font-size: 14px;
  font-weight: 700;
  color: var(--text-primary);
  margin-bottom: 8px;
}

.lesson-content h4 {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-primary);
  margin-top: 16px;
  margin-bottom: 6px;
}

.lesson-content p {
  font-size: 12px;
  line-height: 1.7;
  color: var(--text-secondary);
  margin-bottom: 10px;
}

.lesson-content strong {
  color: var(--text-primary);
  font-weight: 600;
}

.lesson-content code {
  font-family: 'JetBrains Mono', 'Fira Code', monospace;
  font-size: 11px;
  background-color: var(--surface-3);
  color: var(--accent);
  padding: 1px 5px;
  border-radius: 3px;
}

.lesson-content .concept-box {
  background-color: var(--accent);
  background-color: rgba(99, 102, 241, 0.08);
  border-left: 3px solid var(--accent);
  padding: 10px 12px;
  border-radius: 0 6px 6px 0;
  margin: 12px 0;
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-secondary);
}

.lesson-content .concept-box strong {
  color: var(--accent);
}

.lesson-content .example-box {
  background-color: rgba(8, 153, 129, 0.06);
  border-left: 3px solid var(--positive);
  padding: 10px 12px;
  border-radius: 0 6px 6px 0;
  margin: 12px 0;
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-secondary);
}

.lesson-content .example-box strong {
  color: var(--positive);
}
</style>
