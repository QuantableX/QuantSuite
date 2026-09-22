import { getCurrentWindow } from '@tauri-apps/api/window'

/** Keep aligned with qs-core/window.rs and the suite-* capability. */
export function isSuiteWindow(label: string): boolean {
  return label === 'main' || label.startsWith('suite-')
}

/** Browser previews use the primary shell; native windows use their own label. */
export function currentWindowLabel(): string {
  if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) return 'main'
  return getCurrentWindow().label
}
