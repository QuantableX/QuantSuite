<template>
  <div class="wc-module">
    <div class="wc-tabs-wrapper">
      <div class="wc-tabs">
        <button :class="{ active: tab === 'clocks' }" @click="tab = 'clocks'">
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            style="vertical-align: middle; margin-right: 3px"
          >
            <circle cx="12" cy="12" r="10" />
            <polyline points="12 6 12 12 16 14" />
          </svg>
          Clocks
        </button>
      </div>
      <div class="wc-tabs">
        <button :class="{ active: tab === 'timer' }" @click="tab = 'timer'">
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            style="vertical-align: middle; margin-right: 3px"
          >
            <circle cx="12" cy="13" r="8" />
            <path d="M12 9v4l2 2" />
            <path d="M5 3L2 6" />
            <path d="M22 6l-3-3" />
            <line x1="12" y1="1" x2="12" y2="3" />
            <line x1="10" y1="1" x2="14" y2="1" />
          </svg>
          Timer
        </button>
        <button
          :class="{ active: tab === 'stopwatch' }"
          @click="tab = 'stopwatch'"
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            style="vertical-align: middle; margin-right: 3px"
          >
            <circle cx="12" cy="13" r="8" />
            <path d="M12 9v4" />
            <line x1="12" y1="1" x2="12" y2="3" />
            <line x1="10" y1="1" x2="14" y2="1" />
          </svg>
          Stopwatch
        </button>
        <button :class="{ active: tab === 'alarm' }" @click="tab = 'alarm'">
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
            style="vertical-align: middle; margin-right: 3px"
          >
            <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
            <path d="M13.73 21a2 2 0 0 1-3.46 0" />
          </svg>
          Alarm
        </button>
      </div>
    </div>
    <div v-if="tab === 'clocks'" class="wc-panel">
      <div class="clock-list">
        <div v-for="clock in clocks" :key="clock.id" class="clock-row">
          <div class="clock-info">
            <span class="clock-label">{{ clock.label }}</span>
            <span class="clock-time">{{ getTime(clock.timezone) }}</span>
          </div>
          <button
            class="ctrl-btn ctrl-del"
            @click="removeClock(clock.id)"
            title="Remove"
          >
            ✕
          </button>
        </div>
      </div>
      <div v-if="showAddClock" class="add-form">
        <input v-model="newClockLabel" class="input sm" placeholder="Label" />
        <select v-model="newClockTz" class="input sm">
          <option v-for="tz in utcOffsets" :key="tz.value" :value="tz.value">
            {{ tz.label }}
          </option>
        </select>
        <button class="btn btn-green btn-sm" @click="doAddClock">Add</button>
        <button class="btn btn-ghost btn-sm" @click="showAddClock = false">
          ✕
        </button>
      </div>
      <button
        v-else
        class="btn btn-primary add-btn"
        @click="showAddClock = true"
      >
        + Add Clock
      </button>
    </div>
    <div v-if="tab === 'timer'" class="wc-panel">
      <div class="hud-top-group">
      <div class="profile-row">
        <select
          v-model="activeProfileId"
          class="input sm profile-select"
          @change="onProfileChange"
        >
          <option v-for="p in timerProfiles" :key="p.id" :value="p.id">
            {{ p.name }} ({{ formatSec(p.durationSec) }})
          </option>
        </select>
        <button
          v-if="activeProfileId"
          class="ctrl-btn ctrl-del"
          @click="removeProfile(activeProfileId)"
          title="Delete profile"
        >
          ✕
        </button>
      </div>
      <div class="timer-display">{{ formatMs(timerRemainingMs) }}</div>
      <div class="timer-controls">
        <button
          class="btn btn-green btn-sm"
          @click="startTimer"
          :disabled="timerRunning"
        >
          ▶
        </button>
        <button
          class="btn btn-red btn-sm"
          @click="stopTimer"
          :disabled="!timerRunning"
        >
          ||
        </button>
        <button class="btn btn-primary btn-sm" @click="resetTimer">↺</button>
      </div>
      </div>
      <div v-if="showAddProfile" class="add-form-wrap">
        <input v-model="newProfileName" class="input sm" placeholder="Name" />
        <div class="add-form-row">
          <input
            v-model="newProfileTime"
            class="input sm time-input"
            type="time"
            step="1"
          />
          <button class="btn btn-green btn-sm btn-sq" @click="doAddProfile">
            ✓
          </button>
          <button
            class="btn btn-ghost btn-sm btn-sq"
            @click="showAddProfile = false"
          >
            ✕
          </button>
        </div>
      </div>
      <button
        v-else
        class="btn btn-primary add-btn"
        @click="showAddProfile = true"
      >
        + Add Profile
      </button>
    </div>
    <div v-if="tab === 'stopwatch'" class="wc-panel">
      <div class="hud-top-group">
      <div class="timer-display">{{ formatMs(stopwatchElapsedMs) }}</div>
      <div class="timer-controls">
        <button
          class="btn btn-green btn-sm"
          @click="startStopwatch"
          :disabled="stopwatchRunning"
        >
          ▶
        </button>
        <button
          class="btn btn-red btn-sm"
          @click="stopStopwatch"
          :disabled="!stopwatchRunning"
        >
          ||
        </button>
        <button
          class="btn btn-primary btn-sm"
          @click="addStopwatchRound"
          :disabled="stopwatchElapsedMs === 0"
          title="Record round"
        >
          ⏱
        </button>
        <button class="btn btn-primary btn-sm" @click="resetStopwatch">
          ↺
        </button>
      </div>
      </div>
      <!-- Rounds list -->
      <div v-if="stopwatchRounds.length" class="rounds-wrapper">
        <div class="rounds-header">
          <span class="rounds-title">Rounds</span>
          <button
            class="ctrl-btn ctrl-del"
            @click="clearStopwatchRounds"
            title="Clear rounds"
          >
            ✕
          </button>
        </div>
        <div class="rounds-list">
          <div
            v-for="round in [...stopwatchRounds].reverse()"
            :key="round.id"
            class="round-row"
          >
            <span class="round-num">#{{ round.round }}</span>
            <span class="round-split">{{ formatMs(round.splitMs) }}</span>
            <span class="round-total">{{ formatMs(round.totalMs) }}</span>
          </div>
        </div>
      </div>
    </div>
    <!-- Alarm Panel -->
    <div v-if="tab === 'alarm'" class="wc-panel">
      <div class="alarm-list">
        <div v-for="alarm in alarms" :key="alarm.id" class="alarm-row">
          <button
            class="alarm-toggle"
            :class="{ on: alarm.enabled }"
            @click="toggleAlarm(alarm.id)"
            :title="alarm.enabled ? 'Disable' : 'Enable'"
          >
            {{ alarm.enabled ? "●" : "○" }}
          </button>
          <span class="alarm-time">{{ alarm.time }}</span>
          <span class="alarm-label">{{ alarm.label }}</span>
          <button
            class="ctrl-btn ctrl-del"
            @click="removeAlarm(alarm.id)"
            title="Delete"
          >
            ✕
          </button>
        </div>
        <div v-if="alarms.length === 0" class="alarm-empty">No alarms set</div>
      </div>
      <div v-if="showAddAlarm" class="add-form-wrap">
        <input v-model="newAlarmLabel" class="input sm" placeholder="Label" />
        <div class="add-form-row">
          <input
            v-model="newAlarmTime"
            class="input sm time-input"
            type="time"
            step="1"
          />
          <button class="btn btn-green btn-sm btn-sq" @click="doAddAlarm">
            ✓
          </button>
          <button
            class="btn btn-ghost btn-sm btn-sq"
            @click="showAddAlarm = false"
          >
            ✕
          </button>
        </div>
      </div>
      <button
        v-else
        class="btn btn-primary add-btn"
        @click="showAddAlarm = true"
      >
        + Add Alarm
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useWorldClock } from '#hud/composables/useWorldClock'
const {
  clocks,
  timerProfiles,
  activeProfileId,
  timerRemainingMs,
  timerRunning,
  stopwatchElapsedMs,
  stopwatchRunning,
  stopwatchRounds,
  alarms,
  addClock,
  removeClock,
  addProfile,
  removeProfile,
  addAlarm,
  removeAlarm,
  toggleAlarm,
  startTimer,
  stopTimer,
  resetTimer,
  startStopwatch,
  stopStopwatch,
  resetStopwatch,
  addStopwatchRound,
  clearStopwatchRounds,
} = useWorldClock();
const tab = ref<"clocks" | "timer" | "stopwatch" | "alarm">("clocks");
const showAddClock = ref(false);
const newClockLabel = ref("");
const newClockTz = ref("Etc/GMT");
const showAddProfile = ref(false);
const newProfileName = ref("");
const newProfileTime = ref("00:25:00");
const showAddAlarm = ref(false);
const newAlarmTime = ref("08:00:00");
const newAlarmLabel = ref("");
const now = ref(new Date());
let clockTick: ReturnType<typeof setInterval> | null = null;
onMounted(() => {
  clockTick = setInterval(() => {
    now.value = new Date();
  }, 1000);
});
onUnmounted(() => {
  if (clockTick) clearInterval(clockTick);
});
const utcOffsets = (() => {
  const offsets: { label: string; value: string }[] = [];
  for (let i = -12; i <= 14; i++) {
    const sign = i > 0 ? "+" : i < 0 ? "" : "";
    offsets.push({
      label: i === 0 ? "UTC" : `UTC ${sign}${i}`,
      value: i === 0 ? "Etc/GMT" : `Etc/GMT${i > 0 ? "-" : "+"}${Math.abs(i)}`,
    });
  }
  return offsets;
})();
function getTime(tz: string): string {
  return now.value.toLocaleTimeString("en-US", {
    timeZone: tz,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}
function formatSec(s: number): string {
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`
    : `${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
}
function doAddClock() {
  if (!newClockLabel.value.trim() || !newClockTz.value) return;
  addClock(newClockLabel.value.trim(), newClockTz.value);
  newClockLabel.value = "";
  showAddClock.value = false;
}
function doAddProfile() {
  const parts = (newProfileTime.value || "0:0:0").split(":").map(Number);
  const totalSec =
    (parts[0] || 0) * 3600 + (parts[1] || 0) * 60 + (parts[2] || 0);
  if (!newProfileName.value.trim() || totalSec <= 0) return;
  addProfile(newProfileName.value.trim(), totalSec);
  newProfileName.value = "";
  newProfileTime.value = "00:25:00";
  showAddProfile.value = false;
}
function doAddAlarm() {
  const time = newAlarmTime.value || "08:00:00";
  // Ensure HH:MM:SS format
  const parts = time.split(":");
  const hh = (parts[0] || "00").padStart(2, "0");
  const mm = (parts[1] || "00").padStart(2, "0");
  const ss = (parts[2] || "00").padStart(2, "0");
  addAlarm(`${hh}:${mm}:${ss}`, newAlarmLabel.value.trim());
  newAlarmTime.value = "08:00:00";
  newAlarmLabel.value = "";
  showAddAlarm.value = false;
}
function onProfileChange() {
  resetTimer();
}
function formatMs(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const centis = Math.floor((ms % 1000) / 10);
  const h = Math.floor(totalSec / 3600);
  const m = Math.floor((totalSec % 3600) / 60);
  const s = totalSec % 60;
  const cc = String(centis).padStart(2, "0");
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}.${cc}`
    : `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}.${cc}`;
}
</script>

<style scoped>
.wc-module {
  display: flex;
  flex-direction: column;
  padding: 4px 0;
  min-height: 0;
  flex: 1;
}
.wc-tabs-wrapper {
  flex-shrink: 0;
  margin-bottom: 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.wc-tabs {
  display: flex;
  gap: 6px;
  padding-right: 4px;
  flex-shrink: 0;
}
.wc-tabs button {
  flex: 1;
  padding: 6px 4px;
  font-size: 11px;
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-secondary);
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.15s ease;
}
.wc-tabs button.active {
  background: var(--btn-primary);
  color: var(--text-primary);
  border-color: var(--btn-primary);
}
.wc-panel {
  min-height: 60px;
  padding-right: 4px;
  display: flex;
  flex-direction: column;
  flex: 1;
}
.clock-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 8px;
}
.clock-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
}
.clock-info {
  display: flex;
  flex-direction: column;
}
.clock-label {
  font-size: 11px;
  color: var(--text-secondary);
}
.clock-time {
  font-size: 16px;
  font-weight: 600;
  font-family: monospace;
  color: var(--text-primary);
}
.timer-display {
  text-align: center;
  font-size: 36px;
  font-weight: 700;
  font-family: monospace;
  color: var(--accent-green);
  flex-shrink: 0;
  padding: 16px 0;
}
.timer-controls {
  display: flex;
  justify-content: center;
  gap: 8px;
  margin-bottom: 12px;
  flex-shrink: 0;
}
.profile-row {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-bottom: 8px;
}
.profile-select {
  flex: 1;
}
.add-form {
  display: flex;
  gap: 4px;
  align-items: center;
  margin-top: 8px;
}
.add-form-wrap {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 8px;
}
.add-form-row {
  display: flex;
  gap: 4px;
  align-items: center;
}
.time-input {
  flex: 1 1 0 !important;
  min-width: 0 !important;
  text-align: center;
  padding: 4px 4px !important;
  color-scheme: dark;
}
.btn-sq {
  width: 28px;
  height: 28px;
  padding: 0;
  flex-shrink: 0;
}
.add-btn {
  width: 100%;
  font-size: 12px;
  padding: 5px 10px;
  margin-top: 6px;
}
.input.sm {
  font-size: 12px;
  padding: 4px 8px;
}
.btn-sm {
  font-size: 12px;
  padding: 4px 10px;
}
.ctrl-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 10px;
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 3px;
}
.ctrl-del:hover {
  color: var(--accent-red);
}
/* Rounds wrapper */
.rounds-wrapper {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
  margin-top: 8px;
}
.rounds-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 2px 6px 4px;
  flex-shrink: 0;
}
/* Rounds list - styled like screenshot list */
.rounds-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  overflow-y: auto;
  min-height: 0;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 4px;
  background: var(--bg-secondary);
}
.rounds-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
}
.round-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  flex-shrink: 0;
}
.round-row:hover {
  background: var(--bg-secondary);
}
.round-num {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  min-width: 28px;
}
.round-split {
  font-size: 12px;
  font-family: monospace;
  color: var(--accent-green);
  flex: 1;
}
.round-total {
  font-size: 11px;
  font-family: monospace;
  color: var(--text-secondary);
}
/* Alarm styles */
.alarm-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 8px;
}
.alarm-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
}
.alarm-toggle {
  background: none;
  border: none;
  cursor: pointer;
  font-size: 12px;
  color: var(--text-secondary);
  padding: 0;
  line-height: 1;
}
.alarm-toggle.on {
  color: var(--accent-green);
}
.alarm-time {
  font-size: 15px;
  font-weight: 600;
  font-family: monospace;
  color: var(--text-primary);
}
.alarm-label {
  font-size: 11px;
  color: var(--text-secondary);
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.alarm-empty {
  font-size: 12px;
  color: var(--text-secondary);
  text-align: center;
  padding: 16px 0;
}
</style>
