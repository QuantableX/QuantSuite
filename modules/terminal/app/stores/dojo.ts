import { defineStore } from 'pinia'

// ─── Types ──────────────────────────────────────────────────────────────────

export interface Lesson {
  id: string
  trackId: string
  moduleId: string
  title: string
  description: string
  content: string
  type: 'theory' | 'quiz' | 'challenge' | 'exercise'
  difficulty: 'beginner' | 'intermediate' | 'advanced'
  estimatedMinutes: number
  order: number
  quiz?: QuizQuestion[]
  challenge?: Challenge
}

export interface QuizQuestion {
  id: string
  question: string
  options: string[]
  correctIndex: number
  explanation: string
}

export interface Challenge {
  id: string
  title: string
  description: string
  scenario: string
  options: { label: string; outcome: string; score: number }[]
}

export interface UserProgress {
  completedLessons: string[]
  quizScores: Record<string, number>
  challengeResults: Record<string, number>
  currentStreak: number
  longestStreak: number
  lastActiveDate: string
  totalXP: number
  level: number
  completedDates: string[]
}

export interface Track {
  id: string
  name: string
  icon: string
  description: string
  color: string
  modules: Module[]
}

export interface Module {
  id: string
  trackId: string
  name: string
  description: string
  lessons: Lesson[]
}

// ─── Achievements ───────────────────────────────────────────────────────────

export interface Achievement {
  id: string
  name: string
  description: string
  icon: string
  condition: (progress: UserProgress, tracks: Track[]) => boolean
}

const ACHIEVEMENTS: Achievement[] = [
  {
    id: 'first-steps',
    name: 'First Steps',
    description: 'Complete your first lesson',
    icon: 'Footprints',
    condition: (p) => p.completedLessons.length >= 1,
  },
  {
    id: 'quiz-master',
    name: 'Quiz Master',
    description: 'Score 100% on 5 quizzes',
    icon: 'GraduationCap',
    condition: (p) => Object.values(p.quizScores).filter(s => s === 100).length >= 5,
  },
  {
    id: 'dedicated',
    name: 'Dedicated',
    description: 'Maintain a 7-day streak',
    icon: 'Flame',
    condition: (p) => p.longestStreak >= 7,
  },
  {
    id: 'scholar',
    name: 'Scholar',
    description: 'Complete a full module',
    icon: 'BookCheck',
    condition: (p, tracks) => {
      for (const track of tracks) {
        for (const mod of track.modules) {
          if (mod.lessons.every(l => p.completedLessons.includes(l.id))) return true
        }
      }
      return false
    },
  },
  {
    id: 'expert',
    name: 'Expert',
    description: 'Complete a full track',
    icon: 'Award',
    condition: (p, tracks) => {
      for (const track of tracks) {
        const allLessons = track.modules.flatMap(m => m.lessons)
        if (allLessons.length > 0 && allLessons.every(l => p.completedLessons.includes(l.id))) return true
      }
      return false
    },
  },
  {
    id: 'challenger',
    name: 'Challenger',
    description: 'Complete 5 challenges',
    icon: 'Swords',
    condition: (p) => Object.keys(p.challengeResults).length >= 5,
  },
]

// ─── Store ──────────────────────────────────────────────────────────────────

const STORAGE_KEY = 'quantview-dojo-progress'

function getDefaultProgress(): UserProgress {
  return {
    completedLessons: [],
    quizScores: {},
    challengeResults: {},
    currentStreak: 0,
    longestStreak: 0,
    lastActiveDate: '',
    totalXP: 0,
    level: 1,
    completedDates: [],
  }
}

function todayStr(): string {
  return new Date().toISOString().slice(0, 10)
}

