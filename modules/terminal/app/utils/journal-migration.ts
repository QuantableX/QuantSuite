import type { JournalTrade } from '../stores/journal'

// Frozen contents of the 20 examples shipped before 1.0.19. These are only
// deletion signatures: never insert them into a user's journal.
const legacySamples: Omit<JournalTrade, 'id'>[] = [
  {"date":"2026-03-25","pair":"BTC/USDT","side":"long","entry":69800,"exit":71250,"size":0.4,"status":"closed","notes":"Entered on support bounce at 69.5k zone. Clean breakout above consolidation.","tags":["setup-A","technical"],"strategy":"Breakout","exchange":"Binance","fees":5.64,"screenshots":[],"emotion":"confident","rating":5,"pnl":574.36,"pnlPercent":2.08,"createdAt":"2026-03-01T00:00:00.000Z","updatedAt":"2026-03-01T00:00:00.000Z"},
  {"date":"2026-03-24","pair":"ETH/USDT","side":"short","entry":3950,"exit":3820,"size":3,"status":"closed","notes":"Rejection at 4000 psychological resistance. Bearish divergence on RSI.","tags":["technical","high-conviction"],"strategy":"Mean Reversion","exchange":"Binance","fees":3.12,"screenshots":[],"emotion":"confident","rating":4,"pnl":386.88,"pnlPercent":3.29,"createdAt":"2026-03-02T00:00:00.000Z","updatedAt":"2026-03-02T00:00:00.000Z"},
  {"date":"2026-03-23","pair":"SOL/USDT","side":"long","entry":168,"exit":159,"size":15,"status":"closed","notes":"Stopped out. Failed breakout turned into bull trap.","tags":["setup-A"],"strategy":"Breakout","exchange":"Bybit","fees":2.5,"screenshots":[],"emotion":"neutral","rating":3,"pnl":-137.5,"pnlPercent":-5.36,"createdAt":"2026-03-03T00:00:00.000Z","updatedAt":"2026-03-03T00:00:00.000Z"},
  {"date":"2026-03-22","pair":"BNB/USDT","side":"long","entry":580,"exit":602,"size":5,"status":"closed","notes":"Mean reversion play from oversold RSI. Took profit at 20 EMA.","tags":["technical"],"strategy":"Mean Reversion","exchange":"Binance","fees":2.96,"screenshots":[],"emotion":"neutral","rating":4,"pnl":107.04,"pnlPercent":3.79,"createdAt":"2026-03-04T00:00:00.000Z","updatedAt":"2026-03-04T00:00:00.000Z"},
  {"date":"2026-03-21","pair":"BTC/USDT","side":"long","entry":68200,"exit":69500,"size":0.3,"status":"closed","notes":"Trend continuation after 4H pullback to EMA. News catalyst from ETF flows.","tags":["news-driven","technical"],"strategy":"Trend Following","exchange":"Binance","fees":4.14,"screenshots":[],"emotion":"confident","rating":4,"pnl":385.86,"pnlPercent":1.91,"createdAt":"2026-03-05T00:00:00.000Z","updatedAt":"2026-03-05T00:00:00.000Z"},
  {"date":"2026-03-19","pair":"XRP/USDT","side":"long","entry":0.62,"exit":0.58,"size":5000,"status":"closed","notes":"Bottom fishing attempt. Market had more downside than expected.","tags":["high-conviction"],"strategy":"Mean Reversion","exchange":"Bybit","fees":1.5,"screenshots":[],"emotion":"greedy","rating":2,"pnl":-201.5,"pnlPercent":-6.45,"createdAt":"2026-03-07T00:00:00.000Z","updatedAt":"2026-03-07T00:00:00.000Z"},
  {"date":"2026-03-18","pair":"ETH/USDT","side":"long","entry":3680,"exit":3760,"size":4,"status":"closed","notes":"Quick scalp on 15m chart. Double bottom pattern.","tags":["setup-A","technical"],"strategy":"Scalp","exchange":"Binance","fees":2.97,"screenshots":[],"emotion":"neutral","rating":3,"pnl":317.03,"pnlPercent":2.17,"createdAt":"2026-03-08T00:00:00.000Z","updatedAt":"2026-03-08T00:00:00.000Z"},
  {"date":"2026-03-16","pair":"SOL/USDT","side":"short","entry":182,"exit":175,"size":12,"status":"closed","notes":"Breakdown below ascending wedge. Clean setup.","tags":["technical"],"strategy":"Breakout","exchange":"Bybit","fees":2.14,"screenshots":[],"emotion":"confident","rating":5,"pnl":81.86,"pnlPercent":3.85,"createdAt":"2026-03-10T00:00:00.000Z","updatedAt":"2026-03-10T00:00:00.000Z"},
  {"date":"2026-03-14","pair":"BTC/USDT","side":"short","entry":71500,"exit":72800,"size":0.2,"status":"closed","notes":"Tried to short the top. Squeezed out by strong buying.","tags":["news-driven"],"strategy":"Mean Reversion","exchange":"Binance","fees":5.86,"screenshots":[],"emotion":"fearful","rating":1,"pnl":-265.86,"pnlPercent":-1.82,"createdAt":"2026-03-12T00:00:00.000Z","updatedAt":"2026-03-12T00:00:00.000Z"},
  {"date":"2026-03-12","pair":"BNB/USDT","side":"short","entry":610,"exit":595,"size":8,"status":"closed","notes":"Head and shoulders breakdown on 1H chart.","tags":["technical","setup-A"],"strategy":"Breakout","exchange":"Binance","fees":3.84,"screenshots":[],"emotion":"confident","rating":4,"pnl":116.16,"pnlPercent":2.46,"createdAt":"2026-03-14T00:00:00.000Z","updatedAt":"2026-03-14T00:00:00.000Z"},
  {"date":"2026-03-08","pair":"ETH/USDT","side":"long","entry":3520,"exit":3410,"size":5,"status":"closed","notes":"Entered too early on a falling knife. Should have waited for confirmation.","tags":["technical"],"strategy":"Trend Following","exchange":"Binance","fees":3.46,"screenshots":[],"emotion":"frustrated","rating":1,"pnl":-553.46,"pnlPercent":-3.12,"createdAt":"2026-03-18T00:00:00.000Z","updatedAt":"2026-03-18T00:00:00.000Z"},
  {"date":"2026-03-04","pair":"SOL/USDT","side":"long","entry":155,"exit":172,"size":20,"status":"closed","notes":"Swing trade. Entered on weekly support with bullish engulfing.","tags":["high-conviction","technical"],"strategy":"Swing","exchange":"Bybit","fees":3.27,"screenshots":[],"emotion":"confident","rating":5,"pnl":336.73,"pnlPercent":10.97,"createdAt":"2026-03-22T00:00:00.000Z","updatedAt":"2026-03-22T00:00:00.000Z"},
  {"date":"2026-03-01","pair":"BTC/USDT","side":"long","entry":64200,"exit":65800,"size":0.5,"status":"closed","notes":"Breakout above descending trendline. Strong volume.","tags":["setup-A","technical"],"strategy":"Breakout","exchange":"Binance","fees":6.5,"screenshots":[],"emotion":"neutral","rating":4,"pnl":793.5,"pnlPercent":2.49,"createdAt":"2026-03-25T00:00:00.000Z","updatedAt":"2026-03-25T00:00:00.000Z"},
  {"date":"2026-02-24","pair":"XRP/USDT","side":"short","entry":0.65,"exit":0.61,"size":8000,"status":"closed","notes":"Overbought on daily RSI. Clean rejection at resistance.","tags":["technical"],"strategy":"Mean Reversion","exchange":"Bybit","fees":2.56,"screenshots":[],"emotion":"neutral","rating":3,"pnl":317.44,"pnlPercent":6.15,"createdAt":"2026-03-02T00:00:00.000Z","updatedAt":"2026-03-02T00:00:00.000Z"},
  {"date":"2026-02-19","pair":"ETH/USDT","side":"long","entry":3280,"exit":3350,"size":6,"status":"closed","notes":"Trend following after golden cross on 4H chart.","tags":["technical","setup-A"],"strategy":"Trend Following","exchange":"Binance","fees":3.98,"screenshots":[],"emotion":"confident","rating":4,"pnl":416.02,"pnlPercent":2.13,"createdAt":"2026-03-07T00:00:00.000Z","updatedAt":"2026-03-07T00:00:00.000Z"},
  {"date":"2026-02-14","pair":"BTC/USDT","side":"short","entry":62500,"exit":61800,"size":0.3,"status":"closed","notes":"Scalp on 5m bearish engulfing at local high.","tags":["technical"],"strategy":"Scalp","exchange":"Binance","fees":3.73,"screenshots":[],"emotion":"neutral","rating":3,"pnl":206.27,"pnlPercent":1.12,"createdAt":"2026-03-12T00:00:00.000Z","updatedAt":"2026-03-12T00:00:00.000Z"},
  {"date":"2026-02-09","pair":"SOL/USDT","side":"long","entry":142,"exit":138,"size":25,"status":"closed","notes":"False breakout. Got trapped above resistance that did not hold.","tags":["setup-A"],"strategy":"Breakout","exchange":"Bybit","fees":3.55,"screenshots":[],"emotion":"frustrated","rating":2,"pnl":-103.55,"pnlPercent":-2.82,"createdAt":"2026-03-17T00:00:00.000Z","updatedAt":"2026-03-17T00:00:00.000Z"},
  {"date":"2026-02-04","pair":"BNB/USDT","side":"long","entry":545,"exit":568,"size":6,"status":"closed","notes":"Bounce off 200 EMA on daily. Took profit at prior resistance.","tags":["technical","high-conviction"],"strategy":"Swing","exchange":"Binance","fees":3.34,"screenshots":[],"emotion":"confident","rating":4,"pnl":134.66,"pnlPercent":4.22,"createdAt":"2026-03-22T00:00:00.000Z","updatedAt":"2026-03-22T00:00:00.000Z"},
  {"date":"2026-03-25","pair":"BTC/USDT","side":"long","entry":70500,"exit":null,"size":0.25,"status":"open","notes":"Scaling in on pullback to 20 EMA. Targeting 73k.","tags":["trend","technical"],"strategy":"Trend Following","exchange":"Binance","fees":1.76,"screenshots":[],"emotion":"confident","rating":4,"pnl":null,"pnlPercent":null,"createdAt":"2026-03-01T00:00:00.000Z","updatedAt":"2026-03-01T00:00:00.000Z"},
  {"date":"2026-03-24","pair":"ETH/USDT","side":"long","entry":3880,"exit":null,"size":3,"status":"open","notes":"Accumulation zone. Building position for ETH/BTC ratio reversal.","tags":["high-conviction"],"strategy":"Swing","exchange":"Binance","fees":1.16,"screenshots":[],"emotion":"neutral","rating":3,"pnl":null,"pnlPercent":null,"createdAt":"2026-03-02T00:00:00.000Z","updatedAt":"2026-03-02T00:00:00.000Z"},
]

