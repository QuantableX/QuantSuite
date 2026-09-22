/**
 * `v-drag-window` — makes an element drag the suite window.
 *
 * Why a directive and not `data-tauri-drag-region`: that attribute only applies
 * when the element carrying it *is* the click target. Every module titlebar is
 * fully covered by child containers (logo section, nav section, actions
 * section), so the bar itself is never the target and the window never moves.
 * This walks up from the actual target instead, which is what the standalone
 * apps did with their own mousedown handlers.
 *
 * Interactive descendants are skipped, so buttons, links and inputs in the bar
 * keep working. Double-click toggles maximize, matching the OS convention and
 * what `data-tauri-drag-region` would have given for free.
 *
 * Opt an element out with `data-no-drag` or any `*-no-drag` class the module
 * already uses.
 */

const INTERACTIVE = 'button, a, input, select, textarea, label, [role="button"], [data-no-drag], [class*="no-drag"]'

const DOUBLE_CLICK_MS = 300

export default defineNuxtPlugin((nuxtApp) => {
  const lastClick = new WeakMap<HTMLElement, number>()

  async function currentWindow() {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window')
      return getCurrentWindow()
    } catch {
      return null // not in Tauri — nothing to drag
    }
  }

  function onMousedown(this: HTMLElement, event: MouseEvent) {
    if (event.button !== 0) return
    const target = event.target as HTMLElement | null
    if (!target || target.closest(INTERACTIVE)) return

    const now = Date.now()
    const previous = lastClick.get(this) ?? 0
    lastClick.set(this, now)

    void currentWindow().then((w) => {
      if (!w) return
      if (now - previous < DOUBLE_CLICK_MS) void w.toggleMaximize()
      else void w.startDragging()
    })
  }

  nuxtApp.vueApp.directive('drag-window', {
    mounted(el: HTMLElement) {
      el.addEventListener('mousedown', onMousedown)
    },
    unmounted(el: HTMLElement) {
      el.removeEventListener('mousedown', onMousedown)
    },
  })
})
