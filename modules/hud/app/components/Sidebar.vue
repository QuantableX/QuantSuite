<template>
  <div class="sidebar-wrapper">
    <!-- Burger Menu Button -->
    <button
      class="burger-btn btn btn-icon"
      :class="{ active: isOpen }"
      @click="toggleSidebar"
      title="Toggle menu"
    >
      <svg
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2.5"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <line x1="3" y1="6" x2="21" y2="6" />
        <line x1="3" y1="12" x2="21" y2="12" />
        <line x1="3" y1="18" x2="21" y2="18" />
      </svg>
    </button>

    <!-- Sidebar Panel -->
    <div class="sidebar-panel" :class="{ open: isOpen, right: isRight }">
      <div class="sidebar-header">
        <h3>Modules</h3>
        <button class="close-btn" @click="toggleSidebar">
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>

      <div class="sidebar-tabs">
        <!-- Basic mode: only general modules (with home) -->
        <template v-if="displayMode === 'basic'">
          <button
            v-for="module in [homeModule, ...generalModules]"
            :key="module.id"
            class="tab-btn"
            :class="{ active: activeModule === module.id }"
            @click="selectModule(module.id)"
          >
            <span class="tab-icon" v-html="module.icon"></span>
            <span class="tab-label">{{ module.label }}</span>
          </button>
        </template>

        <!-- Pro mode: sections with collapsible dividers -->
        <template v-else>
          <!-- Home button above sections -->
          <button
            class="tab-btn"
            :class="{ active: activeModule === 'home' }"
            @click="selectModule('home')"
          >
            <span class="tab-icon" v-html="homeModule.icon"></span>
            <span class="tab-label">{{ homeModule.label }}</span>
          </button>

          <div
            class="section-divider"
            @click="generalCollapsed = !generalCollapsed"
          >
            <span class="divider-line"></span>
            <span class="divider-label"
              >{{ generalCollapsed ? "▶" : "▼" }} General</span
            >
            <span class="divider-line"></span>
          </div>
          <template v-if="!generalCollapsed">
            <button
              v-for="module in generalModules"
              :key="module.id"
              class="tab-btn"
              :class="{ active: activeModule === module.id }"
              @click="selectModule(module.id)"
            >
              <span class="tab-icon" v-html="module.icon"></span>
              <span class="tab-label">{{ module.label }}</span>
            </button>
          </template>

          <div
            class="section-divider"
            @click="advancedCollapsed = !advancedCollapsed"
          >
            <span class="divider-line"></span>
            <span class="divider-label"
              >{{ advancedCollapsed ? "▶" : "▼" }} Advanced</span
            >
            <span class="divider-line"></span>
          </div>
          <template v-if="!advancedCollapsed">
            <button
              v-for="module in advancedModules"
              :key="module.id"
              class="tab-btn"
              :class="{ active: activeModule === module.id }"
              @click="selectModule(module.id)"
            >
              <span class="tab-icon" v-html="module.icon"></span>
              <span class="tab-label">{{ module.label }}</span>
            </button>
          </template>
        </template>
      </div>

      <div class="sidebar-footer">
        <button
          class="tab-btn settings-tab"
          :class="{ active: activeModule === 'settings' }"
          @click="selectModule('settings')"
        >
          <span class="tab-icon">
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <path
                d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"
              />
              <circle cx="12" cy="12" r="3" />
            </svg>
          </span>
          <span class="tab-label">Settings</span>
        </button>
      </div>
    </div>

    <!-- Overlay -->
    <div v-if="isOpen" class="sidebar-overlay" @click="toggleSidebar"></div>
  </div>
</template>

<script setup lang="ts">
import type { DisplayMode } from "#hud/composables/useConfig";

const isOpen = ref(false);
const generalCollapsed = ref(false);
const advancedCollapsed = ref(false);

const props = defineProps<{
  activeModule: string;
  windowPosition?: string;
  displayMode: DisplayMode;
}>();

const emit = defineEmits<{
  "update:activeModule": [moduleId: string];
}>();

const isRight = computed(() => props.windowPosition === "right");

const generalIds = [
  "notes",
  "todos",
  "worldclock",
  "calendar",
  "gen-calc",
  "colorpicker",
  "clipboard",
  "screenshots",
  "transcript",
  "shortcuts",
];

