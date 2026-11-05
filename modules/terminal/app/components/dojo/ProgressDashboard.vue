<script setup lang="ts">
import {
  Flame, Zap, BookOpen, Trophy, Award, GraduationCap, Swords,
  Footprints, BookCheck, Star, Lock,
} from 'lucide-vue-next'
import { useDojoStore } from '#terminal/stores/dojo'

const dojo = useDojoStore()

const achievementIcons: Record<string, any> = {
  Footprints,
  GraduationCap,
  Flame,
  BookCheck,
  Award,
  Swords,
}

const typeColors: Record<string, string> = {
  theory: 'var(--accent)',
  quiz: 'var(--warning)',
  challenge: 'var(--negative)',
  exercise: 'var(--positive)',
}
</script>

<template>
  <div class="dojo-dash">
    <!-- Where you stand: one panel, split by spacing -->
    <section class="card dojo-progress">
      <div class="dojo-stats">
        <div class="dojo-stat">
          <Star class="w-3.5 h-3.5" style="color: var(--accent)" />
          <strong>{{ dojo.progress.level }}</strong>
          <span>Level</span>
        </div>
        <div class="dojo-stat">
          <Zap class="w-3.5 h-3.5" style="color: var(--warning)" />
          <strong>{{ dojo.progress.totalXP }}</strong>
          <span>Total XP</span>
        </div>
        <div class="dojo-stat">
          <Flame class="w-3.5 h-3.5" style="color: var(--negative)" />
          <strong>{{ dojo.progress.currentStreak }}</strong>
          <span>Day Streak</span>
        </div>
        <div class="dojo-stat">
          <BookOpen class="w-3.5 h-3.5" style="color: var(--positive)" />
          <strong>{{ dojo.overallProgress.completed }}</strong>
          <span>Completed</span>
        </div>
      </div>

      <!-- XP progress to next level -->
      <div class="dojo-level">
        <div class="dojo-level__head">
          <span>Level {{ dojo.progress.level }} Progress</span>
          <small>{{ dojo.xpToNextLevel }} XP to Level {{ dojo.progress.level + 1 }}</small>
        </div>
        <div class="w-full h-2 rounded-full" style="background-color: var(--surface-3)">
          <div
            class="h-full rounded-full transition-all"
            style="background-color: var(--accent)"
            :style="{ width: dojo.xpProgressPercent + '%' }"
          />
        </div>
      </div>

      <!-- Streak info -->
      <div class="dojo-streak">
        <Flame class="w-4 h-4 flex-shrink-0" style="color: var(--negative)" />
        <div>
          <strong>{{ dojo.progress.currentStreak }} day streak</strong>
          <small>Longest: {{ dojo.progress.longestStreak }} days</small>
        </div>
        <small class="dojo-streak__lessons">{{ dojo.overallProgress.completed }} / {{ dojo.overallProgress.total }} lessons</small>
      </div>
    </section>

    <!-- The learning tracks come between the progress and the details. -->
    <slot />

    <div class="dojo-lower" :class="{ 'has-activity': dojo.recentActivity.length > 0 }">
      <!-- Track completion circles -->
      <section class="card p-3">
        <h4>Track Progress</h4>
        <div class="dojo-rings">
          <div v-for="track in dojo.tracks" :key="track.id" class="text-center">
            <div class="relative w-12 h-12 mx-auto mb-1.5">
              <svg class="w-12 h-12 -rotate-90" viewBox="0 0 36 36">
                <path
                  d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
                  fill="none"
                  stroke="var(--surface-3)"
                  stroke-width="3"
                />
                <path
                  d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
                  fill="none"
                  :stroke="track.color"
                  stroke-width="3"
                  stroke-linecap="round"
                  :stroke-dasharray="`${dojo.trackProgress[track.id]?.percent ?? 0}, 100`"
                />
              </svg>
              <div class="absolute inset-0 flex items-center justify-center">
                <span class="text-[11px] font-semibold" style="color: var(--text-primary)">
                  {{ dojo.trackProgress[track.id]?.percent ?? 0 }}%
                </span>
              </div>
            </div>
            <span class="dojo-rings__name">
              {{ track.name.split(' ')[0] }}
            </span>
          </div>
        </div>
      </section>

      <!-- Achievements -->
      <section class="card p-3">
        <h4>Achievements</h4>
        <div class="dojo-achievements">
          <div
            v-for="achievement in dojo.allAchievements"
            :key="achievement.id"
            class="dojo-achievement"
            :class="{ 'is-unlocked': dojo.unlockedAchievements.some(a => a.id === achievement.id) }"
          >
            <component
              :is="dojo.unlockedAchievements.some(a => a.id === achievement.id) ? achievementIcons[achievement.icon] || Trophy : Lock"
              class="w-4 h-4 flex-shrink-0"
              :style="{
                color: dojo.unlockedAchievements.some(a => a.id === achievement.id)
                  ? '#ffd700'
                  : 'var(--muted)',
              }"
            />
            <div class="min-w-0">
              <strong>{{ achievement.name }}</strong>
              <small>{{ achievement.description }}</small>
            </div>
          </div>
        </div>
      </section>

      <!-- Recent activity -->
      <section v-if="dojo.recentActivity.length > 0" class="card p-3">
        <h4>Recent Activity</h4>
        <div class="dojo-activity">
          <div
            v-for="item in dojo.recentActivity"
            :key="item.id"
            class="flex items-center gap-2 text-[12px]"
          >
            <div
              class="w-1.5 h-1.5 rounded-full flex-shrink-0"
              :style="{ backgroundColor: typeColors[item.type] || 'var(--muted)' }"
            />
            <span class="truncate" style="color: var(--text-secondary)" :title="item.title">{{ item.title }}</span>
            <span class="dojo-activity__type ml-auto flex-shrink-0" :style="{ color: typeColors[item.type] || 'var(--muted)' }">
              {{ item.type }}
            </span>
          </div>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.dojo-dash {
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
  font-variant-numeric: tabular-nums;
}
.dojo-progress {
  display: grid;
  grid-template-columns: auto minmax(200px, 1fr) auto;
  align-items: center;
  gap: var(--qss-gap) 24px;
  padding: var(--qss-gap-tight) var(--qss-panel-pad);
}
.dojo-stats { display: flex; flex-wrap: wrap; gap: var(--qss-gap) 24px; }
.dojo-stat {
  display: grid;
  grid-template-columns: auto auto;
  align-items: center;
  column-gap: 6px;
}
.dojo-stat strong {
  color: var(--text-primary);
  font-size: var(--qss-text-title);
  font-weight: 600;
  line-height: 1.25;
}
.dojo-stat span {
  grid-column: 1 / -1;
  color: var(--text-secondary);
  font-size: var(--qss-text-meta);
  white-space: nowrap;
}
.dojo-level { min-width: 0; }
.dojo-level__head {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 6px;
  color: var(--text-secondary);
  font-size: var(--qss-text-label);
}
.dojo-level__head small { color: var(--text-secondary); font-size: var(--qss-text-meta); }
.dojo-streak { display: flex; align-items: center; gap: 8px; }
.dojo-streak strong,
.dojo-streak small { display: block; white-space: nowrap; }
.dojo-streak strong { color: var(--text-primary); font-size: var(--qss-text-label); font-weight: 500; }
.dojo-streak small { color: var(--text-secondary); font-size: var(--qss-text-meta); }
.dojo-streak__lessons { margin-left: var(--qss-gap); }

