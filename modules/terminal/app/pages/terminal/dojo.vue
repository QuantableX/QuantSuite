<script setup lang="ts">
definePageMeta({ layout: 'terminal' })

import { Zap, Flame, BookOpen, Star } from 'lucide-vue-next'
import { useDojoStore } from '#terminal/stores/dojo'

const dojo = useDojoStore()

// The curriculum is a lazy chunk (see the store): fetch it when the page
// mounts, and again on re-activation — a no-op once loaded, a retry if the
// first fetch failed.
onMounted(() => {
  void dojo.loadCurriculum()
})

onActivated(() => {
  void dojo.loadCurriculum()
})

// View state: 'overview' | 'track' | 'lesson'
const viewMode = ref<'overview' | 'track' | 'lesson'>('overview')
const selectedTrackId = ref<string | null>(null)
const selectedLessonId = ref<string | null>(null)

function selectTrack(trackId: string) {
  selectedTrackId.value = trackId
  viewMode.value = 'track'
}

function selectLesson(lessonId: string) {
  selectedLessonId.value = lessonId
  viewMode.value = 'lesson'
}

function backToOverview() {
  viewMode.value = 'overview'
  selectedTrackId.value = null
  selectedLessonId.value = null
}

function backToTrack() {
  viewMode.value = 'track'
  selectedLessonId.value = null
}

// Determine lesson type for rendering
const currentLesson = computed(() => {
  if (!selectedLessonId.value) return null
  return dojo.allLessons.find(l => l.id === selectedLessonId.value) ?? null
})

// When selecting a lesson from a different track (e.g. from module list), sync track
watch(selectedLessonId, (id) => {
  if (id) {
    const lesson = dojo.allLessons.find(l => l.id === id)
    if (lesson) {
      selectedTrackId.value = lesson.trackId
    }
  }
})
</script>

<template>
  <div class="terminal-workspace space-y-4 h-full overflow-y-auto">
    <!-- Header -->
    <QPageHeading title="Dojo" :meta="`${dojo.overallProgress.completed}/${dojo.overallProgress.total} lessons`" />

    <!-- Top stats bar -->
    <div class="flex items-center gap-3 flex-wrap">
      <div class="flex items-center gap-1.5 text-[11px]">
        <Star class="w-3.5 h-3.5" style="color: var(--accent)" />
        <span class="font-semibold" style="color: var(--text-primary)">Lv.{{ dojo.progress.level }}</span>
      </div>
      <div class="flex items-center gap-1.5 text-[11px]">
        <Zap class="w-3.5 h-3.5" style="color: var(--warning)" />
        <span style="color: var(--text-secondary)">{{ dojo.progress.totalXP }} XP</span>
      </div>
      <div class="flex items-center gap-1.5 text-[11px]">
        <Flame class="w-3.5 h-3.5" style="color: var(--negative)" />
        <span style="color: var(--text-secondary)">{{ dojo.progress.currentStreak }} day streak</span>
      </div>
      <div class="flex items-center gap-1.5 text-[11px]">
        <BookOpen class="w-3.5 h-3.5" style="color: var(--positive)" />
        <span style="color: var(--text-secondary)">{{ dojo.overallProgress.completed }}/{{ dojo.overallProgress.total }} lessons</span>
      </div>
    </div>

    <!-- Overview Mode -->
    <template v-if="viewMode === 'overview'">
      <TerminalDojoProgressDashboard />
      <TerminalDojoTrackOverview @select-track="selectTrack" />
    </template>

    <!-- Track Detail Mode -->
    <template v-else-if="viewMode === 'track' && selectedTrackId">
      <TerminalDojoModuleList
        :track-id="selectedTrackId"
        @back="backToOverview"
        @select-lesson="selectLesson"
      />
    </template>

    <!-- Lesson Mode -->
    <template v-else-if="viewMode === 'lesson' && selectedLessonId && currentLesson">
      <!-- Quiz -->
      <TerminalDojoQuizView
        v-if="currentLesson.type === 'quiz'"
        :lesson-id="selectedLessonId"
        @back="backToTrack"
        @back-to-overview="backToOverview"
        @select-lesson="selectLesson"
      />
      <!-- Challenge -->
      <TerminalDojoChallengeView
        v-else-if="currentLesson.type === 'challenge'"
        :lesson-id="selectedLessonId"
        @back="backToTrack"
        @back-to-overview="backToOverview"
        @select-lesson="selectLesson"
      />
      <!-- Theory / Exercise -->
      <TerminalDojoLessonViewer
        v-else
        :lesson-id="selectedLessonId"
        @back="backToTrack"
        @back-to-overview="backToOverview"
        @select-lesson="selectLesson"
      />
    </template>
  </div>
</template>
