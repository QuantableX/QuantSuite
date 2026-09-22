<script setup lang="ts">
/** One open note. The editor owns loading it — this route only names it. */
definePageMeta({ layout: 'notes' })

const route = useRoute()
const noteId = computed(() => String(route.params.id ?? ''))
</script>

<template>
  <NotesNoteEditor v-if="noteId" :key="noteId" :note-id="noteId" />
</template>
