<template>
  <div ref="moduleRef" class="todo-module">
    <!-- Add Section Button -->
    <button class="btn btn-primary add-section-btn" @click="addSection()">
      + New Section
    </button>

    <!-- Sections -->
    <div
      v-for="(section, sIdx) in sections"
      :key="section.id"
      class="section"
      :class="{
        'section-drop-above':
          dropTargetSectionIdx === sIdx &&
          dragType === 'section' &&
          sectionDropPosition === 'above',
        'section-drop-below':
          dropTargetSectionIdx === sIdx &&
          dragType === 'section' &&
          sectionDropPosition === 'below',
        dragging: draggingSectionIdx === sIdx,
      }"
    >
      <!-- Section Header -->
      <div class="section-header">
        <span
          class="drag-handle"
          draggable="true"
          @dragstart="onSectionDragStart(sIdx, $event)"
          @dragend="onDragEnd"
          title="Drag to reorder"
          >⠿</span
        >

        <button
          class="collapse-btn"
          @click="toggleSection(section.id)"
          :title="section.collapsed ? 'Expand' : 'Collapse'"
        >
          <span class="collapse-icon" :class="{ collapsed: section.collapsed }"
            >▼</span
          >
        </button>

        <!-- Section Name (inline rename) -->
        <input
          v-if="editingSectionId === section.id"
          ref="sectionInputRef"
          class="section-name-input"
          :value="section.name"
          @blur="finishRenameSection(section.id, $event)"
          @keydown.enter="($event.target as HTMLInputElement).blur()"
          @keydown.escape="editingSectionId = null"
        />
        <span
          v-else
          class="section-name"
          @dblclick="startRenameSection(section.id)"
        >
          {{ section.name }}
          <span class="section-progress">{{
            getSectionProgress(section)
          }}</span>
        </span>

        <!-- Section Controls -->
        <div class="section-controls">
          <button
            class="ctrl-btn ctrl-add"
            @click="addTask(section.id)"
            title="Add task"
          >
            +
          </button>
          <button
            class="ctrl-btn"
            @click="duplicateSection(section.id)"
            title="Duplicate section"
          >
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            >
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
              <path
                d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"
              />
            </svg>
          </button>
          <button
            class="ctrl-btn ctrl-del"
            @click="confirmDeleteSection(section.id)"
            title="Delete section"
          >
            ✕
          </button>
        </div>
      </div>

      <!-- Tasks List (collapsible, drop zone) -->
      <div
        v-if="!section.collapsed"
        class="tasks-list"
        :class="{
          'task-drop-zone': dragType === 'task' && section.tasks.length === 0,
        }"
        @dragover.prevent.stop="
          onTaskListDragOver(section.id, section.tasks.length, $event)
        "
        @drop.prevent.stop="
          onTaskDrop(section.id, dropTargetTaskIdx ?? section.tasks.length)
        "
      >
        <div
          v-for="(task, tIdx) in section.tasks"
          :key="task.id"
          class="task-item"
          :class="{
            'task-done': task.done,
            'task-drop-above':
              dropTargetTaskIdx === tIdx &&
              dropTargetSectionId === section.id &&
              dragType === 'task' &&
              taskDropPosition === 'above',
            'task-drop-below':
              dropTargetTaskIdx === tIdx &&
              dropTargetSectionId === section.id &&
              dragType === 'task' &&
              taskDropPosition === 'below',
            'task-dragging':
              draggingTask?.taskIdx === tIdx &&
              draggingTask?.sectionId === section.id,
          }"
          @dragover.prevent.stop="onTaskDragOver(section.id, tIdx, $event)"
          @drop.prevent.stop="onTaskDrop(section.id, tIdx)"
        >
          <!-- Task Header -->
          <div class="task-header">
            <span
              class="drag-handle task-handle"
              draggable="true"
              @dragstart.stop="onTaskDragStart(section.id, tIdx, $event)"
              @dragend="onDragEnd"
              title="Drag to reorder"
              >⠿</span
            >
            <input
              type="checkbox"
              class="task-checkbox"
              :checked="task.done"
              @change="toggleTask(section.id, task.id)"
            />
            <input
              v-if="editingTaskId === task.id"
              ref="taskInputRef"
              class="task-title-input"
              :value="task.title"
              @blur="finishRenameTask(section.id, task.id, $event)"
              @keydown.enter="($event.target as HTMLInputElement).blur()"
              @keydown.escape="editingTaskId = null"
            />
            <span
              v-else
              class="task-title"
              @dblclick="startRenameTask(task.id)"
            >
              {{ task.title }}
            </span>

            <div class="task-controls">
              <span v-if="task.subtasks.length > 0" class="subtask-count">
                {{ task.subtasks.filter((st) => st.done).length }}/{{
                  task.subtasks.length
                }}
              </span>
              <button
                class="ctrl-btn ctrl-expand"
                @click="toggleTaskExpand(section.id, task.id)"
                :title="task.expanded ? 'Collapse subtasks' : 'Expand subtasks'"
              >
                <span
                  class="collapse-icon"
                  :class="{ collapsed: !task.expanded }"
                  >▼</span
                >
              </button>
              <button
                class="ctrl-btn ctrl-add"
                @click="addSubtask(section.id, task.id)"
                title="Add subtask"
              >
                +
              </button>
              <button
                class="ctrl-btn"
                @click="duplicateTask(section.id, task.id)"
                title="Duplicate task (without subtasks)"
              >
                <svg
                  width="11"
                  height="11"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                >
                  <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                  <path
                    d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"
                  />
                </svg>
              </button>
              <button
                class="ctrl-btn ctrl-del"
                @click="deleteTask(section.id, task.id)"
                title="Delete task"
              >
                ✕
              </button>
            </div>
          </div>

          <!-- Subtasks -->
          <div
            v-if="task.expanded"
            class="subtasks-list"
            @dragover.prevent.stop="
              onSubtaskListDragOver(
                section.id,
                task.id,
                task.subtasks.length,
                $event,
              )
            "
            @drop.prevent.stop="
              onSubtaskDrop(
                section.id,
                task.id,
                dropTargetSubtaskIdx ?? task.subtasks.length,
              )
            "
          >
            <div
              v-for="(sub, stIdx) in task.subtasks"
              :key="sub.id"
              class="subtask-item"
              :class="{
                'subtask-done': sub.done,
                'subtask-drop-above':
                  dropTargetSubtaskIdx === stIdx &&
                  dropTargetTaskId === task.id &&
                  dragType === 'subtask' &&
                  subtaskDropPosition === 'above',
                'subtask-drop-below':
                  dropTargetSubtaskIdx === stIdx &&
                  dropTargetTaskId === task.id &&
                  dragType === 'subtask' &&
                  subtaskDropPosition === 'below',
                'subtask-dragging':
                  draggingSubtask?.subtaskIdx === stIdx &&
                  draggingSubtask?.taskId === task.id,
              }"
              @dragover.prevent.stop="
                onSubtaskDragOver(section.id, task.id, stIdx, $event)
              "
              @drop.prevent.stop="onSubtaskDrop(section.id, task.id, stIdx)"
            >
              <span
                class="drag-handle subtask-handle"
                draggable="true"
                @dragstart.stop="
                  onSubtaskDragStart(section.id, task.id, stIdx, $event)
                "
                @dragend="onDragEnd"
                title="Drag to reorder"
                >⠿</span
              >
              <input
                type="checkbox"
                class="subtask-checkbox"
                :checked="sub.done"
                @change="toggleSubtask(section.id, task.id, sub.id)"
              />
              <input
                v-if="editingSubtaskId === sub.id"
                ref="subtaskInputRef"
                class="subtask-title-input"
                :value="sub.title"
                @blur="finishRenameSubtask(section.id, task.id, sub.id, $event)"
                @keydown.enter="($event.target as HTMLInputElement).blur()"
                @keydown.escape="editingSubtaskId = null"
              />
              <span
                v-else
                class="subtask-title"
                @dblclick="startRenameSubtask(sub.id)"
              >
                {{ sub.title }}
              </span>
              <button
                class="ctrl-btn ctrl-del subtask-del"
                @click="deleteSubtask(section.id, task.id, sub.id)"
                title="Delete subtask"
              >
                ✕
              </button>
            </div>
            <p v-if="task.subtasks.length === 0" class="empty-hint">
              {{
                dragType === "subtask" ? "Drop subtask here" : "No subtasks yet"
              }}
            </p>
          </div>
        </div>

        <p v-if="section.tasks.length === 0" class="empty-hint">
          {{ dragType === "task" ? "Drop task here" : "No tasks yet" }}
        </p>
      </div>
    </div>

    <p v-if="sections.length === 0" class="empty-hint" style="margin-top: 12px">
      Create a section to start tracking tasks
    </p>
  </div>
