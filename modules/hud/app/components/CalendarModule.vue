<template>
  <div class="cal-module">
    <div class="hud-top-group">
    <div class="cal-header">
      <button class="ctrl-btn" @click="prevMonth">◀</button>
      <span class="cal-title"
        >{{ monthNames[currentMonth] }} {{ currentYear }}</span
      >
      <button class="ctrl-btn" @click="nextMonth">▶</button>
    </div>
    <div class="cal-grid">
      <span v-for="d in dayLabels" :key="d" class="cal-day-label">{{ d }}</span>
      <span
        v-for="blank in firstDay"
        :key="'b' + blank"
        class="cal-cell empty"
      ></span>
      <button
        v-for="day in daysInMonth"
        :key="day"
        :class="[
          'cal-cell',
          {
            selected: isSelected(day),
            today: isToday(day),
            'has-apt': hasApt(day),
          },
        ]"
        @click="selectDay(day)"
      >
        {{ day }}
      </button>
    </div>
    </div>
    <div class="apt-section">
      <h4 class="apt-date">{{ selectedDate || "Select a date" }}</h4>
      <div v-if="selectedDate" class="apt-list">
        <div v-for="apt in dayAppointments" :key="apt.id" class="apt-row">
          <span class="apt-color" :style="{ background: apt.color }"></span>
          <div class="apt-info">
            <span class="apt-title">{{ apt.title }}</span>
            <span class="apt-time">{{ apt.time || "All day" }}</span>
          </div>
          <button class="ctrl-btn ctrl-del" @click="removeAppointment(apt.id)">
            ✕
          </button>
        </div>
        <div v-if="showAdd" class="add-apt-form">
          <div class="apt-form-row">
            <input
              v-model="newTitle"
              class="input sm"
              placeholder="Title"
              style="flex: 1"
            />
            <button class="btn btn-ghost btn-sm" @click="showAdd = false">
              ✕
            </button>
          </div>
          <div class="apt-form-row">
            <input
              v-model="newTime"
              class="input sm"
              type="time"
              style="flex: 1"
            />
            <select v-model="newColor" class="input sm" style="width: 50px">
              <option
                v-for="c in COLORS"
                :key="c"
                :value="c"
                :style="{ color: c }"
              >
                ●
              </option>
            </select>
            <button class="btn btn-green btn-sm" @click="doAdd">Add</button>
          </div>
        </div>
        <button v-else class="btn btn-primary add-btn" @click="showAdd = true">
          + Appointment
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useCalendar } from '#hud/composables/useCalendar'
const {
  appointments,
  selectedDate,
  currentMonth,
  currentYear,
  addAppointment,
  removeAppointment,
  getAppointmentsForDate,
  getDaysInMonth,
  getFirstDayOfMonth,
  prevMonth,
  nextMonth,
  COLORS,
} = useCalendar();
const showAdd = ref(false);
const newTitle = ref("");
const newTime = ref("");
const newColor = ref(COLORS[0]);
const monthNames = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];
const dayLabels = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"];
const daysInMonth = computed(() =>
  getDaysInMonth(currentYear.value, currentMonth.value),
);
const firstDay = computed(() =>
  getFirstDayOfMonth(currentYear.value, currentMonth.value),
);
const dayAppointments = computed(() =>
  selectedDate.value ? getAppointmentsForDate(selectedDate.value) : [],
);
function isSelected(day: number) {
  return (
    selectedDate.value ===
    `${currentYear.value}-${String(currentMonth.value + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`
  );
}
function isToday(day: number) {
  const t = new Date();
  return (
    t.getFullYear() === currentYear.value &&
    t.getMonth() === currentMonth.value &&
    t.getDate() === day
  );
}
function hasApt(day: number) {
  const d = `${currentYear.value}-${String(currentMonth.value + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
  return getAppointmentsForDate(d).length > 0;
}
function selectDay(day: number) {
  selectedDate.value = `${currentYear.value}-${String(currentMonth.value + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
}
function doAdd() {
  if (!newTitle.value.trim() || !selectedDate.value) return;
  addAppointment(
    newTitle.value.trim(),
    selectedDate.value,
    newTime.value,
    newColor.value,
  );
  newTitle.value = "";
  newTime.value = "";
  showAdd.value = false;
}
</script>

<style scoped>
.cal-module {
  padding: 4px 0;
}
.cal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}
.cal-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}
.cal-grid {
  display: grid;
  grid-template-columns: repeat(7, 1fr);
  gap: 2px;
  margin-bottom: 12px;
}
.cal-day-label {
  text-align: center;
  font-size: 10px;
  color: var(--text-secondary);
  padding: 2px 0;
}
.cal-cell {
  text-align: center;
  padding: 4px 0;
  font-size: 12px;
  border-radius: 4px;
  border: 1px solid transparent;
  background: none;
  color: var(--text-primary);
  cursor: pointer;
  position: relative;
}
.cal-cell.empty {
  cursor: default;
}
.cal-cell:hover:not(.empty) {
  background: var(--bg-card);
}
.cal-cell.selected {
  background: var(--btn-primary);
  color: #fff;
  border-color: var(--btn-primary);
}
.cal-cell.today {
  border-color: var(--accent-green);
}
.cal-cell.has-apt::after {
  content: "";
  position: absolute;
  bottom: 2px;
  left: 50%;
  transform: translateX(-50%);
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: var(--accent-blue);
}
.apt-section {
  border-top: 1px solid var(--border-color);
  padding-top: 8px;
}
.apt-date {
  font-size: 12px;
  color: var(--text-secondary);
  margin-bottom: 6px;
}
.apt-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.apt-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
}
.apt-color {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.apt-info {
  flex: 1;
  display: flex;
  flex-direction: column;
}
.apt-title {
  font-size: 12px;
  color: var(--text-primary);
}
.apt-time {
  font-size: 10px;
  color: var(--text-secondary);
}
.add-apt-form {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 6px;
}
.apt-form-row {
  display: flex;
  gap: 4px;
  align-items: center;
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
  font-size: 12px;
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 3px;
}
.ctrl-btn:hover {
  color: var(--text-primary);
}
.ctrl-del:hover {
  color: var(--accent-red);
}
</style>
