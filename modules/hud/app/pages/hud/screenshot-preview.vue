<template>
  <div class="preview-overlay" @click="close">
    <div v-if="loading" class="preview-loading">Loading image...</div>
    <div v-else-if="imageSrc" class="preview-container" @click.stop>
      <div class="preview-header">
        <span class="preview-title">{{ filename }}</span>
        <button class="preview-close" @click="close">✕</button>
      </div>
      <div class="preview-body">
        <img :src="imageSrc" class="preview-img" />
      </div>
    </div>
    <div v-else class="preview-loading">Failed to load image</div>
  </div>
</template>

<script setup lang="ts">
definePageMeta({ layout: 'hud' })

import { invoke } from "@tauri-apps/api/core";

const imageSrc = ref("");
const filename = ref("");
const loading = ref(true);

async function loadPreview() {
  try {
    const path = await invoke<string | null>("plugin:hud|get_screenshot_preview_path");
    if (!path) {
      loading.value = false;
      return;
    }
    // Extract filename from path
    filename.value = path.split("\\").pop() || path.split("/").pop() || path;
    const base64 = await invoke<string>("plugin:hud|read_screenshot_file", { path });
    imageSrc.value = `data:image/png;base64,${base64}`;
  } catch (e) {
    console.warn("Failed to load preview:", e);
  } finally {
    loading.value = false;
  }
}

async function close() {
  await invoke("plugin:hud|close_screenshot_preview");
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.preventDefault();
    close();
  }
}

onMounted(() => {
  window.addEventListener("keydown", handleKeydown);
  loadPreview();
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
.preview-overlay {
  position: fixed;
  top: 0;
  left: 0;
  width: 100vw;
  height: 100vh;
  background: rgba(0, 0, 0, 0.92);
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}
.preview-loading {
  color: #aaa;
  font-size: 14px;
  font-family: sans-serif;
}
.preview-container {
  display: flex;
  flex-direction: column;
  max-width: 95vw;
  max-height: 95vh;
  cursor: default;
}
.preview-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 14px;
  background: rgba(30, 30, 30, 0.95);
  border-radius: 8px 8px 0 0;
  flex-shrink: 0;
}
.preview-title {
  font-size: 13px;
  color: #ddd;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: sans-serif;
}
.preview-close {
  background: none;
  border: none;
  color: #999;
  cursor: pointer;
  font-size: 18px;
  padding: 0 4px;
  margin-left: 12px;
}
.preview-close:hover {
  color: #fff;
}
.preview-body {
  overflow: auto;
  background: rgba(20, 20, 20, 0.95);
  border-radius: 0 0 8px 8px;
}
.preview-img {
  display: block;
  max-width: 95vw;
  max-height: calc(95vh - 44px);
  object-fit: contain;
}
</style>