.dojo-lower {
  display: grid;
  grid-template-columns: minmax(280px, 1fr) minmax(0, 2fr);
  gap: var(--qss-gap);
}
.dojo-lower.has-activity { grid-template-columns: minmax(260px, 1fr) minmax(0, 2fr) minmax(240px, 1fr); }
.dojo-lower h4 {
  margin: 0 0 var(--qss-gap-tight);
  color: var(--text-primary);
  font-size: var(--qss-text-section);
  font-weight: 600;
}
.dojo-rings { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: var(--qss-gap-tight); }
.dojo-rings__name { display: block; color: var(--text-secondary); font-size: var(--qss-text-meta); line-height: 1.2; }
.dojo-achievements {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: var(--qss-gap) var(--qss-gap);
}
.dojo-achievement { display: flex; gap: var(--qss-gap-tight); min-width: 0; }
.dojo-achievement strong,
.dojo-achievement small { display: block; }
.dojo-achievement strong { color: var(--text-secondary); font-size: var(--qss-text-label); font-weight: 500; }
.dojo-achievement.is-unlocked strong { color: var(--text-primary); font-weight: 600; }
.dojo-achievement small { color: var(--text-secondary); font-size: var(--qss-text-meta); line-height: 1.35; }
.dojo-activity { display: flex; flex-direction: column; gap: 6px; max-height: 118px; overflow-y: auto; }
.dojo-activity__type { font-size: var(--qss-text-meta); font-weight: 500; text-transform: capitalize; }

@container terminal-workspace (max-width: 1180px) {
  .dojo-progress { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); }
  .dojo-level { grid-column: 1 / -1; grid-row: 2; }
  .dojo-streak { justify-self: end; }
  .dojo-lower.has-activity { grid-template-columns: minmax(260px, 1fr) minmax(240px, 1fr); }
  .dojo-lower.has-activity > :nth-child(2) { grid-column: 1 / -1; grid-row: 2; }
}
@container terminal-workspace (max-width: 760px) {
  .dojo-progress { grid-template-columns: minmax(0, 1fr); }
  .dojo-streak { justify-self: start; }
  .dojo-level { grid-row: auto; }
  .dojo-lower,
  .dojo-lower.has-activity { grid-template-columns: minmax(0, 1fr); }
  .dojo-lower.has-activity > :nth-child(2) { grid-row: auto; }
  .dojo-achievements { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
</style>
