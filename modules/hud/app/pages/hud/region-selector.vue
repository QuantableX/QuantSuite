<template>
  <div
    class="overlay"
    @mousedown="startSelection"
    @mousemove="updateSelection"
    @mouseup="endSelection"
  >
    <div class="instructions">
      Click and drag to select region • ESC to cancel
    </div>
    <div v-if="selection" class="selection-box" :style="selectionStyle"></div>
  </div>
</template>

<script setup lang="ts">
definePageMeta({ layout: 'hud' })

import { invoke } from "@tauri-apps/api/core";

const selection = ref<{
  startX: number;
  startY: number;
  endX: number;
  endY: number;
} | null>(null);
const isDragging = ref(false);

const selectionStyle = computed(() => {
  if (!selection.value) return {};
  const { startX, startY, endX, endY } = selection.value;
  return {
    left: `${Math.min(startX, endX)}px`,
    top: `${Math.min(startY, endY)}px`,
    width: `${Math.abs(endX - startX)}px`,
    height: `${Math.abs(endY - startY)}px`,
  };
});

function startSelection(e: MouseEvent) {
  isDragging.value = true;
  selection.value = {
    startX: e.clientX,
    startY: e.clientY,
    endX: e.clientX,
    endY: e.clientY,
  };
}

function updateSelection(e: MouseEvent) {
  if (!isDragging.value || !selection.value) return;
  selection.value.endX = e.clientX;
  selection.value.endY = e.clientY;
}

async function endSelection() {
  if (!isDragging.value || !selection.value) return;
  isDragging.value = false;

  const { startX, startY, endX, endY } = selection.value;
  const x = Math.min(startX, endX);
  const y = Math.min(startY, endY);
  const width = Math.abs(endX - startX);
  const height = Math.abs(endY - startY);

  if (width > 10 && height > 10) {
    // Scale coordinates for high-DPI displays
    const scale = window.devicePixelRatio || 1;
    const scaledRegion = [
      Math.round(x * scale),
      Math.round(y * scale),
      Math.round(width * scale),
      Math.round(height * scale),
    ];
    await invoke("plugin:hud|set_selected_region", { region: scaledRegion });
  } else {
    await invoke("plugin:hud|set_selected_region", { region: null });
  }
}

async function cancel() {
  await invoke("plugin:hud|set_selected_region", { region: null });
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.preventDefault();
    cancel();
  }
}

onMounted(() => {
  window.addEventListener("keydown", handleKeydown);
});

onUnmounted(() => {
  window.removeEventListener("keydown", handleKeydown);
});
</script>

<style>
html,
body,
#__nuxt {
  background: rgba(0, 0, 0, 0.15) !important;
  margin: 0;
  padding: 0;
  overflow: hidden;
  width: 100vw;
  height: 100vh;
}
</style>

<style scoped>
.overlay {
  position: fixed;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  background: transparent;
  cursor: crosshair;
}

.instructions {
  position: absolute;
  top: 20px;
  left: 50%;
  transform: translateX(-50%);
  background: rgba(0, 0, 0, 0.7);
  color: white;
  padding: 8px 16px;
  border-radius: 6px;
  font-size: 13px;
  pointer-events: none;
  user-select: none;
}

.selection-box {
  position: absolute;
  border: 2px solid #00ff88;
  background: rgba(0, 255, 136, 0.1);
  pointer-events: none;
}
</style>
