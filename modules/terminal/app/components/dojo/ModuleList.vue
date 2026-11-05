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
  <div v-if="track" class="dojo-mods">
    <!-- Breadcrumb -->
    <div class="flex items-center gap-2 text-[12px]">
      <button
        class="flex items-center gap-1 font-medium transition-colors hover:brightness-125"
        style="color: var(--accent)"
        @click="emit('back')"
      >
        <ArrowLeft class="w-3.5 h-3.5" />
        Dojo
      </button>
      <ChevronRight class="w-3 h-3" style="color: var(--muted)" />
      <span class="font-medium" style="color: var(--text-secondary)">{{ track.name }}</span>
    </div>

    <!-- Track header -->
    <div class="card dojo-mods__head">
      <div class="min-w-0">
        <h3>{{ track.name }}</h3>
        <p>{{ track.description }}</p>
      </div>
      <div class="dojo-mods__bar">
        <div class="flex justify-between gap-2 mb-1">
          <span>
            {{ dojo.trackProgress[track.id]?.completed ?? 0 }} / {{ dojo.trackProgress[track.id]?.total ?? 0 }} lessons completed
          </span>
          <span>{{ dojo.trackProgress[track.id]?.percent ?? 0 }}%</span>
        </div>
        <div class="w-full h-1.5 rounded-full" style="background-color: var(--surface-3)">
          <div
            class="h-full rounded-full transition-all"
            :style="{ width: (dojo.trackProgress[track.id]?.percent ?? 0) + '%', backgroundColor: track.color }"
          />
        </div>
      </div>
    </div>

    <!-- Module list: more columns as the width allows, each module as tall as it needs -->
    <div class="dojo-mods__grid">
      <div v-for="mod in track.modules" :key="mod.id" class="card overflow-hidden">
        <!-- Module header -->
        <button
          class="dojo-mod transition-colors hover:brightness-110"
          @click="toggleModule(mod.id)"
        >
          <component
            :is="expandedModules.has(mod.id) ? ChevronDown : ChevronRight"
            class="w-4 h-4 flex-shrink-0"
            style="color: var(--muted)"
          />
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-2">
              <h4>{{ mod.name }}</h4>
              <span class="dojo-mod__count">
                {{ dojo.moduleProgress[mod.id]?.completed ?? 0 }}/{{ dojo.moduleProgress[mod.id]?.total ?? 0 }}
              </span>
            </div>
            <p>{{ mod.description }}</p>
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
            class="dojo-lesson transition-colors hover:brightness-110 border-b last:border-b-0"
            style="border-color: var(--border)"
            @click="emit('select-lesson', lesson.id)"
          >
            <Check v-if="isCompleted(lesson.id)" class="w-3.5 h-3.5 flex-shrink-0" style="color: var(--positive)" />
            <component :is="typeIcons[lesson.type]" v-else class="w-3.5 h-3.5 flex-shrink-0" style="color: var(--text-secondary)" />
            <span class="dojo-lesson__title">{{ lesson.title }}</span>
            <span class="dojo-lesson__meta">{{ typeLabels[lesson.type] }}</span>
            <span class="dojo-lesson__meta flex items-center gap-0.5">
              <Clock class="w-2.5 h-2.5" />
              {{ lesson.estimatedMinutes }}m
            </span>
            <ChevronRight class="w-3.5 h-3.5 flex-shrink-0" style="color: var(--muted)" />
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.dojo-mods { display: flex; flex-direction: column; gap: var(--qss-gap); font-variant-numeric: tabular-nums; }
.dojo-mods__head {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(220px, 360px);
  align-items: center;
  gap: var(--qss-gap);
  padding: var(--qss-gap-tight) var(--qss-panel-pad);
}
.dojo-mods__head h3 { margin: 0; color: var(--text-primary); font-size: var(--qss-text-section); font-weight: 600; }
.dojo-mods__head p { margin: 2px 0 0; color: var(--text-secondary); font-size: var(--qss-text-meta); }
.dojo-mods__bar { color: var(--text-secondary); font-size: var(--qss-text-meta); }
.dojo-mods__grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(min(440px, 100%), 1fr));
  align-items: start;
  gap: var(--qss-gap);
}
.dojo-mod {
  display: flex;
  align-items: center;
  gap: var(--qss-gap-tight);
  width: 100%;
  padding: var(--qss-gap-tight) var(--qss-panel-pad);
  text-align: left;
}
.dojo-mod h4 { margin: 0; color: var(--text-primary); font-size: var(--qss-text-label); font-weight: 600; }
.dojo-mod p {
  margin: 1px 0 0;
  overflow: hidden;
  color: var(--text-secondary);
  font-size: var(--qss-text-meta);
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dojo-mod__count {
  padding: 0 6px;
  border-radius: 4px;
  background: var(--surface-3);
  color: var(--text-secondary);
  font-size: 11px;
}
.dojo-lesson {
  display: flex;
  align-items: center;
  gap: var(--qss-gap-tight);
  width: 100%;
  min-height: var(--qss-row-h);
  padding-inline: var(--qss-panel-pad);
  text-align: left;
}
.dojo-lesson__title {
  flex: 1 1 0;
  min-width: 0;
  overflow: hidden;
  color: var(--text-primary);
  font-size: var(--qss-text-label);
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dojo-lesson__meta { flex-shrink: 0; color: var(--text-secondary); font-size: 11px; }
@container terminal-workspace (max-width: 700px) {
  .dojo-mods__head { grid-template-columns: minmax(0, 1fr); }
}
</style>
