<template>
  <div class="cp-module">
    <div class="hud-top-group">
    <div class="cp-preview" :style="{ background: currentColor }"></div>
    <div class="cp-input-row">
      <input v-model="currentColor" type="color" class="cp-color-input" />
      <input v-model="currentColor" class="input sm" style="flex: 1" />
      <button class="btn btn-green btn-sm" @click="saveColor()">Save</button>
    </div>
    <button
      class="btn btn-primary btn-sm cp-pick-btn"
      @click="startPick"
      :disabled="picking"
    >
      <svg
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        style="vertical-align: middle; margin-right: 3px"
      >
        <path d="M2 22l1-1h3l9-9" />
        <path d="M3 21l9-9" />
        <path d="M15 6l3-3a2.12 2.12 0 0 1 3 3l-3 3" />
        <path d="M12 9l3 3" />
      </svg>
      Pick from Screen
    </button>
    <div class="cp-info">
      <div class="cp-info-row">
        <button class="ctrl-btn" @click="copy(currentColor)">
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path
              d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"
            />
            <rect x="8" y="2" width="8" height="4" rx="1" ry="1" />
          </svg>
        </button>
        <span class="cp-lbl">HEX</span>
        <span class="cp-val">{{ currentColor }}</span>
      </div>
      <div class="cp-info-row">
        <button class="ctrl-btn" @click="copy(rgbStr)">
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path
              d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"
            />
            <rect x="8" y="2" width="8" height="4" rx="1" ry="1" />
          </svg>
        </button>
        <span class="cp-lbl">RGB</span>
        <span class="cp-val">{{ rgbStr }}</span>
      </div>
      <div class="cp-info-row">
        <button class="ctrl-btn" @click="copy(hslStr)">
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path
              d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"
            />
            <rect x="8" y="2" width="8" height="4" rx="1" ry="1" />
          </svg>
        </button>
        <span class="cp-lbl">HSL</span>
        <span class="cp-val">{{ hslStr }}</span>
      </div>
    </div>
    </div>
    <div v-if="savedColors.length" class="cp-saved">
      <h4 class="cp-saved-title">Saved Colors</h4>
      <div class="cp-palette">
        <div v-for="c in savedColors" :key="c.id" class="cp-swatch-wrap">
          <button
            class="cp-swatch"
            :style="{ background: c.hex }"
            @click="currentColor = c.hex"
            :title="c.hex"
          ></button>
          <button class="cp-swatch-del" @click="removeColor(c.id)">✕</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useColorPicker } from '#hud/composables/useColorPicker'
const {
  currentColor,
  savedColors,
  saveColor,
  removeColor,
  hexToRgb,
  hexToHsl,
  pickScreenColor,
} = useColorPicker();
const rgbStr = computed(() => {
  const r = hexToRgb(currentColor.value);
  return r ? `rgb(${r.r}, ${r.g}, ${r.b})` : "";
});
const hslStr = computed(() => {
  const h = hexToHsl(currentColor.value);
  return h ? `hsl(${h.h}, ${h.s}%, ${h.l}%)` : "";
});
const picking = ref(false);
async function startPick() {
  picking.value = true;
  await pickScreenColor();
  picking.value = false;
}
async function copy(text: string) {
  try {
    const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
    await writeText(text);
  } catch {
    navigator.clipboard?.writeText(text);
  }
}
</script>

<style scoped>
.cp-module {
  padding: 4px 0;
}
.cp-preview {
  height: 48px;
  border-radius: 8px;
  border: 1px solid var(--border-color);
  margin-bottom: 8px;
}
.cp-input-row {
  display: flex;
  gap: 6px;
  align-items: center;
  margin-bottom: 6px;
}
.cp-pick-btn {
  width: 100%;
  margin-bottom: 10px;
}
.cp-color-input {
  width: 36px;
  height: 30px;
  border: none;
  padding: 0;
  cursor: pointer;
  background: none;
}
.cp-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-bottom: 12px;
}
.cp-info-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
}
.cp-lbl {
  font-size: 10px;
  font-weight: 600;
  color: var(--text-secondary);
  width: 28px;
}
.cp-val {
  flex: 1;
  font-size: 12px;
  font-family: monospace;
  color: var(--text-primary);
}
.cp-saved {
  border-top: 1px solid var(--border-color);
  padding-top: 8px;
}
.cp-saved-title {
  font-size: 12px;
  color: var(--text-secondary);
  margin-bottom: 6px;
}
.cp-palette {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.cp-swatch-wrap {
  position: relative;
}
.cp-swatch {
  width: 28px;
  height: 28px;
  border-radius: 4px;
  border: 1px solid var(--border-color);
  cursor: pointer;
}
.cp-swatch-del {
  position: absolute;
  top: -4px;
  right: -4px;
  width: 14px;
  height: 14px;
  font-size: 8px;
  border-radius: 50%;
  background: var(--accent-red);
  color: #fff;
  border: none;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  opacity: 0;
  transition: opacity 0.15s;
}
.cp-swatch-wrap:hover .cp-swatch-del {
  opacity: 1;
}
.input.sm {
  font-size: 12px;
  padding: 4px 8px;
}
.btn-sm {
  font-size: 12px;
  padding: 4px 10px;
}
.ctrl-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 12px;
  padding: 2px;
  border-radius: 3px;
}
.ctrl-btn:hover {
  color: var(--text-primary);
}
</style>
