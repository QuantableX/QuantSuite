<script setup lang="ts">
definePageMeta({ layout: 'systems', path: '/algo/manual/:id' })

import { useAppStore } from '#systems/stores/app'

const route = useRoute()
const app = useAppStore()
onMounted(() => {
  const id = route.params.id as string
  navigateTo(`/algo/manual/${id}/${app.viewFor(id)}`, { replace: true })
})
</script>

<template>
  <div />
</template>
