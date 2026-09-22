<script setup lang="ts">
/**
 * QuantCode → Editor — the module's section in the unified settings modal (V3).
 *
 * Word wrap and the minimap sat in the tab strip's context menu for a day
 * (they had been in a status bar before that); they are settings, so they
 * belong here (user, 2026-08-26). Controls use the shared `.qsu-*` form
 * language from shell.css.
 */
import { useEditorStore } from '#code-root/stores/editor'

const store = useEditorStore()
</script>

<template>
  <div>
    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Word wrap</p>
        <p class="qsu-hint">Break long lines at the edge of the editor instead of scrolling sideways.</p>
      </div>
      <button
        class="qsu-toggle"
        :class="{ 'is-on': store.wordWrap }"
        role="switch"
        :aria-checked="store.wordWrap"
        @click="store.wordWrap = !store.wordWrap"
      />
    </div>

    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Minimap</p>
        <p class="qsu-hint">The condensed overview along the right edge of the editor.</p>
      </div>
      <button
        class="qsu-toggle"
        :class="{ 'is-on': store.minimap }"
        role="switch"
        :aria-checked="store.minimap"
        @click="store.minimap = !store.minimap"
      />
    </div>
  </div>
</template>
