/** Exact non-negative cents. Accepts German/English grouping and up to two decimals. */
export function parseFundCents(raw: string): number | null {
  const text = raw.trim().replace(/[\s\u00a0\u202f]/g, '')
  if (!/^\d[\d.,]*$/.test(text)) return null
  const last = Math.max(text.lastIndexOf(','), text.lastIndexOf('.'))
  let whole = text
  let fraction = ''
  if (last >= 0) {
    const tail = text.slice(last + 1)
    if (tail.length === 1 || tail.length === 2) {
      whole = text.slice(0, last)
      fraction = tail
      // A decimal separator cannot also group the integer part.
      if (whole.includes(text[last]!)) return null
    } else if (tail.length !== 3) return null
  }
  if (!/^\d+$/.test(whole) && !/^\d{1,3}(,\d{3})+$/.test(whole) && !/^\d{1,3}(\.\d{3})+$/.test(whole)) return null
  const cents = BigInt(whole.replace(/[.,]/g, '')) * 100n + BigInt(fraction.padEnd(2, '0'))
  return cents <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(cents) : null
}
