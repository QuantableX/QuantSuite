<script setup lang="ts">
/**
 * The scope selector — visually the QuantSpace explorer's workspace dropdown
 * (`ws-trigger`/`ws-menu` in canvas FileExplorer.vue), but module-local:
 * choosing here changes what the memory module shows and where new memories
 * land, and NEVER touches `core / workspace.active`.
 *
 * "General" is the gigabrain: the shared vault plus every workspace's
 * memories in one view.
 */
import { useVaultStore, GENERAL_SCOPE } from '#memory/stores/vault'

const vault = useVaultStore()
const route = useRoute()
const router = useRouter()

const open = ref(false)
const root = ref<HTMLElement | null>(null)

const current = computed(() =>
  vault.scope === GENERAL_SCOPE
    ? { name: 'All memories', hint: '' }
    : {
        name: vault.scopeName(vault.scope),
        hint: parentPathOf(vault.scopes.find((s) => s.scope === vault.scope)?.path ?? ''),
      }
)

function parentPathOf(path: string | null): string {
  if (!path) return ''
  const parts = path.replace(/\\/g, '/').split('/').filter(Boolean)
  if (parts.length <= 1) return ''
  return parts.slice(0, -1).join('/') + ' /'
}

function pick(scope: string) {
  if (route.query.scope || route.path === '/memory/graph') void router.replace({ query: { ...route.query, scope, focus: undefined } })
  vault.setScope(scope)
  open.value = false
}

function onOutside(event: PointerEvent) {
  if (open.value && root.value && !root.value.contains(event.target as Node)) open.value = false
}
onMounted(() => document.addEventListener('pointerdown', onOutside))
onBeforeUnmount(() => document.removeEventListener('pointerdown', onOutside))
</script>

<template>
  <div ref="root" class="qm-scope">
    <button class="qm-scope-trigger" aria-label="Memory workspace" :aria-expanded="open" @click="open = !open" @keydown.esc="open = false">
      <span class="qm-scope-text">
        <span class="qm-scope-hint">{{ current.hint }}</span>
        <span class="qm-scope-name">{{ current.name }}</span>
      </span>
      <svg class="qm-scope-caret" :class="{ 'is-open': open }" width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M3 4l2 2.5L7 4" />
      </svg>
    </button>

    <Transition name="qm-scope-drop">
      <div v-if="open" class="qm-scope-menu">
        <div class="qm-scope-label">Memory scopes</div>
        <div
          v-for="s in vault.scopes"
          :key="s.scope"
          class="qm-scope-item"
          :class="{ 'is-current': s.scope === vault.scope }"
          :title="s.path ?? 'General knowledge + every workspace, in one view'"
          role="button"
          tabindex="0"
          @click="pick(s.scope)"
          @keydown.enter="pick(s.scope)"
        >
          <span class="qm-scope-item-name">{{ s.scope === 'general' ? 'All memories' : s.name }}</span>
          <span class="qm-scope-item-path">
            {{ s.scope === 'general' ? 'General + workspaces' : parentPathOf(s.path) }}
          </span>
          <span class="qm-scope-item-count">{{ s.scope === 'general' ? vault.memories.length : s.memoryCount }}</span>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
/* The QuantSpace explorer's ws-selector, on --qm tokens. */
.qm-scope {
  position: relative;
  flex: 1;
  min-width: 0;
}

.qm-scope-trigger {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  min-height: 28px;
  padding: 5px 8px;
  gap: 4px;
  border: 1px solid var(--qm-border);
  border-radius: 6px;
  background: transparent;
  color: var(--qm-text);
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  transition: border-color 0.15s, background-color 0.15s;
}
.qm-scope-trigger:hover {
  border-color: color-mix(in srgb, var(--qm-text) 25%, transparent);
  background: color-mix(in srgb, var(--qm-text) 4%, transparent);
}

.qm-scope-text {
  display: flex;
  align-items: baseline;
  gap: 2px;
  min-width: 0;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}

.qm-scope-hint {
  color: var(--qm-text-muted);
  opacity: 0.4;
  font-size: 10px;
  flex-shrink: 1;
  overflow: hidden;
  text-overflow: ellipsis;
}

.qm-scope-name {
  flex-shrink: 0;
  font-weight: 600;
}

.qm-scope-caret {
  flex-shrink: 0;
  color: var(--qm-text-muted);
  opacity: 0.5;
  transition: transform 0.15s;
}
.qm-scope-caret.is-open {
  transform: rotate(180deg);
}

.qm-scope-menu {
  position: absolute;
  left: 0;
  right: 0;
  top: calc(100% + 4px);
  z-index: 30;
  padding: 6px;
  border: 1px solid var(--qm-border);
  border-radius: 8px;
  background: var(--qm-bg-raised);
  box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
  max-height: 320px;
  overflow-y: auto;
}

.qm-scope-label {
  padding: 4px 8px;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--qm-text-muted);
}

.qm-scope-item {
  display: grid;
  grid-template-columns: 1fr auto;
  grid-template-areas: 'name count' 'path count';
  column-gap: 8px;
  padding: 5px 8px;
  border-radius: 5px;
  font-size: 11px;
  cursor: pointer;
}
.qm-scope-item:hover {
  background: var(--qm-bg-hover);
}
.qm-scope-item.is-current {
  background: var(--qm-bg-card);
}

.qm-scope-item-name {
  grid-area: name;
  font-weight: 500;
  display: flex;
  align-items: center;
  gap: 6px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qm-scope-item-path {
  grid-area: path;
  font-size: 10px;
  color: var(--qm-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qm-scope-item-count {
  grid-area: count;
  align-self: center;
  font-size: 11px;
  color: var(--qm-text-muted);
}

.qm-scope-drop-enter-active,
.qm-scope-drop-leave-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.qm-scope-drop-enter-from,
.qm-scope-drop-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
