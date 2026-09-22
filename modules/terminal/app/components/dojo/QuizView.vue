<script setup lang="ts">
import {
  ArrowLeft, ChevronRight, Check, X, RotateCcw, Zap, Trophy,
} from 'lucide-vue-next'
import { useDojoStore } from '#terminal/stores/dojo'
import type { Lesson, Track, Module, QuizQuestion } from '#terminal/stores/dojo'

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
const questions = computed(() => lesson.value?.quiz ?? [])

const currentQuestion = ref(0)
const selectedAnswer = ref<number | null>(null)
const answered = ref(false)
const answers = ref<{ questionId: string; selected: number; correct: boolean }[]>([])
const quizComplete = ref(false)

const currentQ = computed(() => questions.value[currentQuestion.value] as QuizQuestion | undefined)
const isCorrect = computed(() => selectedAnswer.value === currentQ.value?.correctIndex)
const score = computed(() => {
  const correct = answers.value.filter(a => a.correct).length
  return questions.value.length > 0 ? Math.round((correct / questions.value.length) * 100) : 0
})

function selectOption(idx: number) {
  if (answered.value) return
  selectedAnswer.value = idx
  answered.value = true

  answers.value.push({
    questionId: currentQ.value!.id,
    selected: idx,
    correct: idx === currentQ.value!.correctIndex,
  })
}

function nextQuestion() {
  if (currentQuestion.value < questions.value.length - 1) {
    currentQuestion.value++
    selectedAnswer.value = null
    answered.value = false
  } else {
    quizComplete.value = true
    dojo.submitQuiz(props.lessonId, score.value)
  }
}

function retry() {
  currentQuestion.value = 0
  selectedAnswer.value = null
  answered.value = false
  answers.value = []
  quizComplete.value = false
}

