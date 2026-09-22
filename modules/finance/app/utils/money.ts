/**
 * The only place cents become a string, and the only place a string becomes
 * cents.
 *
 * Money is `i64` cents from the database to the DOM. It is turned into a
 * decimal exactly once, at the moment it is displayed. If you find yourself
 * writing `amount / 100` anywhere else, that is the bug.
 */

/** `123456` → `"1.234,56 €"` (locale-dependent). */
export function formatCents(cents: number, currency = 'EUR', locale?: string): string {
  return new Intl.NumberFormat(locale, {
    style: 'currency',
    currency,
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(cents / 100)
}

/** Without the currency symbol — for table columns that carry it in the header. */
export function formatAmount(cents: number, locale?: string): string {
  return new Intl.NumberFormat(locale, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(cents / 100)
}

/** A compact axis/label form: `123456` → `"1.235"`, `12345678` → `"123k"`. */
export function formatCompact(cents: number, locale?: string): string {
  const units = Math.round(cents / 100)
  return new Intl.NumberFormat(locale, { notation: 'compact', maximumFractionDigits: 1 }).format(units)
}

/**
 * Parse a typed amount into cents.
 *
 * Accepts both separators (`1.234,56` and `1,234.56`) by treating the LAST
 * separator as the decimal point and dropping the rest as grouping — which is
 * what a person typing either convention means. Returns `null` for anything
 * it cannot read, so a caller can refuse rather than store a silent zero.
 *
 * Rounding happens once, here, on a value that has been a string until now —
 * never on an accumulated float.
 */
export function parseCents(raw: string): number | null {
  const trimmed = raw.trim().replace(/\s| /g, '')
  if (!trimmed) return null

  const negative = /^-/.test(trimmed) || /^\(.*\)$/.test(trimmed)
  const digitsOnly = trimmed.replace(/[^0-9.,]/g, '')
  if (!digitsOnly) return null

  const lastComma = digitsOnly.lastIndexOf(',')
  const lastDot = digitsOnly.lastIndexOf('.')
  const decimalAt = Math.max(lastComma, lastDot)

  let whole = digitsOnly
  let fraction = ''
  if (decimalAt !== -1) {
    const tail = digitsOnly.slice(decimalAt + 1)
    // Three digits after the last separator is grouping, not cents: "1.234".
    if (tail.length !== 3 || digitsOnly.split(/[.,]/).length === 2) {
      whole = digitsOnly.slice(0, decimalAt)
      fraction = tail
    }
  }

  const wholeDigits = whole.replace(/[^0-9]/g, '')
  const fractionDigits = (fraction.replace(/[^0-9]/g, '') + '00').slice(0, 2)
  if (!wholeDigits && !fraction) return null

  const cents = Number(wholeDigits || '0') * 100 + Number(fractionDigits)
  if (!Number.isFinite(cents)) return null
  return negative ? -cents : cents
}
