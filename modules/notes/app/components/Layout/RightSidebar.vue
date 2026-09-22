<script setup lang="ts">
/**
 * The right panel: the open note's outline, or — with no note open — a
 * summary of the collection's schema.
 *
 * Property *editing* lives in the note body and in the views' cells, not here.
 * Duplicating those controls in a panel is how the old module ended up with
 * three editors per property kind.
 */
import { useNotesStore } from '#notes/stores/notes'
import { useSchemaStore } from '#notes/stores/schema'

const notes = useNotesStore()
const schema = useSchemaStore()

interface OutlineItem {
  level: number
  text: string
}

/** Headings of the open document, read straight off the TipTap JSON. */
const outline = computed<OutlineItem[]>(() => {
  const content = notes.openNote?.content?.content
  if (!Array.isArray(content)) return []
  const items: OutlineItem[] = []
  for (const node of content) {
    if (node?.type !== 'heading') continue
    const text = (node.content ?? [])
      .map((c: any) => c?.text ?? '')
      .join('')
      .trim()
    if (text) items.push({ level: node.attrs?.level ?? 1, text })
  }
  return items
})

const KIND_LABELS: Record<string, string> = {
  text: 'Text',
  number: 'Number',
  select: 'Select',
  multi_select: 'Multi-select',
  date: 'Date',
  checkbox: 'Checkbox',
  url: 'URL',
}
</script>

<template>
  <div class="qn-rs">
    <header class="qn-rs__head">
      <span>{{ notes.openNote ? 'Outline' : 'Properties' }}</span>
    </header>

    <div class="qn-rs__body">
      <template v-if="notes.openNote">
        <a
          v-for="(item, i) in outline"
          :key="`${i}-${item.text}`"
          class="qn-rs__outline"
          :style="{ paddingLeft: `${10 + (item.level - 1) * 12}px` }"
        >{{ item.text }}</a>
        <p v-if="!outline.length" class="qn-rs__empty">
          Headings in this note show up here.
        </p>
      </template>

      <template v-else>
        <div v-for="property in schema.properties" :key="property.id" class="qn-rs__prop">
          <span class="qn-rs__prop-name">{{ property.name }}</span>
          <span class="qn-rs__prop-kind">{{ KIND_LABELS[property.kind] ?? property.kind }}</span>
        </div>
        <p v-if="!schema.properties.length" class="qn-rs__empty">
          No properties yet — add one from the view toolbar.
        </p>
      </template>
    </div>
  </div>
</template>

<style scoped>
.qn-rs {
  height: 100%;
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.qn-rs__head {
  flex-shrink: 0;
  padding: 12px 14px;
  border-bottom: 1px solid var(--qn-border);
  color: var(--qn-text-secondary);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

.qn-rs__body {
  flex: 1;
  min-height: 0;
  padding: 6px;
  overflow: auto;
}

.qn-rs__outline {
  display: block;
  padding: 5px 10px;
  border-radius: 6px;
  color: var(--qn-text-secondary);
  font-size: 12px;
  line-height: 1.4;
  text-decoration: none;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qn-rs__outline:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}

.qn-rs__prop {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 10px;
  border-radius: 6px;
}

.qn-rs__prop:hover {
  background: var(--qn-bg-hover);
}

.qn-rs__prop-name {
  color: var(--qn-text-secondary);
  font-size: 12px;
}

.qn-rs__prop-kind {
  color: var(--qn-text-muted);
  font-size: 11px;
}

.qn-rs__empty {
  margin: 0;
  padding: 10px;
  color: var(--qn-text-muted);
  font-size: 12px;
  line-height: 1.5;
}
</style>