</template>

<script setup lang="ts">
import { useTodos } from '#hud/composables/useTodos'
import type { TodoSection } from "#hud/composables/useTodos";

const {
  sections,
  addSection,
  renameSection,
  deleteSection,
  toggleSection,
  addTask,
  renameTask,
  toggleTask,
  deleteTask,
  addSubtask,
  renameSubtask,
  toggleSubtask,
  deleteSubtask,
  reorderSection,
  reorderTask,
  moveTaskToSectionAt,
  reorderSubtask,
  moveSubtaskToTask,
  toggleTaskExpand,
  duplicateSection,
  duplicateTask,
} = useTodos();

const editingSectionId = ref<string | null>(null);
const editingTaskId = ref<string | null>(null);
const editingSubtaskId = ref<string | null>(null);
const sectionInputRef = ref<HTMLInputElement[] | null>(null);
const taskInputRef = ref<HTMLInputElement[] | null>(null);
const subtaskInputRef = ref<HTMLInputElement[] | null>(null);
const moduleRef = ref<HTMLElement | null>(null);

// --- Drag state ---
const dragType = ref<"section" | "task" | "subtask" | null>(null);
const draggingSectionIdx = ref<number | null>(null);
const draggingTask = ref<{ sectionId: string; taskIdx: number } | null>(null);
const draggingSubtask = ref<{
  sectionId: string;
  taskId: string;
  subtaskIdx: number;
} | null>(null);
const dropTargetSectionId = ref<string | null>(null);
const dropTargetTaskIdx = ref<number | null>(null);
const taskDropPosition = ref<"above" | "below" | null>(null);
const dropTargetSubtaskIdx = ref<number | null>(null);
const subtaskDropPosition = ref<"above" | "below" | null>(null);
const dropTargetTaskId = ref<string | null>(null);
const dropTargetSubtaskSectionId = ref<string | null>(null);
const sectionDropPosition = ref<"above" | "below" | null>(null);
const dropTargetSectionIdx = ref<number | null>(null);

