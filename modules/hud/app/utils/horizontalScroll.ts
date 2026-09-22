/** Route a vertical wheel to the HUD rail without stealing editor/list scroll. */
export function horizontalWheel(event: WheelEvent, viewport: HTMLElement): boolean {
  if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.shiftKey) return false;
  if (!event.deltaY || Math.abs(event.deltaX) >= Math.abs(event.deltaY)) return false;
  const target = event.target instanceof Element ? event.target : null;
  if (target?.closest('input, textarea, select, [contenteditable="true"], [role="slider"]')) return false;

  for (let node = target; node && node !== viewport; node = node.parentElement) {
    const style = getComputedStyle(node);
    if (/auto|scroll/.test(style.overflowY) && node.scrollHeight > node.clientHeight + 1) {
      const canScroll = event.deltaY < 0 ? node.scrollTop > 0
        : node.scrollTop + node.clientHeight < node.scrollHeight - 1;
      if (canScroll) return false;
    }
  }
  const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? viewport.clientWidth : 1);
  const next = Math.max(0, Math.min(viewport.scrollLeft + delta, viewport.scrollWidth - viewport.clientWidth));
  if (Math.abs(next - viewport.scrollLeft) < 1) return false;
  event.preventDefault();
  viewport.scrollLeft = next;
  return true;
}
