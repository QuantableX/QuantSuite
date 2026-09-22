<script setup lang="ts">
/**
 * QuantSystems' left nav — the shared QSidebar (V3). Glyph characters
 * (◈ ◇ ▦) are gone: chrome uses 24×24 stroke-path line icons, the same
 * language as the rail and every other module.
 */
import { useAppStore } from '#systems/stores/app'
import { useSystemsStore } from '#systems/stores/systems'
import { useConfigStore } from '#systems/stores/config'
import { useActiveView } from '#systems/composables/useActiveView'

const app = useAppStore()
const systems = useSystemsStore()
const config = useConfigStore()
const router = useRouter()
const { systemId } = useActiveView()

/** Line icons per system id; fallback is a generic grid. */
const ICONS: Record<string, string> = {
  lces: 'M12 2l9 5-9 5-9-5 9-5z M3 12l9 5 9-5',
  sces: 'M12 3l7 4v10l-7 4-7-4V7l7-4z',
}
const FALLBACK_ICON = 'M4 4h7v7H4z M13 4h7v7h-7z M4 13h7v7H4z M13 13h7v7h-7z'

// Names redundantly spell out the acronym (e.g. "Large-Cap Evaluation System"
// for LCES); trim the trailing "System" so the sidebar label stays concise.
function displayName(name: string) {
  return name.replace(/\s*System$/i, '').trim() || name
}

function select(id: string, status: string) {
  if (status !== 'ready') return
  app.setActiveSystem(id)
  void config.load(id)
  void router.push(`/algo/manual/${id}/${app.viewFor(id)}`)
}
</script>

<template>
  <QSidebar>
    <nav class="qs-rail">
      <span class="qs-rail__label">Systems</span>
      <div class="qs-rail__group">
        <button
          v-for="(s, i) in systems.systems"
          :key="s.id"
          class="qs-rail-btn"
          :class="{
            'qs-rail-btn--active': systemId === s.id,
            'qs-rail-btn--planned': s.status !== 'ready',
          }"
          :title="`${s.name} (${s.short}) — Ctrl+${i + 1}${s.status !== 'ready' ? ' · planned' : ''}`"
          @click="select(s.id, s.status)"
        >
          <svg class="qs-rail-btn__icon" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path :d="ICONS[s.id] ?? FALLBACK_ICON" />
          </svg>
          <span class="qs-rail-btn__text">
            <span class="qs-rail-btn__short">{{ s.short }}</span>
            <span class="qs-rail-btn__name">{{ displayName(s.name) }}</span>
          </span>
          <span v-if="s.status !== 'ready'" class="qs-rail-btn__soon">soon</span>
          <span class="qs-rail-btn__bar" aria-hidden="true" />
        </button>
      </div>
    </nav>
  </QSidebar>
</template>

<style scoped>
.qs-rail {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 14px 10px;
}

.qs-rail__label {
  padding: 0 10px;
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--qs-text-muted);
}

.qs-rail__group {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.qs-rail-btn {
  position: relative;
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 9px 12px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--qs-text-secondary);
  cursor: pointer;
  text-align: left;
  transition: color 150ms ease, background 150ms ease;
}

.qs-rail-btn:hover {
  color: var(--qs-text);
  background: var(--qs-bg-hover);
}

.qs-rail-btn__icon {
  flex-shrink: 0;
  color: var(--qs-accent);
}

.qs-rail-btn__text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.qs-rail-btn__short {
  font-size: 11px;
  font-weight: 700;
  letter-spacing: 0.05em;
  color: var(--qs-accent);
}

.qs-rail-btn__name {
  font-size: 11px;
  color: var(--qs-text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.qs-rail-btn--planned {
  cursor: not-allowed;
  opacity: 0.55;
}

.qs-rail-btn__soon {
  flex-shrink: 0;
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--qs-warning);
}

.qs-rail-btn__bar {
  position: absolute;
  left: 0;
  top: 8px;
  bottom: 8px;
  width: 2px;
  border-radius: 0 2px 2px 0;
  background: transparent;
  transition: background 150ms ease;
}

.qs-rail-btn--active {
  color: var(--qs-text);
  background: var(--qs-bg-hover);
}

.qs-rail-btn--active .qs-rail-btn__short {
  color: var(--qs-accent);
}

.qs-rail-btn--active .qs-rail-btn__name {
  color: var(--qs-text);
}

.qs-rail-btn--active .qs-rail-btn__bar {
  background: var(--qs-accent);
}
</style>
