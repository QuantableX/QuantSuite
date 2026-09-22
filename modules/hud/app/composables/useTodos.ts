import { todosStore, type SubTask as SharedSubTask, type TodoTask, type TodoSection as SharedTodoSection } from "@quantsuite/core";

/**
 * v2 (PLAN-V2 E3): the **suite's** to-dos. One document in `core.db`
 * (`todos/sections`); this overlay and the shell drawer render it live via
 * the bus. Pre-E3 data (`_todos` in the hud plugin config) is copy-imported
 * once and left in place, never deleted.
 */
export type SubTask = SharedSubTask;
export type Task = TodoTask;
export type TodoSection = SharedTodoSection;

const TODOS_KEY = "quanthud_todos";

function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}

export function useTodos() {
  const sections = ref<TodoSection[]>([]);

  // --- Persistence ---

  async function loadTodos() {
    try {
      const shared = await todosStore.load();
      if (shared) {
        sections.value = shared;
        migrateTasks();
        return;
      }

      // Shared store empty — one-time copy-import of the pre-E3 data. The
      // source keys stay where they are (standing rule: copy, never move).
      let legacy: TodoSection[] | null = null;
      if (typeof window !== "undefined" && (window as any).__TAURI__) {
        const { invoke } = await import("@tauri-apps/api/core");
        const saved = await invoke<string>("plugin:hud|load_config");
        if (saved) legacy = JSON.parse(saved)._todos ?? null;
      } else {
        const saved = localStorage.getItem(TODOS_KEY);
        if (saved) legacy = JSON.parse(saved);
      }
      if (legacy) {
        sections.value = legacy;
        migrateTasks();
        await todosStore.save(JSON.parse(JSON.stringify(sections.value)));
      }
    } catch (e) {
      console.warn("Failed to load todos:", e);
    }
  }

  function migrateTasks() {
    for (const section of sections.value) {
      for (const task of section.tasks) {
        if (task.expanded === undefined) task.expanded = false;
      }
    }
  }

  async function saveTodos() {
    try {
      await todosStore.save(JSON.parse(JSON.stringify(sections.value)));
    } catch (e) {
      console.warn("Failed to save todos:", e);
    }
  }

  // --- Section CRUD ---

  function addSection(name = "New Section") {
    sections.value.push({
      id: generateId(),
      name,
      collapsed: false,
      tasks: [],
    });
    saveTodos();
  }

  function renameSection(sectionId: string, name: string) {
    const s = sections.value.find((s) => s.id === sectionId);
    if (s) {
      s.name = name;
      saveTodos();
    }
  }

  function deleteSection(sectionId: string) {
    sections.value = sections.value.filter((s) => s.id !== sectionId);
    saveTodos();
  }

  function toggleSection(sectionId: string) {
    const s = sections.value.find((s) => s.id === sectionId);
    if (s) {
      s.collapsed = !s.collapsed;
      saveTodos();
    }
  }

  // --- Task CRUD ---

  function addTask(sectionId: string, title = "New Task") {
    const s = sections.value.find((s) => s.id === sectionId);
    if (s) {
      s.tasks.push({
        id: generateId(),
        title,
        done: false,
        expanded: false,
        subtasks: [],
      });
      saveTodos();
    }
  }

  function renameTask(sectionId: string, taskId: string, title: string) {
    const t = sections.value
      .find((s) => s.id === sectionId)
      ?.tasks.find((t) => t.id === taskId);
    if (t) {
      t.title = title;
      saveTodos();
    }
  }

  function toggleTask(sectionId: string, taskId: string) {
    const t = sections.value
      .find((s) => s.id === sectionId)
      ?.tasks.find((t) => t.id === taskId);
    if (t) {
      t.done = !t.done;
      saveTodos();
    }
  }

  function deleteTask(sectionId: string, taskId: string) {
    const s = sections.value.find((s) => s.id === sectionId);
    if (s) {
      s.tasks = s.tasks.filter((t) => t.id !== taskId);
      saveTodos();
    }
  }

  // --- Subtask CRUD ---

  function addSubtask(
    sectionId: string,
    taskId: string,
    title = "New Subtask",
  ) {
    const t = sections.value
      .find((s) => s.id === sectionId)
      ?.tasks.find((t) => t.id === taskId);
    if (t) {
      t.subtasks.push({ id: generateId(), title, done: false });
      saveTodos();
    }
  }

  function renameSubtask(
    sectionId: string,
    taskId: string,
    subtaskId: string,
    title: string,
  ) {
    const st = sections.value
      .find((s) => s.id === sectionId)
      ?.tasks.find((t) => t.id === taskId)
      ?.subtasks.find((st) => st.id === subtaskId);
    if (st) {
      st.title = title;
      saveTodos();
    }
  }

  function toggleSubtask(sectionId: string, taskId: string, subtaskId: string) {
    const st = sections.value
      .find((s) => s.id === sectionId)
      ?.tasks.find((t) => t.id === taskId)
      ?.subtasks.find((st) => st.id === subtaskId);
    if (st) {
      st.done = !st.done;
      saveTodos();
    }
  }

  function deleteSubtask(sectionId: string, taskId: string, subtaskId: string) {
    const t = sections.value
      .find((s) => s.id === sectionId)
      ?.tasks.find((t) => t.id === taskId);
    if (t) {
      t.subtasks = t.subtasks.filter((st) => st.id !== subtaskId);
      saveTodos();
    }
  }

  // --- Duplicate helpers ---

  function duplicateSection(sectionId: string) {
    const s = sections.value.find((s) => s.id === sectionId);
    if (!s) return;
    const idx = sections.value.indexOf(s);
    const newSection: TodoSection = {
      id: generateId(),
      name: s.name + " (copy)",
      collapsed: false,
      tasks: s.tasks.map((t) => ({
        id: generateId(),
        title: t.title,
        done: t.done,
        expanded: t.expanded,
        subtasks: t.subtasks.map((st) => ({
          id: generateId(),
          title: st.title,
          done: st.done,
        })),
      })),
    };
    sections.value.splice(idx + 1, 0, newSection);
    saveTodos();
  }

  function duplicateTask(sectionId: string, taskId: string) {
    const s = sections.value.find((s) => s.id === sectionId);
    if (!s) return;
    const tIdx = s.tasks.findIndex((t) => t.id === taskId);
    if (tIdx === -1) return;
    const original = s.tasks[tIdx];
    const newTask: Task = {
      id: generateId(),
      title: original.title + " (copy)",
      done: original.done,
      expanded: original.expanded,
      subtasks: original.subtasks.map((st) => ({
        id: generateId(),
        title: st.title,
        done: st.done,
      })),
    };
    s.tasks.splice(tIdx + 1, 0, newTask);
    saveTodos();
  }

  // --- Reorder helpers ---

  function reorderSection(fromIndex: number, toIndex: number) {
    if (fromIndex === toIndex) return;
    const [moved] = sections.value.splice(fromIndex, 1);
    sections.value.splice(toIndex, 0, moved);
    saveTodos();
  }

  function reorderTask(sectionId: string, fromIndex: number, toIndex: number) {
    const s = sections.value.find((s) => s.id === sectionId);
    if (!s || fromIndex === toIndex) return;
    const [moved] = s.tasks.splice(fromIndex, 1);
    s.tasks.splice(toIndex, 0, moved);
    saveTodos();
  }

  function moveTaskToSectionAt(
    fromSectionId: string,
    taskIndex: number,
    toSectionId: string,
    toIndex: number,
  ) {
    const from = sections.value.find((s) => s.id === fromSectionId);
    const to = sections.value.find((s) => s.id === toSectionId);
    if (!from || !to) return;
    const [task] = from.tasks.splice(taskIndex, 1);
    to.tasks.splice(toIndex, 0, task);
    saveTodos();
  }

  function reorderSubtask(
    sectionId: string,
    taskId: string,
    fromIndex: number,
    toIndex: number,
  ) {
    const t = sections.value
      .find((s) => s.id === sectionId)
      ?.tasks.find((t) => t.id === taskId);
    if (!t || fromIndex === toIndex) return;
    const [moved] = t.subtasks.splice(fromIndex, 1);
    t.subtasks.splice(toIndex, 0, moved);
    saveTodos();
  }

  function moveSubtaskToTask(
    fromSectionId: string,
    fromTaskId: string,
    subtaskIndex: number,
    toSectionId: string,
    toTaskId: string,
    toIndex: number,
  ) {
    const fromTask = sections.value
      .find((s) => s.id === fromSectionId)
      ?.tasks.find((t) => t.id === fromTaskId);
    const toTask = sections.value
      .find((s) => s.id === toSectionId)
      ?.tasks.find((t) => t.id === toTaskId);
    if (!fromTask || !toTask) return;
    const [subtask] = fromTask.subtasks.splice(subtaskIndex, 1);
    toTask.subtasks.splice(toIndex, 0, subtask);
    saveTodos();
  }

  function toggleTaskExpand(sectionId: string, taskId: string) {
    const t = sections.value
      .find((s) => s.id === sectionId)
      ?.tasks.find((t) => t.id === taskId);
    if (t) {
      t.expanded = !t.expanded;
      saveTodos();
    }
  }

  let _syncCleanup: (() => void) | null = null;
  let _unmounted = false;

  onMounted(async () => {
    await loadTodos();
    // Another window saved (the drawer, or a second overlay). Our own save
    // echoes back too — the equality check keeps it from clobbering state.
    const cleanup = todosStore.onChange((next) => {
      if (JSON.stringify(next) !== JSON.stringify(sections.value)) {
        sections.value = next;
        migrateTasks();
      }
    });
    // loadTodos can outlast the component — unregister right away then
    if (_unmounted) cleanup();
    else _syncCleanup = cleanup;
  });

  onUnmounted(() => {
    _unmounted = true;
    _syncCleanup?.();
    _syncCleanup = null;
  });

  return {
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
  };
}