// Navigation
const moduleLessons = computed(() => module_.value?.lessons ?? [])
const currentIndex = computed(() => moduleLessons.value.findIndex(l => l.id === props.lessonId))
const nextLesson = computed(() => currentIndex.value < moduleLessons.value.length - 1 ? moduleLessons.value[currentIndex.value + 1] : null)
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
      <span class="text-[11px]" style="color: var(--text-secondary)">{{ lesson.title }}</span>
    </div>

    <!-- Quiz complete -->
    <template v-if="quizComplete">
      <div class="card p-6 text-center">
        <div
          class="w-14 h-14 rounded-full mx-auto flex items-center justify-center mb-3"
          :style="{
            backgroundColor: score >= 80 ? 'rgba(8,153,129,0.15)' : score >= 50 ? 'rgba(255,152,0,0.15)' : 'rgba(242,54,69,0.15)',
            color: score >= 80 ? 'var(--positive)' : score >= 50 ? 'var(--warning)' : 'var(--negative)',
          }"
        >
          <Trophy class="w-7 h-7" />
        </div>
        <h3 class="text-sm font-bold mb-1" style="color: var(--text-primary)">Quiz Complete!</h3>
        <div class="text-2xl font-bold mb-1" :style="{ color: score >= 80 ? 'var(--positive)' : score >= 50 ? 'var(--warning)' : 'var(--negative)' }">
          {{ score }}%
        </div>
        <p class="text-[11px] mb-1" style="color: var(--text-secondary)">
          {{ answers.filter(a => a.correct).length }} / {{ questions.length }} correct
        </p>
        <div class="flex items-center justify-center gap-1 text-[11px] mb-4" style="color: var(--accent)">
          <Zap class="w-3 h-3" />
          +{{ Math.round(25 * (score / 100)) }} XP earned
        </div>
        <div class="flex items-center justify-center gap-3">
          <button
            class="px-4 py-2 rounded-md text-xs font-medium flex items-center gap-1.5 transition-colors"
            style="background-color: var(--surface-3); color: var(--text-secondary)"
            @click="retry"
          >
            <RotateCcw class="w-3.5 h-3.5" />
            Retry
          </button>
          <button
            v-if="nextLesson"
            class="px-4 py-2 rounded-md text-xs font-semibold text-white flex items-center gap-1.5"
            :style="{ backgroundColor: track.color }"
            @click="emit('select-lesson', nextLesson.id)"
          >
            Next Lesson
            <ChevronRight class="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    </template>

    <!-- Quiz in progress -->
    <template v-else-if="currentQ">
      <!-- Progress -->
      <div class="card p-3">
        <div class="flex items-center justify-between mb-2">
          <span class="text-[11px] font-medium" style="color: var(--text-secondary)">
            Question {{ currentQuestion + 1 }} of {{ questions.length }}
          </span>
          <span class="text-[11px]" style="color: var(--muted)">
            {{ answers.filter(a => a.correct).length }} correct so far
          </span>
        </div>
        <div class="flex gap-1">
          <div
            v-for="(_, i) in questions"
            :key="i"
            class="flex-1 h-1.5 rounded-full transition-all"
            :style="{
              backgroundColor: i < answers.length
                ? (answers[i]?.correct ? 'var(--positive)' : 'var(--negative)')
                : i === currentQuestion
                  ? track.color
                  : 'var(--surface-3)',
            }"
          />
        </div>
      </div>

      <!-- Question -->
      <div class="card p-4">
        <h3 class="text-xs font-semibold mb-4" style="color: var(--text-primary)">
          {{ currentQ.question }}
        </h3>

        <!-- Options -->
        <div class="space-y-2">
          <button
            v-for="(option, idx) in currentQ.options"
            :key="idx"
            class="w-full p-3 rounded-lg text-left text-[11px] transition-all border"
            :style="{
              borderColor: answered
                ? idx === currentQ.correctIndex
                  ? 'var(--positive)'
                  : idx === selectedAnswer && !isCorrect
                    ? 'var(--negative)'
                    : 'var(--border)'
                : idx === selectedAnswer
                  ? track.color
                  : 'var(--border)',
              backgroundColor: answered
                ? idx === currentQ.correctIndex
                  ? 'rgba(8,153,129,0.08)'
                  : idx === selectedAnswer && !isCorrect
                    ? 'rgba(242,54,69,0.08)'
                    : 'var(--surface-1)'
                : 'var(--surface-1)',
              color: 'var(--text-primary)',
              cursor: answered ? 'default' : 'pointer',
            }"
            @click="selectOption(idx)"
          >
            <div class="flex items-center gap-2">
              <div
                class="w-5 h-5 rounded-full flex items-center justify-center flex-shrink-0 text-[10px] font-bold"
                :style="{
                  backgroundColor: answered && idx === currentQ.correctIndex
                    ? 'var(--positive)'
                    : answered && idx === selectedAnswer && !isCorrect
                      ? 'var(--negative)'
                      : 'var(--surface-3)',
                  color: answered && (idx === currentQ.correctIndex || (idx === selectedAnswer && !isCorrect))
                    ? 'white'
                    : 'var(--text-secondary)',
                }"
              >
                <Check v-if="answered && idx === currentQ.correctIndex" class="w-3 h-3" />
                <X v-else-if="answered && idx === selectedAnswer && !isCorrect" class="w-3 h-3" />
                <span v-else>{{ String.fromCharCode(65 + idx) }}</span>
              </div>
              <span>{{ option }}</span>
            </div>
          </button>
        </div>

        <!-- Explanation -->
        <div
          v-if="answered"
          class="mt-3 p-3 rounded-lg text-[11px] leading-relaxed"
          :style="{
            backgroundColor: isCorrect ? 'rgba(8,153,129,0.06)' : 'rgba(242,54,69,0.06)',
            borderLeft: '3px solid ' + (isCorrect ? 'var(--positive)' : 'var(--negative)'),
            color: 'var(--text-secondary)',
          }"
        >
          <strong :style="{ color: isCorrect ? 'var(--positive)' : 'var(--negative)' }">
            {{ isCorrect ? 'Correct!' : 'Incorrect' }}
          </strong>
          <span> {{ currentQ.explanation }}</span>
        </div>

        <!-- Next button -->
        <div v-if="answered" class="mt-3 flex justify-end">
          <button
            class="px-4 py-2 rounded-md text-xs font-semibold text-white"
            :style="{ backgroundColor: track.color }"
            @click="nextQuestion"
          >
            {{ currentQuestion < questions.length - 1 ? 'Next Question' : 'See Results' }}
          </button>
        </div>
      </div>
    </template>
  </div>
</template>
