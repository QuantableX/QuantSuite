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
  <section>
    <h3 class="dojo-tracks__title">Learning Tracks</h3>
    <div class="dojo-tracks">
      <button
        v-for="track in dojo.tracks"
        :key="track.id"
        class="card dojo-track transition-all hover:brightness-110"
        @click="emit('select-track', track.id)"
      >
        <div class="dojo-track__head">
          <component :is="iconMap[track.icon]" class="w-4 h-4 flex-shrink-0" style="color: var(--text-secondary)" />
          <div class="min-w-0 flex-1">
            <h3>{{ track.name }}</h3>
            <p>{{ track.description }}</p>
          </div>
        </div>

        <div class="mt-auto">
          <div class="dojo-track__line">
            <span>
              {{ dojo.trackProgress[track.id]?.completed ?? 0 }} / {{ dojo.trackProgress[track.id]?.total ?? 0 }} lessons
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

        <div class="dojo-track__line">
          <span>{{ track.modules.length }} modules</span>
          <span class="flex items-center gap-1">
            <Zap class="w-3 h-3" />
            {{ dojo.trackProgress[track.id]?.xp ?? 0 }} XP
          </span>
        </div>
      </button>
    </div>
  </section>
</template>

<style scoped>
.dojo-tracks__title {
  margin: 0 0 var(--qss-gap-tight);
  color: var(--text-primary);
  font-size: var(--qss-text-section);
  font-weight: 600;
}
.dojo-tracks {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: var(--qss-gap);
}
.dojo-track {
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap-tight);
  padding: var(--qss-panel-pad);
  text-align: left;
  font-variant-numeric: tabular-nums;
}
.dojo-track__head { display: flex; gap: var(--qss-gap-tight); }
.dojo-track__head svg { margin-top: 2px; }
.dojo-track h3 {
  margin: 0;
  color: var(--text-primary);
  font-size: var(--qss-text-section);
  font-weight: 600;
}
.dojo-track p {
  margin: 2px 0 0;
  color: var(--text-secondary);
  font-size: var(--qss-text-meta);
  line-height: 1.35;
}
.dojo-track__line {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  color: var(--text-secondary);
  font-size: var(--qss-text-meta);
}
.dojo-track .mt-auto .dojo-track__line { margin-bottom: 4px; }
@container terminal-workspace (max-width: 1180px) {
  .dojo-tracks { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
@container terminal-workspace (max-width: 520px) {
  .dojo-tracks { grid-template-columns: minmax(0, 1fr); }
}
</style>
