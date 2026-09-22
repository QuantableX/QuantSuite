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
  <div class="space-y-3">
    <!-- Stats row -->
    <div class="grid grid-cols-4 gap-2">
      <div class="card p-3 text-center">
        <div class="flex items-center justify-center gap-1 mb-1">
          <Star class="w-3.5 h-3.5" style="color: var(--accent)" />
        </div>
        <div class="text-lg font-bold" style="color: var(--text-primary)">{{ dojo.progress.level }}</div>
        <div class="text-[9px]" style="color: var(--muted)">Level</div>
      </div>
      <div class="card p-3 text-center">
        <div class="flex items-center justify-center gap-1 mb-1">
          <Zap class="w-3.5 h-3.5" style="color: var(--warning)" />
        </div>
        <div class="text-lg font-bold" style="color: var(--text-primary)">{{ dojo.progress.totalXP }}</div>
        <div class="text-[9px]" style="color: var(--muted)">Total XP</div>
      </div>
      <div class="card p-3 text-center">
        <div class="flex items-center justify-center gap-1 mb-1">
          <Flame class="w-3.5 h-3.5" style="color: var(--negative)" />
        </div>
        <div class="text-lg font-bold" style="color: var(--text-primary)">{{ dojo.progress.currentStreak }}</div>
        <div class="text-[9px]" style="color: var(--muted)">Day Streak</div>
      </div>
      <div class="card p-3 text-center">
        <div class="flex items-center justify-center gap-1 mb-1">
          <BookOpen class="w-3.5 h-3.5" style="color: var(--positive)" />
        </div>
        <div class="text-lg font-bold" style="color: var(--text-primary)">{{ dojo.overallProgress.completed }}</div>
        <div class="text-[9px]" style="color: var(--muted)">Completed</div>
      </div>
    </div>

    <!-- XP progress to next level -->
    <div class="card p-3">
      <div class="flex items-center justify-between mb-1.5">
        <span class="text-[11px] font-medium" style="color: var(--text-secondary)">
          Level {{ dojo.progress.level }} Progress
        </span>
        <span class="text-[10px]" style="color: var(--muted)">
          {{ dojo.xpToNextLevel }} XP to Level {{ dojo.progress.level + 1 }}
        </span>
      </div>
      <div class="w-full h-2 rounded-full" style="background-color: var(--surface-3)">
        <div
          class="h-full rounded-full transition-all"
          style="background-color: var(--accent)"
          :style="{ width: dojo.xpProgressPercent + '%' }"
        />
      </div>
    </div>

    <!-- Track completion circles -->
    <div class="card p-3">
      <h4 class="text-[11px] font-semibold mb-3" style="color: var(--text-secondary)">Track Progress</h4>
      <div class="grid grid-cols-4 gap-3">
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
              <span class="text-[10px] font-bold" style="color: var(--text-primary)">
                {{ dojo.trackProgress[track.id]?.percent ?? 0 }}%
              </span>
            </div>
          </div>
          <span class="text-[9px] leading-tight block" style="color: var(--text-secondary)">
            {{ track.name.split(' ')[0] }}
          </span>
        </div>
      </div>
    </div>

    <!-- Achievements -->
    <div class="card p-3">
      <h4 class="text-[11px] font-semibold mb-3" style="color: var(--text-secondary)">Achievements</h4>
      <div class="grid grid-cols-3 gap-2">
        <div
          v-for="achievement in dojo.allAchievements"
          :key="achievement.id"
          class="p-2 rounded-lg text-center"
          :style="{
            backgroundColor: dojo.unlockedAchievements.some(a => a.id === achievement.id)
              ? 'rgba(255, 215, 0, 0.08)'
              : 'var(--surface-2)',
            opacity: dojo.unlockedAchievements.some(a => a.id === achievement.id) ? 1 : 0.5,
          }"
        >
          <component
            :is="dojo.unlockedAchievements.some(a => a.id === achievement.id) ? achievementIcons[achievement.icon] || Trophy : Lock"
            class="w-5 h-5 mx-auto mb-1"
            :style="{
              color: dojo.unlockedAchievements.some(a => a.id === achievement.id)
                ? '#ffd700'
                : 'var(--muted)',
            }"
          />
          <div class="text-[9px] font-semibold" style="color: var(--text-primary)">{{ achievement.name }}</div>
          <div class="text-[8px]" style="color: var(--muted)">{{ achievement.description }}</div>
        </div>
      </div>
    </div>

    <!-- Recent activity -->
    <div class="card p-3" v-if="dojo.recentActivity.length > 0">
      <h4 class="text-[11px] font-semibold mb-2" style="color: var(--text-secondary)">Recent Activity</h4>
      <div class="space-y-1.5">
        <div
          v-for="item in dojo.recentActivity"
          :key="item.id"
          class="flex items-center gap-2 text-[10px]"
        >
          <div
            class="w-1.5 h-1.5 rounded-full flex-shrink-0"
            :style="{ backgroundColor: typeColors[item.type] || 'var(--muted)' }"
          />
          <span style="color: var(--text-secondary)">{{ item.title }}</span>
          <span class="text-[9px] ml-auto flex-shrink-0 uppercase font-medium" :style="{ color: typeColors[item.type] || 'var(--muted)' }">
            {{ item.type }}
          </span>
        </div>
      </div>
    </div>

    <!-- Streak info -->
    <div class="card p-3 flex items-center justify-between">
      <div class="flex items-center gap-2">
        <Flame class="w-4 h-4" style="color: var(--negative)" />
        <div>
          <div class="text-[11px] font-medium" style="color: var(--text-primary)">
            {{ dojo.progress.currentStreak }} day streak
          </div>
          <div class="text-[9px]" style="color: var(--muted)">
            Longest: {{ dojo.progress.longestStreak }} days
          </div>
        </div>
      </div>
      <div class="text-[10px]" style="color: var(--text-secondary)">
        {{ dojo.overallProgress.completed }} / {{ dojo.overallProgress.total }} lessons
      </div>
    </div>
  </div>
</template>
