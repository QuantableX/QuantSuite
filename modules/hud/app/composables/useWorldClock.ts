export interface ClockEntry {
  id: string;
  label: string;
  timezone: string;
}

export interface TimerProfile {
  id: string;
  name: string;
  durationSec: number;
}

export interface StopwatchRound {
  id: string;
  round: number;
  splitMs: number;
  totalMs: number;
}

export interface AlarmEntry {
  id: string;
  time: string; // HH:MM:SS (24h)
  label: string;
  enabled: boolean;
}

const WORLDCLOCK_KEY = "quanthud_worldclock";
// Timer/stopwatch readouts show centiseconds, so a sub-100ms tick is required
// while they are on screen — but the overlay composits at ~60Hz, so ticking
// faster than this is wasted.
const TICK_MS = 33;
// Off screen the timer ticks only to fire its notification on time; the
// readout nobody sees does not need centisecond resolution.
const IDLE_TICK_MS = 1000;

function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}

const DEFAULT_CLOCKS: ClockEntry[] = [
  { id: "utc", label: "UTC", timezone: "UTC" },
  { id: "ny", label: "New York", timezone: "America/New_York" },
  { id: "london", label: "London", timezone: "Europe/London" },
  { id: "tokyo", label: "Tokyo", timezone: "Asia/Tokyo" },
];

const DEFAULT_PROFILES: TimerProfile[] = [
  { id: "default-30m", name: "30 min", durationSec: 1800 },
];

// ── Shared singleton state so alarms, timer and stopwatch survive the
//    component unmount that a HUD module switch causes ──
const clocks = ref<ClockEntry[]>([...DEFAULT_CLOCKS]);
const timerProfiles = ref<TimerProfile[]>([...DEFAULT_PROFILES]);
const activeProfileId = ref<string>(
  DEFAULT_PROFILES.length > 0 ? DEFAULT_PROFILES[0].id : "",
);
const timerRemainingMs = ref(0);
const timerRunning = ref(false);
const stopwatchElapsedMs = ref(0);
const stopwatchRunning = ref(false);
const stopwatchRounds = ref<StopwatchRound[]>([]);
const alarms = ref<AlarmEntry[]>([]);

let timerInterval: ReturnType<typeof setInterval> | null = null;
let stopwatchInterval: ReturnType<typeof setInterval> | null = null;
let alarmInterval: ReturnType<typeof setInterval> | null = null;
let stopwatchStart = 0;
let timerStart = 0;
let timerSnapshot = 0;
let timerLabel = "Timer"; // profile name captured when the timer was started
let displayMounted = 0; // World Clock panels currently on screen
let _lastAlarmSecond = ""; // prevent re-firing same alarm in the same second
let _initialized = false;
let _syncCleanup: (() => void) | null = null;

// --- Persistence ---
async function load() {
  try {
    if (typeof window !== "undefined" && (window as any).__TAURI__) {
      const { invoke } = await import("@tauri-apps/api/core");
      const saved = await invoke<string>("plugin:hud|load_config");
      if (saved) {
        const config = JSON.parse(saved);
        if (config._worldclock) {
          clocks.value = config._worldclock.clocks || [...DEFAULT_CLOCKS];
          timerProfiles.value = config._worldclock.profiles || [
            ...DEFAULT_PROFILES,
          ];
          alarms.value = config._worldclock.alarms || [];
          if (timerProfiles.value.length > 0) {
            activeProfileId.value = timerProfiles.value[0].id;
          }
          return;
        }
      }
    }
    const saved = localStorage.getItem(WORLDCLOCK_KEY);
    if (saved) {
      const data = JSON.parse(saved);
      clocks.value = data.clocks || [...DEFAULT_CLOCKS];
      timerProfiles.value = data.profiles || [...DEFAULT_PROFILES];
      alarms.value = data.alarms || [];
      if (timerProfiles.value.length > 0) {
        activeProfileId.value = timerProfiles.value[0].id;
      }
    }
  } catch (e) {
    console.warn("Failed to load worldclock:", e);
  }
}

