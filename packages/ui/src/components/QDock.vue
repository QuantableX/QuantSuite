<script setup lang="ts">
/**
 * The QuantSpace footer dock — QuantCode's bottom panel chrome, extracted
 * (2026-08-27) so every module's footer bar is the SAME bar: one drag grip,
 * one tab strip, one close button, one set of colours, fonts and heights.
 * The content is CodeDock's (Problems / Terminal / Search, identical across
 * the QuantSpace modules); this component is only the chrome around it, the
 * way QSidebar and QRightPanel are for the edges.
 *
 * The host owns the height (`:height` + `@update:height`) so each module can
 * keep persisting it however it already does. The grip drags between
 * `minHeight` and `maxFraction` of the viewport, and a double-click snaps
 * back to `defaultHeight` — QuantCode's exact behaviour, now everyone's.
 */
import { ref } from 'vue'

const props = withDefaults(
  defineProps<{
    /** The strip's tabs. An empty list shows just the strip and the close. */
    tabs?: { id: string; label: string; badge?: number | string; badgeKind?: 'error' | 'normal' }[]
    /** Which tab is lit. Null lights none. */
    active?: string | null
    height: number
    defaultHeight?: number
    minHeight?: number
    /** Upper bound as a share of the viewport. */
    maxFraction?: number
    resizable?: boolean
  }>(),
  {
    tabs: () => [],
    active: null,
    defaultHeight: 240,
    minHeight: 90,
    maxFraction: 0.7,
    resizable: true,
  }
)

const emit = defineEmits<{
  (e: 'select', id: string): void
  (e: 'close'): void
  (e: 'update:height', height: number): void
}>()

const resizing = ref(false)

function startResize(e: MouseEvent) {
  e.preventDefault()
  resizing.value = true
  const startY = e.clientY
  const startH = props.height
  const move = (ev: MouseEvent) => {
    const max = Math.floor(window.innerHeight * props.maxFraction)
    emit('update:height', Math.min(max, Math.max(props.minHeight, startH + (startY - ev.clientY))))
  }
  const up = () => {
    resizing.value = false
    document.removeEventListener('mousemove', move)
    document.removeEventListener('mouseup', up)
    document.body.style.cursor = ''
    document.body.style.userSelect = ''
  }
  document.addEventListener('mousemove', move)
  document.addEventListener('mouseup', up)
  document.body.style.cursor = 'row-resize'
  document.body.style.userSelect = 'none'
}
</script>

<template>
  <div class="qdock-frame">
    <div
      v-if="resizable"
      class="qdock-grip"
      :class="{ 'is-active': resizing }"
      @mousedown="startResize"
      @dblclick="emit('update:height', defaultHeight)"
    />
    <div class="qdock" :style="{ height: height + 'px' }">
      <div class="qdock-tabs">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          class="qdock-tab"
          :class="{ 'is-active': tab.id === active }"
          @click="emit('select', tab.id)"
        >
          {{ tab.label }}
          <span
            v-if="tab.badge != null"
            class="qdock-badge"
            :class="{ 'is-error': tab.badgeKind === 'error' }"
            >{{ tab.badge }}</span
          >
        </button>
        <!-- Extra strip content beside the tabs — a hint, a field, whatever. -->
        <slot name="strip" />
        <span class="qdock-spacer" />
        <slot name="actions" />
        <button class="qdock-close" title="Hide panel" @click="emit('close')">
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
            <path d="M2 2l6 6M8 2l-6 6" stroke="currentColor" stroke-width="1.5" />
          </svg>
        </button>
      </div>
      <div class="qdock-body">
        <slot />
      </div>
    </div>
  </div>
</template>

<style scoped>
.qdock-frame {
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  min-height: 0;
  /* Pinned, not inherited: the dock is mounted inside three different module
     contexts (QuantCanvas sets its own family, 15px base and antialiasing on
     its root), and "the same component" must also RENDER the same. This is
     the shell's own base typography (shell.css). */
  font: 400 13px/1.55 var(--qss-font-sans, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif);
  letter-spacing: normal;
  -webkit-font-smoothing: auto;
}

.qdock-grip {
  height: 4px;
  flex-shrink: 0;
  cursor: row-resize;
  background: transparent;
}
.qdock-grip:hover,
.qdock-grip.is-active {
  background: color-mix(in srgb, var(--qss-accent) 40%, transparent);
}

.qdock {
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
  min-height: 0;
  background: var(--qss-bg-raised);
  border-top: 1px solid var(--qss-border);
}

.qdock-tabs {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
  height: 30px;
  padding: 0 6px;
  border-bottom: 1px solid var(--qss-border);
}

.qdock-tab {
  display: flex;
  align-items: center;
  gap: 6px;
  height: 22px;
  padding: 0 10px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text-muted);
  font-size: 11px;
  cursor: pointer;
}
.qdock-tab:hover {
  color: var(--qss-text-secondary);
  background: var(--qss-bg-hover);
}
.qdock-tab.is-active {
  color: var(--qss-text);
  background: var(--qss-bg);
}

.qdock-badge {
  min-width: 15px;
  padding: 0 4px;
  border-radius: 7px;
  background: var(--qss-bg-hover);
  font-size: 9.5px;
  line-height: 15px;
  text-align: center;
}
.qdock-badge.is-error {
  background: color-mix(in srgb, #ef4444 30%, transparent);
  color: var(--qss-text);
}

.qdock-spacer {
  flex: 1;
}

.qdock-close {
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
}
.qdock-close:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

.qdock-body {
  flex: 1;
  min-height: 0;
  position: relative;
  display: flex;
  flex-direction: column;
}
</style>