onUnmounted(() => {
  document.removeEventListener("dragover", onDocSectionDragOver);
  document.removeEventListener("drop", onDocSectionDrop);
});

function getSectionProgress(section: TodoSection): string {
  if (section.tasks.length === 0) return "";
  const done = section.tasks.filter((t) => t.done).length;
  return `(${done}/${section.tasks.length})`;
}

// --- Section drag handlers ---
function onSectionDragStart(idx: number, e: DragEvent) {
  dragType.value = "section";
  draggingSectionIdx.value = idx;
  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", `section:${idx}`);
  }
  document.addEventListener("dragover", onDocSectionDragOver);
  document.addEventListener("drop", onDocSectionDrop);
}

function onDocSectionDragOver(e: DragEvent) {
  e.preventDefault();
  if (dragType.value !== "section" || !moduleRef.value) return;
  if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  const sectionEls = Array.from(
    moduleRef.value.querySelectorAll(":scope > .section"),
  );
  if (sectionEls.length === 0) return;
  const cursorY = e.clientY;
  const slotYs: number[] = [];
  slotYs.push(sectionEls[0].getBoundingClientRect().top);
  for (let i = 1; i < sectionEls.length; i++) {
    const prevBottom = sectionEls[i - 1].getBoundingClientRect().bottom;
    const currTop = sectionEls[i].getBoundingClientRect().top;
    slotYs.push((prevBottom + currTop) / 2);
  }
  slotYs.push(sectionEls[sectionEls.length - 1].getBoundingClientRect().bottom);
  if (cursorY <= slotYs[0]) {
    dropTargetSectionIdx.value = 0;
    sectionDropPosition.value = "above";
    return;
  }
  if (cursorY >= slotYs[slotYs.length - 1]) {
    dropTargetSectionIdx.value = sectionEls.length - 1;
    sectionDropPosition.value = "below";
    return;
  }
  let bestSlot = 0,
    bestDist = Infinity;
  for (let i = 0; i <= sectionEls.length; i++) {
    const dist = Math.abs(cursorY - slotYs[i]);
    if (dist < bestDist) {
      bestDist = dist;
      bestSlot = i;
    }
  }
  if (bestSlot < sectionEls.length) {
    dropTargetSectionIdx.value = bestSlot;
    sectionDropPosition.value = "above";
  } else {
    dropTargetSectionIdx.value = sectionEls.length - 1;
    sectionDropPosition.value = "below";
  }
}

