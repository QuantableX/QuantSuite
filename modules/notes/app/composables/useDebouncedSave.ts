export function useDebouncedSave(delay = 800) {
  const timers = new Map<string, ReturnType<typeof setTimeout>>()

  function schedule(key: string, callback: () => void | Promise<void>) {
    const existing = timers.get(key)
    if (existing) clearTimeout(existing)
    timers.set(key, setTimeout(() => {
      timers.delete(key)
      void callback()
    }, delay))
  }

  function flush(key: string, callback: () => void | Promise<void>) {
    const existing = timers.get(key)
    if (existing) {
      clearTimeout(existing)
      timers.delete(key)
    }
    return callback()
  }

  return {
    schedule,
    flush,
  }
}
