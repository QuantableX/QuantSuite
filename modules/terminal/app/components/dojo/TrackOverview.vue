<script setup lang="ts">
import { BookOpen, Code, Brain, Trophy, Zap } from 'lucide-vue-next'
import { useDojoStore } from '#terminal/stores/dojo'

const emit = defineEmits<{
  (e: 'select-track', trackId: string): void
}>()

const dojo = useDojoStore()

const iconMap: Record<string, any> = {
  BookOpen,
  Code,
  Brain,
  Trophy,
}
</script>

<template>
  <div>
    <h3 class="text-xs font-semibold mb-3" style="color: var(--text-secondary)">Learning Tracks</h3>
    <div class="grid grid-cols-2 gap-3">
      <button
        v-for="track in dojo.tracks"
        :key="track.id"
        class="card p-4 flex flex-col gap-3 text-left transition-all hover:brightness-110"
        @click="emit('select-track', track.id)"
      >
        <div class="flex items-center gap-3">
          <div class="p-2.5 rounded-lg" :style="{ backgroundColor: track.color + '18' }">
            <component :is="iconMap[track.icon]" class="w-5 h-5" :style="{ color: track.color }" />
          </div>
          <div class="min-w-0 flex-1">
            <h3 class="text-sm font-semibold truncate" style="color: var(--text-primary)">{{ track.name }}</h3>
            <p class="text-[11px] truncate" style="color: var(--text-secondary)">{{ track.description }}</p>
          </div>
        </div>

        <div>
          <div class="flex justify-between text-[10px] mb-1">
            <span style="color: var(--muted)">
              {{ dojo.trackProgress[track.id]?.completed ?? 0 }} / {{ dojo.trackProgress[track.id]?.total ?? 0 }} lessons
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

        <div class="flex items-center justify-between mt-auto">
          <span class="text-[10px]" style="color: var(--muted)">
            {{ track.modules.length }} modules
          </span>
          <div class="flex items-center gap-1 text-[10px]" :style="{ color: track.color }">
            <Zap class="w-3 h-3" />
            <span>{{ dojo.trackProgress[track.id]?.xp ?? 0 }} XP</span>
          </div>
        </div>
      </button>
    </div>
  </div>
</template>
