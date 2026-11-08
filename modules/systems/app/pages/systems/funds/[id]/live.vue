<script setup lang="ts">
definePageMeta({ layout: 'systems', path: '/algo/manual/funds/:id/live' })

import SystemsFundRunPage from '#systems/components/Fund/RunPage.vue'
</script>

<template>
  <SystemsFundRunPage mode="live" />
</template>
