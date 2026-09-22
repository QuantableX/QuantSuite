<script setup lang="ts">
/**
 * QuantMCP → General — entry in the unified settings modal (V3). The MCP
 * host's configuration (servers, clients, tool approval) lives on the
 * module's Settings page; this section routes there.
 */
function openFullSettings() {
  window.dispatchEvent(new CustomEvent('qss:navigate', { detail: { route: '/mcp/settings' } }))
  window.dispatchEvent(new CustomEvent('qss:settings-close'))
}
</script>

<template>
  <div>
    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">MCP host settings</p>
        <p class="qsu-hint">Servers, AI client connections, tool approval mode.</p>
      </div>
      <button class="qsu-btn qsu-btn--primary" @click="openFullSettings">Open settings</button>
    </div>
  </div>
</template>
