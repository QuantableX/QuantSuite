/** A web link in the user's browser (the shell plugin; a plain window in the browser build). */
export async function openExternal(url: string) {
  try {
    const { open } = await import('@tauri-apps/plugin-shell')
    await open(url)
  } catch {
    window.open(url, '_blank', 'noopener')
  }
}
