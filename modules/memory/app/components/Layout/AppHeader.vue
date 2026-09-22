<script setup lang="ts">
/**
 * Header center: Vault | Graph navigation and the "new memory" popover.
 * The chrome itself (logo cap, selector, settings) is QModuleHeader's.
 */
import { useVaultStore } from '#memory/stores/vault'
import { useAppStore } from '#memory/stores/app'

const vault = useVaultStore()
const app = useAppStore()
const router = useRouter()
const route = useRoute()

const tab = computed(() => (route.path.startsWith('/memory/graph') ? 'graph' : 'vault'))
</script>

<template>
  <QModuleHeader module-id="memory">
    <div class="qm-header-center">
      <nav class="qm-nav">
        <button class="qm-nav-btn" :class="{ 'is-active': tab === 'vault' }" @click="router.push('/memory')">
          Vault
        </button>
        <button class="qm-nav-btn" :class="{ 'is-active': tab === 'graph' }" @click="router.push('/memory/graph')">
          Graph
        </button>
      </nav>

      <div class="qm-header-spacer" />

      <button class="qm-new-btn" title="New memory" @click="app.startCreate(vault.createScope)">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true">
          <path d="M12 5v14M5 12h14" />
        </svg>
        New memory
      </button>
    </div>
  </QModuleHeader>
</template>

<style scoped>
.qm-header-center {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 14px;
}

.qm-nav {
  display: flex;
  gap: 4px;
}

.qm-nav-btn {
  padding: 5px 12px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qm-text-secondary);
  cursor: pointer;
}
.qm-nav-btn:hover {
  background: var(--qm-bg-hover);
  color: var(--qm-text);
}
.qm-nav-btn.is-active {
  background: var(--qm-bg-card);
  color: var(--qm-text);
}

.qm-header-spacer {
  flex: 1;
}

.qm-new-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 5px 12px;
  border: 1px solid var(--qm-border);
  border-radius: 6px;
  background: var(--qm-bg-card);
  color: var(--qm-text);
  cursor: pointer;
}
.qm-new-btn:hover {
  background: var(--qm-bg-hover);
}

</style>