async function save() {
  try {
    const payload = {
      clocks: clocks.value,
      profiles: timerProfiles.value,
      alarms: alarms.value,
    };
    if (typeof window !== "undefined" && (window as any).__TAURI__) {
      await updateConfigFile("worldclock", (config) => {
        config._worldclock = payload;
      });
    } else {
      localStorage.setItem(WORLDCLOCK_KEY, JSON.stringify(payload));
    }
  } catch (e) {
    console.warn("Failed to save worldclock:", e);
  }
}

function addClock(label: string, timezone: string) {
  clocks.value.push({ id: generateId(), label, timezone });
  save();
}
function removeClock(id: string) {
  clocks.value = clocks.value.filter((c) => c.id !== id);
  save();
}
function addProfile(name: string, durationSec: number) {
  timerProfiles.value.push({ id: generateId(), name, durationSec });
  save();
}
function removeProfile(id: string) {
  timerProfiles.value = timerProfiles.value.filter((p) => p.id !== id);
  if (activeProfileId.value === id && timerProfiles.value.length > 0) {
    activeProfileId.value = timerProfiles.value[0].id;
  }
  save();
}

// Alarm CRUD
function addAlarm(time: string, label: string) {
  alarms.value.push({ id: generateId(), time, label, enabled: true });
  save();
}
function removeAlarm(id: string) {
  alarms.value = alarms.value.filter((a) => a.id !== id);
  save();
}
function toggleAlarm(id: string) {
  const a = alarms.value.find((a) => a.id === id);
  if (a) a.enabled = !a.enabled;
  save();
}

// Alarm checker (runs every second)
function startAlarmChecker() {
  if (alarmInterval) return;
  alarmInterval = setInterval(async () => {
    const now = new Date();
    const hhmmss =
      String(now.getHours()).padStart(2, "0") +
      ":" +
      String(now.getMinutes()).padStart(2, "0") +
      ":" +
      String(now.getSeconds()).padStart(2, "0");
    const key = hhmmss + ":" + now.getDate(); // unique per day-second
    if (key === _lastAlarmSecond) return;
    for (const alarm of alarms.value) {
      if (alarm.enabled && alarm.time === hhmmss) {
        _lastAlarmSecond = key;
        try {
          const { invoke } = await import("@tauri-apps/api/core");
          invoke("plugin:hud|show_notification_popup", {
            message: `⏰ Alarm: ${alarm.label || alarm.time}`,
          });
        } catch {
          // fallback: ignore if not in Tauri
        }
      }
    }
  }, 1000);
}
function stopAlarmChecker() {
  if (alarmInterval) {
    clearInterval(alarmInterval);
    alarmInterval = null;
  }
}

// Timer
async function timerTick() {
  const elapsed = Date.now() - timerStart;
  timerRemainingMs.value = Math.max(0, timerSnapshot - elapsed);
  if (timerRemainingMs.value <= 0) {
    stopTimer();
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      invoke("plugin:hud|show_notification_popup", {
        message: `⏱ ${timerLabel} finished!`,
      });
    } catch {
      // fallback: ignore if not in Tauri
    }
  }
}

// The timer owes the user a notification, so it ticks on regardless — only its
// rate follows whether anyone is reading the countdown.
function armTimerInterval() {
  if (timerInterval) clearInterval(timerInterval);
  timerInterval = setInterval(
    timerTick,
    displayMounted > 0 ? TICK_MS : IDLE_TICK_MS,
  );
}

