const TICK_OPTIONS = [0.01, 0.1, 1, 5, 10, 50, 100]

const tickIndex = ref(4) // default $10

export function useOrderbookTick() {
  const tickSize = computed(() => TICK_OPTIONS[tickIndex.value]!)

  function selectTick(idx: number) {
    tickIndex.value = idx
  }

  function formatTick(t: number): string {
    if (t >= 1) return '$' + t.toFixed(0)
    return '$' + t.toString()
  }

  return {
    TICK_OPTIONS,
    tickIndex,
    tickSize,
    selectTick,
    formatTick,
  }
}
