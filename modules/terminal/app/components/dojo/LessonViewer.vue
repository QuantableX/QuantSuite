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
  <div v-if="lesson && track && module_" class="dojo-lesson-view">
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
      <span class="text-[12px]" style="color: var(--text-secondary)">{{ module_.name }}</span>
    </div>

    <!-- Lesson header -->
    <div class="card dojo-lesson-head">
      <component :is="typeIcons[lesson.type]" class="w-4 h-4 flex-shrink-0" style="color: var(--text-secondary)" />
      <div class="flex-1 min-w-0">
        <h2>{{ lesson.title }}</h2>
        <p>{{ lesson.description }}</p>
      </div>
      <div class="dojo-lesson-tags">
        <span class="dojo-tag">{{ lesson.type }}</span>
        <span
          class="dojo-tag"
          :style="{ backgroundColor: difficultyColors[lesson.difficulty] + '18', color: difficultyColors[lesson.difficulty] }"
        >
          {{ lesson.difficulty }}
        </span>
        <span class="flex items-center gap-1">
          <Clock class="w-3 h-3" />
          {{ lesson.estimatedMinutes }} min
        </span>
        <span class="flex items-center gap-1">
          <Zap class="w-3 h-3" />
          +{{ xpMap[lesson.type] }} XP
        </span>
        <span v-if="isCompleted" class="flex items-center gap-1" style="color: var(--positive)">
          <Check class="w-3 h-3" />
          Completed
        </span>
      </div>
    </div>

    <!-- Content -->
    <div class="card lesson-content" v-html="lesson.content" />

    <!-- Previous, completion and next on one row -->
    <div class="dojo-lesson-foot">
      <button
        v-if="prevLesson"
        class="dojo-lesson-nav"
        :title="prevLesson.title"
        @click="emit('select-lesson', prevLesson.id)"
      >
        <ChevronLeft class="w-3.5 h-3.5 flex-shrink-0" />
        <span class="truncate">{{ prevLesson.title }}</span>
      </button>
      <span v-else />
      <div v-if="lesson.type === 'theory' || lesson.type === 'exercise'" class="dojo-lesson-complete">
        <div v-if="justCompleted" class="flex items-center gap-2">
          <Check class="w-4 h-4" style="color: var(--positive)" />
          <span class="text-xs font-medium" style="color: var(--positive)">+{{ xpMap[lesson.type] }} XP earned!</span>
        </div>
        <div v-else-if="isCompleted" class="flex items-center gap-2">
          <Check class="w-4 h-4" style="color: var(--positive)" />
          <span class="text-xs" style="color: var(--text-secondary)">Lesson completed</span>
        </div>
        <button
          v-if="!isCompleted"
          class="btn-primary"
          @click="markComplete"
        >
          Mark as Complete
        </button>
      </div>
      <span v-else />
      <button
        v-if="nextLesson"
        class="dojo-lesson-nav justify-self-end"
        :title="nextLesson.title"
        @click="emit('select-lesson', nextLesson.id)"
      >
        <span class="truncate">{{ nextLesson.title }}</span>
        <ChevronRight class="w-3.5 h-3.5 flex-shrink-0" />
      </button>
    </div>
  </div>
</template>

<style>
.dojo-lesson-view { display: flex; flex-direction: column; gap: var(--qss-gap); }
.dojo-lesson-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--qss-gap-tight) var(--qss-gap);
  padding: var(--qss-gap-tight) var(--qss-panel-pad);
}
.dojo-lesson-head h2 { margin: 0; color: var(--text-primary); font-size: var(--qss-text-section); font-weight: 600; }
.dojo-lesson-head p { margin: 1px 0 0; color: var(--text-secondary); font-size: var(--qss-text-meta); }
.dojo-lesson-tags {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px var(--qss-gap);
  color: var(--text-secondary);
  font-size: var(--qss-text-meta);
}
.dojo-tag {
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--surface-3);
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 500;
  text-transform: capitalize;
}
.dojo-lesson-foot {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: center;
  gap: var(--qss-gap);
  min-height: var(--qss-ctl-h);
}
.dojo-lesson-nav {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 100%;
  width: fit-content;
  height: var(--qss-ctl-h);
  padding: 0 10px;
  border: 1px solid var(--border);
  border-radius: var(--qss-ctl-radius);
  color: var(--text-secondary);
  font-size: var(--qss-ctl-font);
  font-weight: 500;
}
.dojo-lesson-nav:hover { background: var(--surface-2); color: var(--text-primary); }
.dojo-lesson-complete { display: flex; align-items: center; gap: var(--qss-gap); }

/* Prose keeps a reading measure; a wide page gets a second column instead of longer lines. */
.lesson-content {
  padding: var(--qss-panel-pad);
  columns: 2 52ch;
  column-gap: 32px;
}
.lesson-content .concept-box,
.lesson-content .example-box { break-inside: avoid; }

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
  font-family: var(--qss-font-mono);
  font-size: 11px;
  background-color: var(--surface-3);
  color: var(--accent);
  padding: 1px 5px;
  border-radius: 3px;
}

.lesson-content .concept-box {
  background-color: var(--surface-2);
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