export const useDojoStore = defineStore('terminal/dojo', () => {
  // The curriculum lives in `#terminal/data/dojo-curriculum` and is a lazy
  // chunk: the terminal chunk the shell prefetches on idle does not carry the
  // lesson HTML, and an HMR of this file no longer re-parses it. `tracks` is
  // empty until `loadCurriculum()` resolves — the dojo page calls it when it
  // mounts or activates.
  const tracks = ref<Track[]>([])
  let curriculum: Promise<void> | null = null

  function loadCurriculum(): Promise<void> {
    if (!curriculum) {
      curriculum = import('#terminal/data/dojo-curriculum')
        .then(({ buildTracks }) => {
          tracks.value = buildTracks()
        })
        .catch((e) => {
          curriculum = null
          throw e
        })
    }
    return curriculum
  }

  const progress = ref<UserProgress>(getDefaultProgress())

  // ─── Persistence ────────────────────────────────────────────────────────
  function loadFromStorage() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY)
      if (raw) {
        const saved = JSON.parse(raw) as Partial<UserProgress>
        progress.value = { ...getDefaultProgress(), ...saved }
      }
    } catch {
      // Ignore corrupt storage
    }
    updateStreak()
  }

  function saveToStorage() {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(progress.value))
  }

  // ─── Streak Tracking ───────────────────────────────────────────────────
  function updateStreak() {
    const today = todayStr()
    const last = progress.value.lastActiveDate
    if (!last) return

    const lastDate = new Date(last)
    const todayDate = new Date(today)
    const diffDays = Math.floor((todayDate.getTime() - lastDate.getTime()) / (1000 * 60 * 60 * 24))

    if (diffDays > 1) {
      progress.value.currentStreak = 0
    }
  }

  function recordActivity() {
    const today = todayStr()
    const last = progress.value.lastActiveDate

    if (last !== today) {
      const lastDate = new Date(last || '2000-01-01')
      const todayDate = new Date(today)
      const diffDays = Math.floor((todayDate.getTime() - lastDate.getTime()) / (1000 * 60 * 60 * 24))

      if (diffDays === 1) {
        progress.value.currentStreak += 1
      } else if (diffDays > 1) {
        progress.value.currentStreak = 1
      } else if (!last) {
        progress.value.currentStreak = 1
      }

      if (progress.value.currentStreak > progress.value.longestStreak) {
        progress.value.longestStreak = progress.value.currentStreak
      }

      progress.value.lastActiveDate = today
      if (!progress.value.completedDates.includes(today)) {
        progress.value.completedDates.push(today)
      }
    }
  }

  // ─── XP System ──────────────────────────────────────────────────────────
  function addXP(amount: number) {
    progress.value.totalXP += amount
    progress.value.level = Math.floor(progress.value.totalXP / 100) + 1
  }

  // ─── Actions ────────────────────────────────────────────────────────────
  function completeLesson(lessonId: string) {
    if (progress.value.completedLessons.includes(lessonId)) return

    const lesson = allLessons.value.find(l => l.id === lessonId)
    if (!lesson) return

    progress.value.completedLessons.push(lessonId)
    recordActivity()

    const xpMap: Record<string, number> = {
      theory: 10,
      exercise: 15,
      quiz: 25,
      challenge: 50,
    }
    addXP(xpMap[lesson.type] || 10)
    saveToStorage()
  }

  function submitQuiz(lessonId: string, score: number) {
    progress.value.quizScores[lessonId] = Math.max(score, progress.value.quizScores[lessonId] || 0)
    recordActivity()

    if (!progress.value.completedLessons.includes(lessonId)) {
      progress.value.completedLessons.push(lessonId)
      const xp = Math.round(25 * (score / 100))
      addXP(xp)
    }
    saveToStorage()
  }

  function submitChallenge(lessonId: string, score: number) {
    progress.value.challengeResults[lessonId] = Math.max(score, progress.value.challengeResults[lessonId] || 0)
    recordActivity()

    if (!progress.value.completedLessons.includes(lessonId)) {
      progress.value.completedLessons.push(lessonId)
      addXP(score)
    }
    saveToStorage()
  }

  // ─── Computed ───────────────────────────────────────────────────────────
  const allLessons = computed(() => tracks.value.flatMap(t => t.modules.flatMap(m => m.lessons)))
  const totalLessons = computed(() => allLessons.value.length)

  const trackProgress = computed(() => {
    const result: Record<string, { completed: number; total: number; percent: number; xp: number }> = {}
    for (const track of tracks.value) {
      const lessons = track.modules.flatMap(m => m.lessons)
      const completed = lessons.filter(l => progress.value.completedLessons.includes(l.id)).length
      result[track.id] = {
        completed,
        total: lessons.length,
        percent: lessons.length > 0 ? Math.round((completed / lessons.length) * 100) : 0,
        xp: lessons.filter(l => progress.value.completedLessons.includes(l.id))
          .reduce((sum, l) => {
            const xpMap: Record<string, number> = { theory: 10, exercise: 15, quiz: 25, challenge: 50 }
            return sum + (xpMap[l.type] || 10)
          }, 0),
      }
    }
    return result
  })

  const moduleProgress = computed(() => {
    const result: Record<string, { completed: number; total: number; percent: number }> = {}
    for (const track of tracks.value) {
      for (const mod of track.modules) {
        const completed = mod.lessons.filter(l => progress.value.completedLessons.includes(l.id)).length
        result[mod.id] = {
          completed,
          total: mod.lessons.length,
          percent: mod.lessons.length > 0 ? Math.round((completed / mod.lessons.length) * 100) : 0,
        }
      }
    }
    return result
  })

  const overallProgress = computed(() => {
    const completed = progress.value.completedLessons.length
    return {
      completed,
      total: totalLessons.value,
      percent: totalLessons.value > 0 ? Math.round((completed / totalLessons.value) * 100) : 0,
    }
  })

  const nextLesson = computed(() => {
    for (const track of tracks.value) {
      for (const mod of track.modules) {
        for (const lesson of mod.lessons) {
          if (!progress.value.completedLessons.includes(lesson.id)) {
            return lesson
          }
        }
      }
    }
    return null
  })

  const unlockedAchievements = computed(() => {
    return ACHIEVEMENTS.filter(a => a.condition(progress.value, tracks.value))
  })

  const allAchievements = computed(() => ACHIEVEMENTS)

  const recentActivity = computed(() => {
    const completed = [...progress.value.completedLessons].reverse().slice(0, 10)
    return completed.map(id => {
      const lesson = allLessons.value.find(l => l.id === id)
      return lesson ? { id, title: lesson.title, type: lesson.type } : null
    }).filter(Boolean) as { id: string; title: string; type: string }[]
  })

  const xpToNextLevel = computed(() => {
    const nextLevelXP = progress.value.level * 100
    return nextLevelXP - progress.value.totalXP
  })

  const xpProgressPercent = computed(() => {
    const currentLevelXP = (progress.value.level - 1) * 100
    const nextLevelXP = progress.value.level * 100
    const progressInLevel = progress.value.totalXP - currentLevelXP
    const levelRange = nextLevelXP - currentLevelXP
    return Math.round((progressInLevel / levelRange) * 100)
  })

  // Init
  if (import.meta.client) {
    loadFromStorage()
  }

  return {
    tracks,
    progress,
    allLessons,
    totalLessons,
    trackProgress,
    moduleProgress,
    overallProgress,
    nextLesson,
    unlockedAchievements,
    allAchievements,
    recentActivity,
    xpToNextLevel,
    xpProgressPercent,
    loadFromStorage,
    loadCurriculum,
    completeLesson,
    submitQuiz,
    submitChallenge,
  }
})