function startTimer() {
  if (timerRunning.value) return;
  const profile = timerProfiles.value.find(
    (p) => p.id === activeProfileId.value,
  );
  if (!profile) return;
  if (timerRemainingMs.value <= 0)
    timerRemainingMs.value = profile.durationSec * 1000;
  timerRunning.value = true;
  timerSnapshot = timerRemainingMs.value;
  timerStart = Date.now();
  timerLabel = profile.name || "Timer";
  armTimerInterval();
}
function stopTimer() {
  timerRunning.value = false;
  if (timerInterval) {
    clearInterval(timerInterval);
    timerInterval = null;
  }
}
function resetTimer() {
  stopTimer();
  const profile = timerProfiles.value.find(
    (p) => p.id === activeProfileId.value,
  );
  timerRemainingMs.value = profile ? profile.durationSec * 1000 : 0;
}

// Stopwatch — unlike the timer it has no completion event: `stopwatchStart`
// holds the truth and the interval only refreshes the readout, so it may sleep
// while the panel is off screen.
function armStopwatchInterval() {
  if (stopwatchInterval) {
    clearInterval(stopwatchInterval);
    stopwatchInterval = null;
  }
  if (!stopwatchRunning.value || displayMounted === 0) return;
  stopwatchElapsedMs.value = Date.now() - stopwatchStart;
  stopwatchInterval = setInterval(() => {
    stopwatchElapsedMs.value = Date.now() - stopwatchStart;
  }, TICK_MS);
}

function startStopwatch() {
  if (stopwatchRunning.value) return;
  stopwatchRunning.value = true;
  stopwatchStart = Date.now() - stopwatchElapsedMs.value;
  armStopwatchInterval();
}
function stopStopwatch() {
  // The readout can be stale by a whole off-screen stretch — settle it here
  if (stopwatchRunning.value) {
    stopwatchElapsedMs.value = Date.now() - stopwatchStart;
  }
  stopwatchRunning.value = false;
  if (stopwatchInterval) {
    clearInterval(stopwatchInterval);
    stopwatchInterval = null;
  }
}
function addStopwatchRound() {
  const currentMs = stopwatchElapsedMs.value;
  const lastTotal =
    stopwatchRounds.value.length > 0
      ? stopwatchRounds.value[stopwatchRounds.value.length - 1].totalMs
      : 0;
  stopwatchRounds.value.push({
    id: generateId(),
    round: stopwatchRounds.value.length + 1,
    splitMs: currentMs - lastTotal,
    totalMs: currentMs,
  });
}

function clearStopwatchRounds() {
  stopwatchRounds.value = [];
}

function resetStopwatch() {
  stopStopwatch();
  stopwatchElapsedMs.value = 0;
  clearStopwatchRounds();
}

/** Call once at app startup (e.g. in index.vue onMounted) — an alarm has to
 *  fire while another HUD module is on screen, so this never binds to the
 *  World Clock component's lifecycle. */
export async function initWorldClock() {
  if (_initialized) return;
  _initialized = true;
  await load();
  // In dual mode both overlays run this init; the popup must appear once, so
  // the secondary window stays in sync but leaves the alarm to the primary.
  if (!(await isDualRightWindow())) startAlarmChecker();
  _syncCleanup = await onSyncEvent("worldclock", load);
}

/** Counterpart to initWorldClock — the overlay page calls this when it goes
 *  away; a later init re-arms everything from scratch. */
export function disposeWorldClock() {
  stopTimer();
  stopStopwatch();
  stopAlarmChecker();
  _syncCleanup?.();
  _syncCleanup = null;
  _initialized = false;
}

/** The readouts only exist while a World Clock panel is mounted — the ticks
 *  follow it, the underlying state keeps running either way. */
function attachDisplay() {
  displayMounted++;
  if (displayMounted === 1) {
    armStopwatchInterval();
    if (timerRunning.value) armTimerInterval();
  }
}
function detachDisplay() {
  displayMounted = Math.max(0, displayMounted - 1);
  if (displayMounted === 0) {
    armStopwatchInterval();
    if (timerRunning.value) armTimerInterval();
  }
}

export function useWorldClock() {
  // Guard covers the case where the component is reached before startup init
  onMounted(() => {
    initWorldClock();
    attachDisplay();
  });
  onUnmounted(detachDisplay);

  return {
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
    save,
  };
}
