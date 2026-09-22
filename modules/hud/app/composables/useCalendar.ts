export interface Appointment {
  id: string;
  title: string;
  date: string; // YYYY-MM-DD
  time: string; // HH:mm
  color: string;
}

const CALENDAR_KEY = "quanthud_calendar";

function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}

const COLORS = [
  "#4a90d9",
  "#00aa55",
  "#cc3344",
  "#e6a817",
  "#9b59b6",
  "#e67e22",
];

export function useCalendar() {
  const appointments = ref<Appointment[]>([]);
  const selectedDate = ref(new Date().toISOString().slice(0, 10));
  const currentMonth = ref(new Date().getMonth());
  const currentYear = ref(new Date().getFullYear());

  async function load() {
    try {
      if (typeof window !== "undefined" && (window as any).__TAURI__) {
        const { invoke } = await import("@tauri-apps/api/core");
        const saved = await invoke<string>("plugin:hud|load_config");
        if (saved) {
          const config = JSON.parse(saved);
          if (config._calendar) {
            appointments.value = config._calendar;
            return;
          }
        }
      }
      const saved = localStorage.getItem(CALENDAR_KEY);
      if (saved) appointments.value = JSON.parse(saved);
    } catch (e) {
      console.warn("Failed to load calendar:", e);
    }
  }

  async function save() {
    try {
      if (typeof window !== "undefined" && (window as any).__TAURI__) {
        await updateConfigFile("calendar", (config) => {
          config._calendar = appointments.value;
        });
      } else {
        localStorage.setItem(CALENDAR_KEY, JSON.stringify(appointments.value));
      }
    } catch (e) {
      console.warn("Failed to save calendar:", e);
    }
  }

  function addAppointment(
    title: string,
    date: string,
    time: string,
    color?: string,
  ) {
    appointments.value.push({
      id: generateId(),
      title,
      date,
      time,
      color: color || COLORS[appointments.value.length % COLORS.length],
    });
    save();
  }

  function removeAppointment(id: string) {
    appointments.value = appointments.value.filter((a) => a.id !== id);
    save();
  }

  function updateAppointment(
    id: string,
    updates: Partial<Omit<Appointment, "id">>,
  ) {
    const a = appointments.value.find((a) => a.id === id);
    if (a) {
      Object.assign(a, updates);
      save();
    }
  }

  function getAppointmentsForDate(date: string): Appointment[] {
    return appointments.value
      .filter((a) => a.date === date)
      .sort((a, b) => a.time.localeCompare(b.time));
  }

  function getDaysInMonth(year: number, month: number): number {
    return new Date(year, month + 1, 0).getDate();
  }

  function getFirstDayOfMonth(year: number, month: number): number {
    return new Date(year, month, 1).getDay();
  }

  function prevMonth() {
    if (currentMonth.value === 0) {
      currentMonth.value = 11;
      currentYear.value--;
    } else {
      currentMonth.value--;
    }
  }

  function nextMonth() {
    if (currentMonth.value === 11) {
      currentMonth.value = 0;
      currentYear.value++;
    } else {
      currentMonth.value++;
    }
  }

  let _syncCleanup: (() => void) | null = null;
  let _unmounted = false;

  onMounted(async () => {
    await load();
    const cleanup = await onSyncEvent("calendar", load);
    // The awaits above can outlast the component — unlisten right away then
    if (_unmounted) cleanup();
    else _syncCleanup = cleanup;
  });

  onUnmounted(() => {
    _unmounted = true;
    _syncCleanup?.();
    _syncCleanup = null;
  });

  return {
    appointments,
    selectedDate,
    currentMonth,
    currentYear,
    addAppointment,
    removeAppointment,
    updateAppointment,
    getAppointmentsForDate,
    getDaysInMonth,
    getFirstDayOfMonth,
    prevMonth,
    nextMonth,
    COLORS,
  };
}
