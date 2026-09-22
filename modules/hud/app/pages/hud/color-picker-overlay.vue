<template>
  <div class="overlay" @click="pickAtCursor">
    <div class="instructions" :style="instructionsStyle">
      Click anywhere to pick the color at that point • ESC to cancel
    </div>
  </div>
</template>

<script setup lang="ts">
definePageMeta({ layout: 'hud' })

import { invoke } from "@tauri-apps/api/core";

// Read primary-monitor offset from query params so we can center the
// instructions banner on the primary screen instead of the full virtual desktop.
const route = useRoute();
const scale = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;
const pmx = Number(route.query.pmx || 0) / scale; // physical → CSS px
const pmw = Number(route.query.pmw || 0) / scale;

const instructionsStyle = computed(() => {
  if (!pmw) return {}; // fallback: CSS default centering
  return {
    left: `${pmx + pmw / 2}px`,
  };
});

async function pickAtCursor() {
  try {
    // Rust hides the overlay, waits for compositor, then reads the pixel
    const hex = await invoke<string>("plugin:hud|pick_screen_color");
    await invoke("plugin:hud|set_picked_color", { color: hex });
  } catch (err) {
    console.warn("Failed to pick color:", err);
    await invoke("plugin:hud|set_picked_color", { color: null });
  }
}

async function cancel() {
  await invoke("plugin:hud|set_picked_color", { color: null });
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
  background: transparent !important;
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
</style>
