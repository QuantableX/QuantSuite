<template>
  <!-- `h-full` not `h-screen`, and `<slot/>` not `<NuxtPage/>`: the module fills
       the box the shell gives it, and the shell already renders the page inside
       this layout (ARCHITECTURE.md §1). Header and sidebars are the shared V3
       chrome; settings live in the unified QSettingsModal. -->
  <div class="h-full flex flex-col overflow-hidden">
    <TerminalLayoutHeader />
    <div class="flex flex-1 overflow-hidden">
      <TerminalLayoutSidebar />
      <main class="flex-1 min-w-0 overflow-auto" style="background-color: var(--surface-0)">
        <slot />
      </main>
      <TerminalLayoutRightSidebar />
    </div>
  </div>
</template>
