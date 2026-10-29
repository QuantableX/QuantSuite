/** Display-only signed log coordinates. The small offset keeps zero and
 * negative equity finite, as in the chart library's native log scale. */
const offset = 1e-4

export function toEquityLog(value: number): number {
  return Math.sign(value) * Math.log1p(Math.abs(value) / offset) / Math.LN10
}

export function fromEquityLog(value: number): number {
  return Math.sign(value) * Math.expm1(Math.abs(value) * Math.LN10) * offset
}