function onDocSectionDrop(e: DragEvent) {
  e.preventDefault();
  if (
    dragType.value !== "section" ||
    draggingSectionIdx.value === null ||
    dropTargetSectionIdx.value === null ||
    sectionDropPosition.value === null
  ) {
    resetDragState();
    return;
  }
  let finalIdx =
    sectionDropPosition.value === "below"
      ? dropTargetSectionIdx.value + 1
      : dropTargetSectionIdx.value;
  const fromIdx = draggingSectionIdx.value;
  if (fromIdx < finalIdx) finalIdx--;
  reorderSection(fromIdx, finalIdx);
  resetDragState();
}

// --- Task drag handlers ---
function onTaskDragStart(sectionId: string, taskIdx: number, e: DragEvent) {
  dragType.value = "task";
  draggingTask.value = { sectionId, taskIdx };
  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", `task:${sectionId}:${taskIdx}`);
  }
}

function onTaskListDragOver(
  sectionId: string,
  taskCount: number,
  e: DragEvent,
) {
  if (dragType.value !== "task") return;
  if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  dropTargetSectionId.value = sectionId;
  if (taskCount > 0) {
    dropTargetTaskIdx.value = taskCount - 1;
    taskDropPosition.value = "below";
  } else {
    dropTargetTaskIdx.value = 0;
    taskDropPosition.value = "above";
  }
}

function onTaskDragOver(sectionId: string, taskIdx: number, e: DragEvent) {
  if (dragType.value !== "task") return;
  if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  const midY = rect.top + rect.height / 2;
  dropTargetSectionId.value = sectionId;
  dropTargetTaskIdx.value = taskIdx;
  taskDropPosition.value = e.clientY < midY ? "above" : "below";
}

function onTaskDrop(toSectionId: string, toIdx: number) {
  if (dragType.value !== "task" || !draggingTask.value) {
    resetDragState();
    return;
  }
  const { sectionId: fromSectionId, taskIdx: fromIdx } = draggingTask.value;
  let finalIdx = taskDropPosition.value === "below" ? toIdx + 1 : toIdx;
  if (fromSectionId === toSectionId) {
    if (fromIdx < finalIdx) finalIdx--;
    reorderTask(fromSectionId, fromIdx, finalIdx);
  } else {
    moveTaskToSectionAt(fromSectionId, fromIdx, toSectionId, finalIdx);
  }
  resetDragState();
}

// --- Subtask drag handlers ---
function onSubtaskDragStart(
  sectionId: string,
  taskId: string,
  subtaskIdx: number,
  e: DragEvent,
) {
  dragType.value = "subtask";
  draggingSubtask.value = { sectionId, taskId, subtaskIdx };
  if (e.dataTransfer) {
    e.dataTransfer.effectAllowed = "move";
    e.dataTransfer.setData("text/plain", `subtask:${taskId}:${subtaskIdx}`);
  }
}

function onSubtaskListDragOver(
  sectionId: string,
  taskId: string,
  subtaskCount: number,
  e: DragEvent,
) {
  if (dragType.value !== "subtask") return;
  if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  dropTargetTaskId.value = taskId;
  dropTargetSubtaskSectionId.value = sectionId;
  if (subtaskCount > 0) {
    dropTargetSubtaskIdx.value = subtaskCount - 1;
    subtaskDropPosition.value = "below";
  } else {
    dropTargetSubtaskIdx.value = 0;
    subtaskDropPosition.value = "above";
  }
}

function onSubtaskDragOver(
  sectionId: string,
  taskId: string,
  subtaskIdx: number,
  e: DragEvent,
) {
  if (dragType.value !== "subtask") return;
  if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
  dropTargetTaskId.value = taskId;
  dropTargetSubtaskSectionId.value = sectionId;
  const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
  const midY = rect.top + rect.height / 2;
  dropTargetSubtaskIdx.value = subtaskIdx;
  subtaskDropPosition.value = e.clientY < midY ? "above" : "below";
}

function onSubtaskDrop(sectionId: string, taskId: string, toIdx: number) {
  if (dragType.value !== "subtask" || !draggingSubtask.value) {
    resetDragState();
    return;
  }
  const fromSectionId = draggingSubtask.value.sectionId;
  const fromTaskId = draggingSubtask.value.taskId;
  const fromIdx = draggingSubtask.value.subtaskIdx;
  let finalIdx = subtaskDropPosition.value === "below" ? toIdx + 1 : toIdx;

  if (fromTaskId === taskId) {
    // Same task — reorder within
    if (fromIdx < finalIdx) finalIdx--;
    reorderSubtask(sectionId, taskId, fromIdx, finalIdx);
  } else {
    // Different task (possibly different section) — move across
    moveSubtaskToTask(
      fromSectionId,
      fromTaskId,
      fromIdx,
      sectionId,
      taskId,
      finalIdx,
    );
  }
  resetDragState();
}

