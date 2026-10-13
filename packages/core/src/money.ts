/**
 * The suite writes money number first, symbol last, with no space:
 * `4,123.45€`, `83,420.00$`, `-250.00€`, `+12.50$` (user preference, 2026-10-14).
 * Hand-built amounts (compact `1.65T$`, fixed decimals) follow the same order.
 */
export function formatMoney(
  amount: number,
  currency = 'USD',
  options: Omit<Intl.NumberFormatOptions, 'style' | 'currency' | 'currencyDisplay'> = {},
): string {
  let parts: Intl.NumberFormatPart[]
  try {
    parts = new Intl.NumberFormat('en-US', {
      ...options,
      style: 'currency',
      currency,
      currencyDisplay: 'narrowSymbol',
    }).formatToParts(amount)
  } catch {
    // Not an ISO code (old free-text settings): keep the number, show the code.
    return new Intl.NumberFormat('en-US', options).format(amount) + currency
  }
  let symbol = '',
    number = ''
  for (const part of parts) {
    if (part.type === 'currency') symbol += part.value
    else if (part.type !== 'literal' || part.value.trim()) number += part.value
  }
  return number + symbol
}

/** The narrow symbol of a currency code: `EUR` → `€`, `USD` → `$`. */
export function currencySymbol(currency: string): string {
  try {
    return (
      new Intl.NumberFormat('en-US', { style: 'currency', currency, currencyDisplay: 'narrowSymbol' })
        .formatToParts(0)
        .find((part) => part.type === 'currency')?.value ?? currency
    )
  } catch {
    return currency
  }
}