const fields = Object.keys(legacySamples[0]!) as (keyof Omit<JournalTrade, 'id'>)[]
const signature = (trade: Record<string, unknown>) => JSON.stringify(fields.map(field => trade[field]))
const legacySignatures = new Map<string, number>()

for (const [index, sample] of legacySamples.entries()) {
  legacySignatures.set(signature(sample), index)
  // The old generator used local setDate across March 2026 clock changes.
  // Captured IANA-zone outputs differ only by +1h in these metadata fields;
  // match the stored variant even if the user has since changed time zones.
  if (index !== 12) {
    const timestamp = sample.createdAt.replace('T00:', 'T01:')
    legacySignatures.set(signature({ ...sample, createdAt: timestamp, updatedAt: timestamp }), index)
  }
}

function isUnchangedLegacySample(value: unknown): boolean {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false
  const trade = value as Record<string, unknown>
  // Extra fields may contain user data. Edited timestamps, notes, screenshots,
  // prices, tags, etc. also make the full signature differ and are preserved.
  if (Object.keys(trade).length !== fields.length + 1 || typeof trade.id !== 'string') return false
  const index = legacySignatures.get(signature(trade))
  // Old IDs were an 8-character base36 time, up to 6 random characters, then
  // the sample's array index. Manual/imported records with other IDs survive.
  return index !== undefined && new RegExp('^[a-z0-9]{8,14}' + index + '$').test(trade.id)
}

export function removeLegacyJournalSamples<T>(trades: T[]): T[] {
  const remaining = trades.filter(trade => !isUnchangedLegacySample(trade))
  return remaining.length === trades.length ? trades : remaining
}
