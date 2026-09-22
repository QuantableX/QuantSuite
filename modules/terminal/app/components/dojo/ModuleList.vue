<script setup lang="ts">
import {
  ChevronRight, ChevronDown, BookOpen, HelpCircle, Swords, Dumbbell,
  Check, Clock, ArrowLeft,
} from 'lucide-vue-next'
import { useDojoStore } from '#terminal/stores/dojo'
import type { Track, Lesson } from '#terminal/stores/dojo'

const props = defineProps<{
  trackId: string
}>()

const emit = defineEmits<{
  (e: 'back'): void
  (e: 'select-lesson', lessonId: string): void
}>()

const dojo = useDojoStore()

const track = computed(() => dojo.tracks.find(t => t.id === props.trackId) as Track)
const expandedModules = ref<Set<string>>(new Set())

// Expand the first module with incomplete lessons by default
onMounted(() => {
  if (track.value) {
    for (const mod of track.value.modules) {
      const hasIncomplete = mod.lessons.some(l => !dojo.progress.completedLessons.includes(l.id))
      if (hasIncomplete) {
        expandedModules.value.add(mod.id)
        break
      }
    }
    if (expandedModules.value.size === 0 && track.value && track.value.modules.length > 0 && track.value.modules[0]) {
      expandedModules.value.add(track.value.modules[0].id)
    }
  }
})

function toggleModule(modId: string) {
  if (expandedModules.value.has(modId)) {
    expandedModules.value.delete(modId)
  } else {
    expandedModules.value.add(modId)
  }
}

const typeIcons: Record<string, any> = {
  theory: BookOpen,
  quiz: HelpCircle,
  challenge: Swords,
  exercise: Dumbbell,
}

const typeLabels: Record<string, string> = {
  theory: 'Theory',
  quiz: 'Quiz',
  challenge: 'Challenge',
  exercise: 'Exercise',
}

function isCompleted(lessonId: string): boolean {
  return dojo.progress.completedLessons.includes(lessonId)
}
</script>

<template>
  <div class="space-y-3" v-if="track">
    <!-- Breadcrumb -->
    <div class="flex items-center gap-2">
      <button
        class="flex items-center gap-1 text-[11px] font-medium transition-colors hover:brightness-125"
        style="color: var(--accent)"
        @click="emit('back')"
      >
        <ArrowLeft class="w-3.5 h-3.5" />
        Dojo
      </button>
      <ChevronRight class="w-3 h-3" style="color: var(--muted)" />
      <span class="text-[11px] font-medium" :style="{ color: track.color }">{{ track.name }}</span>
    </div>

    <!-- Track header -->
    <div class="card p-3">
      <h3 class="text-sm font-semibold" style="color: var(--text-primary)">{{ track.name }}</h3>
      <p class="text-[11px] mt-0.5" style="color: var(--text-secondary)">{{ track.description }}</p>
      <div class="mt-2">
        <div class="flex justify-between text-[10px] mb-1">
          <span style="color: var(--muted)">
            {{ dojo.trackProgress[track.id]?.completed ?? 0 }} / {{ dojo.trackProgress[track.id]?.total ?? 0 }} lessons completed
          </span>
          <span style="color: var(--muted)">{{ dojo.trackProgress[track.id]?.percent ?? 0 }}%</span>
        </div>
        <div class="w-full h-1.5 rounded-full" style="background-color: var(--surface-3)">
          <div
            class="h-full rounded-full transition-all"
            :style="{ width: (dojo.trackProgress[track.id]?.percent ?? 0) + '%', backgroundColor: track.color }"
          />
        </div>
      </div>
    </div>

    <!-- Module list -->
    <div v-for="mod in track.modules" :key="mod.id" class="card overflow-hidden">
      <!-- Module header -->
      <button
        class="w-full p-3 flex items-center gap-3 text-left transition-colors hover:brightness-110"
        @click="toggleModule(mod.id)"
      >
        <component
          :is="expandedModules.has(mod.id) ? ChevronDown : ChevronRight"
          class="w-4 h-4 flex-shrink-0"
          style="color: var(--muted)"
        />
        <div class="flex-1 min-w-0">
          <div class="flex items-center gap-2">
            <h4 class="text-xs font-semibold" style="color: var(--text-primary)">{{ mod.name }}</h4>
            <span class="text-[10px] px-1.5 py-0.5 rounded" style="background-color: var(--surface-3); color: var(--muted)">
              {{ dojo.moduleProgress[mod.id]?.completed ?? 0 }}/{{ dojo.moduleProgress[mod.id]?.total ?? 0 }}
            </span>
          </div>
          <p class="text-[10px] mt-0.5 truncate" style="color: var(--text-secondary)">{{ mod.description }}</p>
        </div>
        <div class="w-16 flex-shrink-0">
          <div class="w-full h-1 rounded-full" style="background-color: var(--surface-3)">
            <div
              class="h-full rounded-full transition-all"
              :style="{ width: (dojo.moduleProgress[mod.id]?.percent ?? 0) + '%', backgroundColor: track.color }"
            />
          </div>
        </div>
      </button>

      <!-- Lessons (collapsible) -->
      <div v-if="expandedModules.has(mod.id)" class="border-t" style="border-color: var(--border)">
        <button
          v-for="lesson in mod.lessons"
          :key="lesson.id"
          class="w-full px-3 py-2.5 flex items-center gap-3 text-left transition-colors hover:brightness-110 border-b last:border-b-0"
          style="border-color: var(--border)"
          @click="emit('select-lesson', lesson.id)"
        >
          <div
            class="w-7 h-7 rounded-md flex items-center justify-center flex-shrink-0"
            :style="{
              backgroundColor: isCompleted(lesson.id) ? 'rgba(8,153,129,0.15)' : track.color + '15',
              color: isCompleted(lesson.id) ? 'var(--positive)' : track.color,
            }"
          >
            <Check v-if="isCompleted(lesson.id)" class="w-3.5 h-3.5" />
            <component v-else :is="typeIcons[lesson.type]" class="w-3.5 h-3.5" />
          </div>
          <div class="flex-1 min-w-0">
            <div class="text-[11px] font-medium" style="color: var(--text-primary)">{{ lesson.title }}</div>
            <div class="flex items-center gap-2 mt-0.5">
              <span
                class="text-[9px] px-1.5 py-0.5 rounded font-medium"
                :style="{ backgroundColor: track.color + '18', color: track.color }"
              >
                {{ typeLabels[lesson.type] }}
              </span>
              <span class="text-[9px] flex items-center gap-0.5" style="color: var(--muted)">
                <Clock class="w-2.5 h-2.5" />
                {{ lesson.estimatedMinutes }}m
              </span>
            </div>
          </div>
          <ChevronRight class="w-3.5 h-3.5 flex-shrink-0" style="color: var(--muted)" />
        </button>
      </div>
    </div>
  </div>
</template>
