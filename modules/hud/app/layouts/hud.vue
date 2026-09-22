<template>
  <!--
    QuantHUD renders in its OWN 340x900 window, not inside the shell's stage.
    The shell suppresses its titlebar and rail for any window that is not
    `main`, so this layout is the whole window (ARCHITECTURE.md 10).

    `height: 100%` rather than `100vh`: shell.css already gives
    html/body/#__nuxt a definite height, and keeping every module off viewport
    units means the guard stays simple and this still works if the HUD is ever
    embedded somewhere.
  -->
  <div class="app-layout">
    <slot />
  </div>
</template>

<style scoped>
.app-layout {
  height: 100%;
  width: 100%;
  display: flex;
  flex-direction: column;
}
</style>