// --- Shared drag utilities ---
function onDragEnd() {
  resetDragState();
}

function resetDragState() {
  dragType.value = null;
  draggingSectionIdx.value = null;
  draggingTask.value = null;
  draggingSubtask.value = null;
  dropTargetSectionId.value = null;
  dropTargetTaskIdx.value = null;
  taskDropPosition.value = null;
  dropTargetSubtaskIdx.value = null;
  subtaskDropPosition.value = null;
  dropTargetTaskId.value = null;
  dropTargetSubtaskSectionId.value = null;
  sectionDropPosition.value = null;
  dropTargetSectionIdx.value = null;
  document.removeEventListener("dragover", onDocSectionDragOver);
  document.removeEventListener("drop", onDocSectionDrop);
}

// --- Rename helpers ---
function startRenameSection(id: string) {
  editingSectionId.value = id;
  nextTick(() => {
    sectionInputRef.value?.[0]?.focus();
    sectionInputRef.value?.[0]?.select();
  });
}

function finishRenameSection(id: string, e: Event) {
  const val = (e.target as HTMLInputElement).value.trim();
  if (val) renameSection(id, val);
  editingSectionId.value = null;
}

function startRenameTask(id: string) {
  editingTaskId.value = id;
  nextTick(() => {
    taskInputRef.value?.[0]?.focus();
    taskInputRef.value?.[0]?.select();
  });
}

function finishRenameTask(sectionId: string, taskId: string, e: Event) {
  const val = (e.target as HTMLInputElement).value.trim();
  if (val) renameTask(sectionId, taskId, val);
  editingTaskId.value = null;
}

function startRenameSubtask(id: string) {
  editingSubtaskId.value = id;
  nextTick(() => {
    subtaskInputRef.value?.[0]?.focus();
    subtaskInputRef.value?.[0]?.select();
  });
}

function finishRenameSubtask(
  sectionId: string,
  taskId: string,
  subtaskId: string,
  e: Event,
) {
  const val = (e.target as HTMLInputElement).value.trim();
  if (val) renameSubtask(sectionId, taskId, subtaskId, val);
  editingSubtaskId.value = null;
}

function confirmDeleteSection(id: string) {
  deleteSection(id);
}
</script>

<style scoped>
.todo-module {
  padding: 4px 0;
}

.add-section-btn {
  width: 100%;
  margin-bottom: 8px;
  font-size: 13px;
  padding: 6px 12px;
}

/* --- Drag handle --- */
.drag-handle {
  cursor: grab;
  color: var(--text-secondary);
  font-size: 14px;
  line-height: 1;
  padding: 2px 2px;
  border-radius: 3px;
  user-select: none;
  flex-shrink: 0;
  opacity: 0.5;
  transition:
    opacity 0.15s ease,
    color 0.15s ease;
}

.drag-handle:hover {
  opacity: 1;
  color: var(--accent-blue);
}

.drag-handle:active {
  cursor: grabbing;
}

/* --- Section --- */
.section {
  position: relative;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  margin-bottom: 8px;
  overflow: visible;
}

.section.dragging {
  opacity: 0.4;
}

.section.section-drop-above::before {
  content: "";
  position: absolute;
  top: -5px;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--accent-blue);
  border-radius: 1px;
  pointer-events: none;
  z-index: 1;
}

.section.section-drop-below::after {
  content: "";
  position: absolute;
  bottom: -5px;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--accent-blue);
  border-radius: 1px;
  pointer-events: none;
  z-index: 1;
}

.section-header {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 8px;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  min-height: 34px;
}

.collapse-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 2px 4px;
  font-size: 10px;
  line-height: 1;
  flex-shrink: 0;
}

.collapse-icon {
  display: inline-block;
  transition: transform 0.2s ease;
}

.collapse-icon.collapsed {
  transform: rotate(-90deg);
}

.section-name {
  flex: 1;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  cursor: default;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.section-progress {
  font-weight: 400;
  font-size: 11px;
  color: var(--text-secondary);
  margin-left: 4px;
}

.section-name-input {
  flex: 1;
  font-size: 13px;
  font-weight: 600;
  background: var(--input-bg);
  border: 1px solid var(--accent-blue);
  border-radius: 4px;
  color: var(--text-primary);
  padding: 2px 6px;
  min-width: 0;
}

.section-name-input:focus {
  outline: none;
}

.section-controls {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}

/* --- Shared control buttons --- */
.ctrl-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 10px;
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 3px;
  transition: all 0.12s ease;
}