const allModules = [
  {
    id: "home",
    label: "Home",
    icon: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>',
  },
  {
    id: "notes",
    label: "Notes",
    icon: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/></svg>',
  },
  {
    id: "todos",
    label: "Todo List",
    icon: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 11l3 3L22 4"/><path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11"/></svg>',
  },
  {
    id: "position-calc",
    label: "Position Sizer",
    icon: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="4" y="2" width="16" height="20" rx="2"/><line x1="8" y1="6" x2="16" y2="6"/><line x1="8" y1="10" x2="16" y2="10"/><line x1="8" y1="14" x2="16" y2="14"/></svg>',
  },
  {
    id: "chart-analyzer",
    label: "Chart Analyzer",
    icon: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 3v18h18"/><path d="M7 16l4-8 4 6 4-4"/></svg>',
  },
  {
    id: "worldclock",
    label: "Clock",
    icon: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>',
  },
  {
    id: "calendar",
    label: "Calendar",
    icon: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="4" width="18" height="18" rx="2" ry="2"/><line x1="16" y1="2" x2="16" y2="6"/><line x1="8" y1="2" x2="8" y2="6"/><line x1="3" y1="10" x2="21" y2="10"/></svg>',
  },
  {
    id: "gen-calc",
    label: "Calculator",
    icon: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="4" y="2" width="16" height="20" rx="2"/><line x1="8" y1="6" x2="16" y2="6"/><line x1="8" y1="10" x2="10" y2="10"/><line x1="14" y1="10" x2="16" y2="10"/><line x1="8" y1="14" x2="10" y2="14"/><line x1="14" y1="14" x2="16" y2="14"/><line x1="8" y1="18" x2="16" y2="18"/></svg>',
  },
  {
    id: "colorpicker",
    label: "Color Picker",
    icon: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2C6.5 2 2 6.5 2 12s4.5 10 10 10c1.1 0 2-.9 2-2v-.7c0-.5-.2-1-.5-1.3-.3-.3-.5-.8-.5-1.3 0-1.1.9-2 2-2h2.3c3 0 5.7-2.5 5.7-5.7C23 5.1 18.1 2 12 2z"/><circle cx="8" cy="10" r="1.5" fill="currentColor" stroke="none"/><circle cx="12" cy="7" r="1.5" fill="currentColor" stroke="none"/><circle cx="16" cy="10" r="1.5" fill="currentColor" stroke="none"/><circle cx="10" cy="14" r="1.5" fill="currentColor" stroke="none"/></svg>',
  },
  {
    id: "clipboard",
    label: "Clipboard",
    icon: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/><rect x="8" y="2" width="8" height="4" rx="1" ry="1"/></svg>',
  },
  {
    id: "screenshots",
    label: "Screenshots",
    icon: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/></svg>',
  },
  {
    id: "transcript",
    label: "Transcript",
    icon: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="22"/></svg>',
  },
  {
    id: "shortcuts",
    label: "Shortcuts",
    icon: '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>',
  },
];

const homeModule = computed(() => allModules.find((m) => m.id === "home")!);
const generalModules = computed(() =>
  allModules.filter((m) => generalIds.includes(m.id)),
);
const advancedModules = computed(() =>
  allModules.filter((m) => m.id !== "home" && !generalIds.includes(m.id)),
);

function toggleSidebar() {
  isOpen.value = !isOpen.value;
}

function selectModule(moduleId: string) {
  emit("update:activeModule", moduleId);
  isOpen.value = false;
}
</script>

<style scoped>
.sidebar-wrapper {
  display: inline-block;
}

.burger-btn {
  background: var(--btn-primary);
}

.burger-btn:hover {
  background: var(--btn-primary-hover);
}

.burger-btn.active {
  background: var(--accent-blue);
}

.sidebar-panel {
  position: fixed;
  top: 0;
  left: -280px;
  width: 280px;
  height: 100vh;
  background: var(--bg-secondary);
  border-right: 1px solid var(--border-color);
  transition:
    left 0.3s ease,
    right 0.3s ease;
  z-index: 1002;
  display: flex;
  flex-direction: column;
}

.sidebar-panel.right {
  left: auto;
  right: -280px;
  border-right: none;
  border-left: 1px solid var(--border-color);
}

.sidebar-panel.open {
  left: 0;
}

.sidebar-panel.right.open {
  left: auto;
  right: 0;
}

.sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px;
  border-bottom: 1px solid var(--border-color);
}

.sidebar-header h3 {
  font-size: 18px;
  font-weight: 700;
  color: var(--text-primary);
  margin: 0;
}

.close-btn {
  width: 32px;
  height: 32px;
  padding: 0;
  background: transparent;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  transition: all 0.15s ease;
}

.close-btn:hover {
  background: var(--bg-card);
  color: var(--text-primary);
}

.sidebar-tabs {
  flex: 1;
  padding: 12px;
  overflow-y: auto;
}

.sidebar-footer {
  padding: 12px;
  background: var(--bg-primary);
}

.tab-btn {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  margin-bottom: 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.15s ease;
  text-align: left;
}

.settings-tab {
  margin-bottom: 0;
}

.tab-btn:hover {
  background: var(--input-bg);
  border-color: var(--accent-blue);
  color: var(--text-primary);
}

.tab-btn.active {
  background: var(--accent-blue);
  border-color: var(--accent-blue);
  color: white;
}

.tab-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.tab-label {
  font-size: 14px;
  font-weight: 500;
}

.section-divider {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 12px 0 8px;
  cursor: pointer;
  user-select: none;
}

.divider-line {
  flex: 1;
  height: 1px;
  background: var(--border-color);
}

.divider-label {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  white-space: nowrap;
}

.section-divider:hover .divider-label {
  color: var(--text-primary);
}

.section-divider:hover .divider-line {
  background: var(--text-secondary);
}

.sidebar-overlay {
  position: fixed;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  background: rgba(0, 0, 0, 0.5);
  z-index: 1000;
}
</style>
