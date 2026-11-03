/**
 * Scheduled prompts (round 3, card 03): the list, each schedule's history
 * and the one being edited. The crate runs them (crate/src/schedules.rs);
 * every change there arrives as `pilot-schedules-changed`.
 */
import { defineStore } from 'pinia'
import { pilot } from '#pilot/utils/invoke'
import { blankSchedule } from '#pilot/utils/schedules'
import type { Schedule, ScheduleRun } from '#pilot/types'

export const useSchedulesStore = defineStore('pilot/schedules', {
  state: () => ({
    list: [] as Schedule[],
    loaded: false,
    /** The schedule shown on the right. */
    selected: null as string | null,
    /** The form: a copy being edited (id '' = new). */
    editing: null as Schedule | null,
    runs: {} as Record<string, ScheduleRun[]>,
    error: '',
    busy: false,
  }),
  getters: {
    current(state): Schedule | null {
      return state.list.find((s) => s.id === state.selected) ?? null
    },
  },
  actions: {
    async load() {
      try {
        this.list = (await pilot<Schedule[]>('plugin:pilot|pilot_schedules')) ?? []
        this.loaded = true
        if (this.selected && !this.list.some((s) => s.id === this.selected)) this.selected = null
        if (this.selected) void this.loadRuns(this.selected)
      } catch (e) {
        this.error = String(e)
      }
    },
    async loadRuns(id: string) {
      try {
        this.runs[id] = (await pilot<ScheduleRun[]>('plugin:pilot|pilot_schedule_runs', { id, limit: 100 })) ?? []
      } catch {
        /* the history stays as it was */
      }
    },
    select(id: string | null) {
      this.selected = id
      this.editing = null
      if (id) void this.loadRuns(id)
    },
    /** The form for a new schedule — prefilled from a thread by "Schedule this…". */
    startNew(base: Partial<Schedule> = {}) {
      this.error = ''
      this.editing = blankSchedule(base)
    },
    edit(id: string) {
      const s = this.list.find((x) => x.id === id)
      if (!s) return
      this.error = ''
      this.editing = JSON.parse(JSON.stringify(s)) as Schedule
    },
    async save(): Promise<boolean> {
      if (!this.editing) return false
      this.busy = true
      this.error = ''
      try {
        const saved = await pilot<Schedule>('plugin:pilot|pilot_schedule_save', { schedule: this.editing })
        if (saved) {
          this.editing = null
          await this.load()
          this.select(saved.id)
        }
        return true
      } catch (e) {
        this.error = e instanceof Error ? e.message : String(e)
        return false
      } finally {
        this.busy = false
      }
    },
    async setEnabled(id: string, enabled: boolean) {
      this.error = ''
      try {
        await pilot('plugin:pilot|pilot_schedule_set_enabled', { id, enabled })
        await this.load()
      } catch (e) {
        this.error = e instanceof Error ? e.message : String(e)
      }
    },
    async remove(id: string) {
      this.error = ''
      try {
        await pilot('plugin:pilot|pilot_schedule_delete', { id })
        if (this.selected === id) this.selected = null
        delete this.runs[id]
        await this.load()
      } catch (e) {
        this.error = e instanceof Error ? e.message : String(e)
      }
    },
    async runNow(id: string): Promise<ScheduleRun | null> {
      this.error = ''
      try {
        const run = await pilot<ScheduleRun>('plugin:pilot|pilot_schedule_run_now', { id })
        await this.load()
        await this.loadRuns(id)
        return run
      } catch (e) {
        this.error = e instanceof Error ? e.message : String(e)
        return null
      }
    },
  },
})
