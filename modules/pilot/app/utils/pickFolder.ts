/**
 * The system folder dialog (round 3, card 01: a folder for a thread's new
 * project). Null when cancelled or outside the app.
 */
export async function pickFolder(title: string): Promise<string | null> {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const picked = await open({ directory: true, multiple: false, title })
    return typeof picked === 'string' ? picked : null
  } catch {
    return null
  }
}