.ctrl-btn:hover:not(:disabled) {
  background: var(--btn-primary);
  color: var(--text-primary);
}

.ctrl-add {
  font-size: 14px;
  font-weight: 700;
  color: var(--accent-green-dim);
}

.ctrl-add:hover {
  color: var(--accent-green);
}

.ctrl-del:hover {
  color: var(--accent-red) !important;
  background: rgba(255, 68, 102, 0.15) !important;
}

.ctrl-expand {
  font-size: 8px;
}

/* --- Tasks list --- */
.tasks-list {
  padding: 6px 6px 12px;
  min-height: 20px;
}

.task-drop-zone {
  background: rgba(74, 144, 226, 0.06);
  border: 1px dashed var(--accent-blue);
  border-radius: 6px;
}

.task-item {
  position: relative;
  background: var(--input-bg);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 6px 8px;
  margin-bottom: 6px;
  transition:
    border-color 0.15s ease,
    opacity 0.15s ease;
}

.task-item:last-child {
  margin-bottom: 0;
}

.task-item.task-dragging {
  opacity: 0.35;
}

.task-item.task-drop-above::before {
  content: "";
  position: absolute;
  top: -4px;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--accent-blue);
  border-radius: 1px;
}

.task-item.task-drop-below::after {
  content: "";
  position: absolute;
  bottom: -4px;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--accent-blue);
  border-radius: 1px;
}

.task-handle {
  font-size: 12px;
}

.task-item.task-done {
  opacity: 0.55;
}

.task-header {
  display: flex;
  align-items: center;
  gap: 6px;
}

.task-checkbox {
  width: 16px;
  height: 16px;
  accent-color: var(--accent-blue);
  cursor: pointer;
  flex-shrink: 0;
}

.task-title {
  flex: 1;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  cursor: default;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.task-done .task-title {
  text-decoration: line-through;
  color: var(--text-secondary);
}

.task-title-input {
  flex: 1;
  font-size: 13px;
  font-weight: 600;
  background: var(--bg-card);
  border: 1px solid var(--accent-blue);
  border-radius: 4px;
  color: var(--text-primary);
  padding: 1px 6px;
  min-width: 0;
}

.task-title-input:focus {
  outline: none;
}

.task-controls {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}

.subtask-count {
  font-size: 10px;
  color: var(--text-secondary);
  margin-right: 2px;
}

/* --- Subtasks --- */
.subtasks-list {
  margin-top: 6px;
  padding-left: 22px;
  border-left: 2px solid var(--border-color);
  margin-left: 8px;
}

.subtask-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 3px 4px;
  border-radius: 4px;
  margin-bottom: 2px;
}

.subtask-item:hover {
  background: rgba(255, 255, 255, 0.03);
}

.subtask-item.subtask-dragging {
  opacity: 0.35;
}

.subtask-item.subtask-drop-above::before {
  content: "";
  position: absolute;
  top: -2px;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--accent-blue);
  border-radius: 1px;
}

.subtask-item.subtask-drop-below::after {
  content: "";
  position: absolute;
  bottom: -2px;
  left: 0;
  right: 0;
  height: 2px;
  background: var(--accent-blue);
  border-radius: 1px;
}

.subtask-handle {
  font-size: 10px;
}

.subtask-checkbox {
  width: 14px;
  height: 14px;
  accent-color: var(--accent-blue);
  cursor: pointer;
  flex-shrink: 0;
}

.subtask-title {
  flex: 1;
  font-size: 12px;
  color: var(--text-primary);
  cursor: default;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.subtask-done .subtask-title {
  text-decoration: line-through;
  color: var(--text-secondary);
}

.subtask-title-input {
  flex: 1;
  font-size: 12px;
  background: var(--bg-card);
  border: 1px solid var(--accent-blue);
  border-radius: 4px;
  color: var(--text-primary);
  padding: 1px 6px;
  min-width: 0;
}

.subtask-title-input:focus {
  outline: none;
}

.subtask-del {
  opacity: 0;
  transition: opacity 0.15s ease;
}

.subtask-item:hover .subtask-del {
  opacity: 1;
}

/* --- Empty state --- */
.empty-hint {
  text-align: center;
  font-size: 12px;
  color: var(--text-secondary);
  padding: 8px 0;
  opacity: 0.7;
}
</style>
