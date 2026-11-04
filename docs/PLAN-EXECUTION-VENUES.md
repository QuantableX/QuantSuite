# Execution venues — where the orders go, decentralised exchanges first

Status: plan, 2026-11-03. Nothing here is built yet. Cards "QuantAlgo Venues 1/5" to "5/5" on the QuantSuite board. The engine that places the orders is [PLAN-AUTO-EXECUTION.md](PLAN-AUTO-EXECUTION.md) §2.2.

## In short

- Every Manual system and every bot gets **two separate choices**: where the signal comes from (data source) and where the orders go (execution venue: exchange + spot or perp + account). Today both are the same exchange.
- **New venues, decentralised first:** Hyperliquid (spot and perp, first), Jupiter on Solana (spot, second), Lighter (spot and perp, a second order-book DEX so one venue is never the only option). Binance and Bybit spot stay.
- **Manual defaults to spot.** A system that holds a coin for days pays perp funding every hour; over a few days that costs more than the lower perp trading fee saves (§5).
- **Honest limit:** on DEX *spot*, only part of a Top-N universe is tradable (BTC, ETH, SOL and a few more, as tokenised versions). A coverage check shows the exact list per system and venue before you switch — never guessed.
- **Keys:** on a DEX you hold your own keys. Hyperliquid's API wallets can trade but cannot withdraw. A Solana trading wallet can do everything, so it only ever holds the trading capital.
- **"BTC is too expensive on a DEX?"** Trading BTC there costs the same fee as any other coin. What is slow and expensive is moving *native* BTC on the Bitcoin chain — you pay that only when you deposit or withdraw, not per trade (§5.2).

## 1. Why decentralised

On a centralised exchange (CEX) the exchange holds your coins: a hack, a freeze or an insolvency can take them. On a decentralised exchange (DEX) you keep custody. The risks do not disappear, they change:

| Risk | CEX (Binance, Bybit) | DEX (Hyperliquid, Jupiter, Lighter) |
|---|---|---|
| Exchange hacked / insolvent / freezes accounts | Your coins are at risk | Your coins are in your own wallet or account on-chain |
| Smart-contract or bridge bug | — | Possible; tokenised BTC/ETH depend on a bridge or custodian |
| Chain or sequencer halt | — | Trading pauses until it resumes |
| Your key stolen from your machine | API key: trades only (if withdrawals are off) | Hyperliquid API wallet: trades only. Solana wallet: everything → small balance only |
| Front-running (MEV) | — | Mostly on AMM swaps (Jupiter); limited by slippage caps (§7) |
| Accounts, KYC, regional blocks | Yes | Usually none |

So the venue choice is a per-system setting, and spreading capital over venues is the real protection. QuantSuite shows the exposure per venue on the Trading overview.

## 2. Concepts

| Term | Meaning | Example |
|---|---|---|
| Data source | The candles the indicators read. Unchanged (systems data rules: Binance first, never Gate.io/MEXC). | Binance BTC/USDT 1d |
| Execution venue | Exchange + market type | Hyperliquid spot |
| Account | Credentials or wallet, optionally a sub-account | "HL – LCES sub-account" |
| Instrument mapping | The data pair → the venue's instrument, verified by id, never by symbol name | BTC/USDT → Binance `BTCUSDT`, Hyperliquid spot `UBTC/USDC` (token index), Hyperliquid perp `BTC`, Solana cbBTC (mint address) |
| Quote asset | What "USD" means on the venue | USDT on Binance/Bybit, USDC on Hyperliquid/Jupiter/Lighter |

Mapping by id matters on-chain: anyone can create a token called "BTC". QuantSuite maps only to verified ids and shows the id next to the name.

## 3. Planned venues

| Venue | Kind | Spot | Perp | What QuantSuite stores | Can that key withdraw? | Test network |
|---|---|---|---|---|---|---|
| Binance | CEX | today | not planned | API key + secret | No, if withdrawals are off on the key | Spot testnet |
| Bybit | CEX | today | not planned | API key + secret | No, if withdrawals are off on the key | Demo trading |
| **Hyperliquid** | DEX, own L1 with an on-chain order book | yes | yes | API-wallet private key + your main address | **No.** Withdrawals and transfers are user-signed actions that need your main wallet's key | Testnet |
| **Jupiter** (Solana) | DEX aggregator over Solana's pools | yes | — | A Solana keypair made for QuantSuite | **Yes** → only trading capital on it | No useful liquidity on devnet → small live canary instead |
| **Lighter** | DEX, zk-rollup order book settled on Ethereum | yes | yes | API key (separate from the wallet) | To verify at card start | Testnet |

Why these three:
- **Hyperliquid** has the deepest DEX order books, spot and perp in one account, low fees, an official API and API wallets that cannot withdraw. One adapter gives both market types.
- **Jupiter** reaches the widest set of spot tokens on one cheap chain. Its current API is Swap V2 (`api.jup.ag/swap/v2`); the older Ultra API is no longer maintained. Alternative if coverage is better: CoW Protocol on Arbitrum/Base (batch auctions protect against MEV). The card re-checks coverage against the systems' universes first and records the choice.
- **Lighter** is the main order-book challenger, with Ethereum settlement as an escape hatch. It is the redundancy venue: if Hyperliquid halts or must be avoided, perp systems can move. The card re-checks it against dYdX at start.

CEX perps (Binance, Bybit futures) are not planned; ask if you want them.

## 4. Coverage — what each venue can actually trade

A rotation system picks from a Top-N universe by market cap. Not every coin exists on every venue:

| Venue | Expected coverage of a Top-20 universe |
|---|---|
| Binance spot | Nearly all |
| Hyperliquid perp | Most |
| Hyperliquid spot | A handful (BTC, ETH, SOL via Unit tokens, HYPE, a few more) |
| Jupiter | What is bridged to Solana with real liquidity (SOL, cbBTC/WBTC, ETH, a few more) |

These are expectations. The coverage check computes the real list live from the venue's API and the verified mapping table, per system, on the day you look. When a coin is missing, the system's policy decides ([PLAN-AUTO-EXECUTION.md](PLAN-AUTO-EXECUTION.md) §2.3): restrict the universe (the backtest does the same, so backtest = live) or hold USD for that pick (flagged as backtest ≠ live).

## 5. Spot or perp — with numbers

### 5.1 One switch and ten days of holding, 10,000$

The system sells BTC, buys ETH, and holds ETH for 10 days. Taker fees, entry tier, no discounts.

| | Binance spot | Hyperliquid spot | Hyperliquid perp 1x | Jupiter spot |
|---|---|---|---|---|
| Fees for the switch (2 legs) | 0.10% × 2 = **20$** | 0.070% × 2 = **14$** | 0.045% × 2 = **9$** | No Jupiter fee on the Swap API; pool fees ≈ 0.01–0.3% per leg (2–60$) + network ≈ 0.01$ |
| Holding 10 days | 0$ | 0$ | Funding, paid hourly. At the base rate (0.01% per 8 h, longs pay) ≈ **30$**; negative in some phases, much higher in hype phases | 0$ |
| Total | **20$** | **14$** | **≈ 39$** | depends on the route |

Where a venue lists a direct pair (on Binance for example ETH/BTC), the switch can be one trade: 10$ instead of 20$ plus the pair's spread. The execution core takes the cheaper route per switch ([PLAN-AUTO-EXECUTION.md](PLAN-AUTO-EXECUTION.md) §2.2, "Route choice"). Hyperliquid spot and every perp have no such pairs, so their numbers above stay.

Hyperliquid funding: interest part fixed at 0.01% per 8 hours, paid every hour at one eighth, capped at 0.5% per hour. Over a year at the base rate that is ≈ 11% of the position for a long.

**Rule of thumb:** a perp is cheaper only when the holding period is short (about two days or less at the base rate) or when the system needs to be short. Manual rotation systems hold for days → spot by default. Perp is a choice, shown with this cost preview.

### 5.2 BTC on a DEX

On a DEX you trade a *tokenised* BTC, not native bitcoin:
- Hyperliquid: UBTC, from the Unit bridge (native BTC in, held by a 2-of-3 MPC guardian group).
- Solana: cbBTC (Coinbase custody) or WBTC (BitGo custody).

The trading fee is the same as for any pair (14$ on 10,000$ in the table). The cost you were thinking of is the Bitcoin chain: moving native BTC in or out takes a Bitcoin transaction and its fee, and confirmations take minutes to an hour. The system never does that — it trades inside the venue against USDC. You pay it only when you deposit or withdraw. The real price of BTC on a DEX is trust in the bridge or custodian, and QuantSuite names it next to every tokenised asset.

## 6. Perp safety (card 4)

- Default 1x, isolated margin. The leverage setting is capped at 3x unless the user raises the cap.
- Liquidation price and its distance are shown per position; a guard refuses a position whose liquidation distance is under the user's limit.
- Funding payments are booked in the journal, so the equity curve shows the true cost.
- Closes are reduce-only. Short positions only exist on perp (single-asset long/short systems, Automated bots that reverse).
- Spot stays long-only.

## 7. Execution quality and MEV

- Order-book venues (Binance, Bybit, Hyperliquid, Lighter): immediate-or-cancel limit orders at the slippage cap.
- Jupiter: quote → swap transaction with a slippage cap (default 50 bps) and a price-impact cap; a quote older than 10 s is refreshed; the transaction is simulated before it is sent; confirmation by signature status; priority fee from recent fees.
- Every venue: the price-sanity guard compares the venue price with the signal's reference price ([PLAN-AUTO-EXECUTION.md](PLAN-AUTO-EXECUTION.md) §2.2).
- Switches: a direct pair or through the quote asset, whichever costs less for the size by fees and the live book; through USD when unsure ([PLAN-AUTO-EXECUTION.md](PLAN-AUTO-EXECUTION.md) §2.2, "Route choice"). The instrument mapping (§2) therefore also lists a venue's direct pairs between mapped coins.

## 8. Keys and accounts per venue

- **CEX:** create the key with trade permission only, withdrawals off, IP whitelist = the machine that trades (this PC, later the server). One sub-account per system.
- **Hyperliquid:** in Hyperliquid's app create an API wallet and approve it once with your main wallet. Paste the API wallet's private key and your main address into QuantSuite. QuantSuite never sees your main key. One sub-account per system, one API wallet per sub-account (shared nonces collide). Never reuse an old API wallet address (Hyperliquid warns that old signed actions can be replayed once it is pruned). An API wallet can carry an expiry; QuantSuite shows it and warns 7 days ahead. Moving USDC between spot and perp balances is a user-signed action, so you do it in Hyperliquid's app.
- **Solana (Jupiter):** QuantSuite generates a new keypair inside the vault and shows its address. You fund it with the trading capital only. QuantSuite never asks for a seed phrase. Exporting the key needs a confirmation. A cap "max value on this wallet" blocks arming when exceeded.
- **Lighter:** API key per account; permissions verified at card start.
- All secrets live in the vault ([PLAN-AUTO-EXECUTION.md](PLAN-AUTO-EXECUTION.md) §2.1) on the machine that trades. Disk encryption (BitLocker) on that machine is recommended.

## 9. Architecture

- `ExecutionVenue` trait from Automation card 2; adapters in `modules/algo/crate/src/venues/` (`binance.rs`, `bybit.rs`, `hyperliquid.rs`, `jupiter.rs`, `lighter.rs`), Tauri-free and unit-tested like `broker.rs` today.
- Hyperliquid: EIP-712 signing of L1 actions (msgpack action hash, agent signature), the `info` endpoint (meta, spotMeta, balances, positions, orders, fills) and the `exchange` endpoint (order, cancel, update leverage). Nonce manager per API wallet. Known-answer signature tests against the official SDK's outputs.
- Jupiter: Swap V2 quote and transaction, local ed25519 signing, an RPC URL the user sets (public fallback with its rate limits), token list by mint address.
- Lighter: its signer and REST API, chosen at card start.
- Python strategies do not change: bots send orders through the runner; routing is in Rust.
- Hyperliquid candles can also be a data source (optional, same adapter).
- MCP: listing venues and coverage are `read`; anything that trades is `external`.

## 10. UI

- **Manual system → Execution:** venue dropdown (connected accounts only, like `AlgoExchangeSelect`), Spot | Perp (Perp shows leverage and the funding estimate), coverage badge ("17 / 20 coins tradable" with the list), cost preview per switch (with the route it would take, direct or through USD) and per 30 days of holding, from live fee schedules and quotes.
- **Bot create modal:** "Data" (exchange + pair) and "Execution" (venue + account + market type) as two groups; for a CEX the default is "same as data".
- **Exchange page (`/algo/exchange`):** add DEX accounts (wallet kinds), test (balance read), API-wallet expiry, sub-account, exposure per venue.

### 10.1 Built (card Venues 1/5, 2026-11-04)

- `modules/algo/crate/src/execution/mapping.rs`: `map_coins` maps data coins onto a venue's spot instruments against its quote (USDT on CEX, USDC on the DEX venues), by verified id. A CEX's asset code is the id (`ExchangeCodes`). An on-chain venue maps only through its `VerifiedList` (token index or mint, plus the custodian), and a look-alike named like the coin is rejected. Direct pairs are listed only between mapped coins. `preview` gives the §5.1 numbers: both switch routes from the live books and the fee tier, plus holding cost (0 on spot, funding on perp).
- Commands, all read: `venue_list` (accounts and what they execute), `venue_mapping` (the table above), `venue_cost_preview` (live books, and the account's fee tier when its keys can read it, else the venue's standard rate, labelled as such), `venue_exposure` (what every bot and Manual system holds per account). MCP: `list_venues`, `venue_coverage`.
- UI: the Manual Execution page gets a cost preview (both routes and the one the engine takes, plus 30 days of holding), and its market select shows Perp as "with the perp venues". The bot form has "Data" and "Execution" groups; a live bot can send its orders to another connected Binance or Bybit account (`execution_exchange_id` in the bot's config, used by the preflight and the live connection), and the default is "same as data". The Exchange page shows exposure per venue, the Trading overview a compact version.
- The coverage check per system is Automation 3/5's "Check coverage". Its restrict policy is card 3b.
- Live values (2026-11-04): a Top-10 universe is 10/10 tradable on Binance spot (17 direct pairs among them) and on Bybit spot (3). 10,000 $ BTC→ETH at the standard rate costs 20.01 $ through USDT or 11.67 $ direct on Binance, and 21.91 $ or 14.76 $ on Bybit; direct wins on both. Run with `--ignored live_venue_values`.
- Not yet: wallet-kind accounts on the Exchange page and perp markets, which come with the DEX venue cards.

### 10.2 Built (card Venues 2/5 Hyperliquid, 2026-11-04)

- `modules/algo/crate/src/venues/hyperliquid.rs` (Tauri-free): `HyperliquidVenue` implements `ExecutionVenue` for spot and perp. Signing is own code with `k256` + `sha3` (msgpack of the action, nonce, vault flag → keccak → EIP-712 `Agent{source, connectionId}` in the `Exchange` domain, chain 1337). The official Rust SDK was not used: no release since May 2025, built on the deprecated ethers-rs. 16 signatures from hyperliquid-python-sdk 0.24.0 (order IOC spot with cloid, reduce-only perp order, cancel by cloid, 1x isolated leverage; mainnet/testnet; with and without a sub-account) plus the SDK's own test vectors are known-answer tests and match.
- Nonces: milliseconds, strictly rising per API wallet in the process. Client order ids: `0x7173` ("qs") + 14 bytes of keccak of our id, so a resumed run finds its order by `orderStatus` and the kill switch only cancels ours.
- Instruments speak the data source's terms: spot `BTC/USDC` is UBTC (pair `@142`, asset 10142) mapped by **token id** per network (mainnet: BTC, ETH, SOL, HYPE, DOGE, AVAX, ENA, PUMP, FARTCOIN, BONK, WLD, XPL, MON, ZEC, VIRTUAL; testnet: ETH, SOL, HYPE, FARTCOIN, PUMP, ENA); a token merely named like a coin is rejected. Perp contracts map by the protocol's own name. Prices are rounded to 5 significant figures inside the cap (integers always pass); IOC limit at the slippage cap; fills and fees come from `userFillsByTime` (a spot buy pays its fee in the bought token, netted from what arrives).
- Perp in this card: 1x isolated (set by `updateLeverage` before the first opening order of a coin) and long-only — a sell on perp is always reduce-only. Leverage settings, shorts, the liquidation guard and funding rows are Venues 4/5.
- Accounts: a DEX account "Hyperliquid" stores the API wallet's key, the main address and an optional sub-account (`vaultAddress`) in the vault; a Testnet switch points everything (prices, instruments, orders) at the testnet. The connection test checks that the main address approved the API wallet (`extraAgents`) and reads both USDC balances; the Exchange list shows the API wallet's expiry and warns 7 days ahead. Moving USDC between spot and perp stays in Hyperliquid's app (user-signed).
- One factory (`venues::open` / `venues::public`) now serves Manual execution, the kill switch, coverage and the cost preview. Manual systems can pick a Hyperliquid account and Spot or Perp; the cost preview on perp adds funding at the live hourly rate × 8 per 8 hours. On any test network the price check compares with the venue's own mid (testnet prices are not market prices).
- Bots still execute on Binance and Bybit only.
- Live values (2026-11-04, `--ignored hyperliquid_live`): 14 verified spot pairs and 179 perps on mainnet, 6 and 159 on testnet; BTC funding −0.0021 %/h.

### 10.3 Built (card Venues 3/5 Jupiter, 2026-11-04)

- **Jupiter vs CoW, re-checked first.** Over two years the Top-5/Top-10 universes held 11 coins. A 10,000 USDC buy with under 0.5 % price impact: Jupiter 7 (BTC, ETH, SOL, TRX, HYPE, ZEC, DOGE; XRP 0.59 %), CoW Protocol on Base 5 (BTC, ETH, SOL, XRP, LINK; DOGE 1.2 %, ADA 2.2 %). CoW does not cover materially more → **Jupiter**. Today's Top 5 (BTC, ETH, BNB, XRP, SOL): Jupiter 3 with depth plus XRP borderline, BNB thin (1.1 %).
- `modules/algo/crate/src/venues/jupiter.rs` (Tauri-free), Swap API V2 (`api.jup.ag/swap/v2`, keyless at 0.5 requests/s, an optional free API key raises it): `/order` with the wallet as taker → the engine's limit becomes the minimum output (`slippageBps` ≤ 50) and the price impact is capped at 0.5 % → the signature goes into the fee payer's slot (legacy and v0; a transaction whose fee payer is not the wallet is refused) → the signature, which is the transaction id, is journaled (`algo/execution/jupiter-swaps.jsonl`) → `simulateTransaction` (sigVerify) on the RPC → `/execute`, which lands it with Jupiter's priority-fee strategy and confirms it. An order older than 10 s is fetched again. RFQ routes (`jupiterz`) are excluded: there the market maker is the fee payer and the transaction id would not be ours. A lost answer is looked up by the journaled signature (`getSignatureStatuses`, `getTransaction` deltas, expiry by `lastValidBlockHeight`), never sent again.
- Verified mints (one per coin, with the custodian): SOL (native), BTC = cbBTC (Coinbase), ETH and BNB (Wormhole Portal), XRP = wXRP (Hex Trust), TRX (TRON DAO); USDC (Circle) is the quote. Not mapped although Jupiter lists them: HYPE, ZEC, DOGE (issuer not confirmed), ADA, LINK (no depth). A token merely named like a coin is rejected.
- Fills from `/execute`'s wallet amounts: Jupiter's fee in its mint (10 bps on 2026-11-04 with RFQ excluded), the network fee (signature + priority + rent) valued in USDC on top. 0.02 SOL always stays on the wallet for fees. Jupiter has no candles: freshness and reference prices come from Binance.
- Accounts: DEX → Jupiter. QuantSuite generates the ed25519 keypair (ring) into the vault and shows the address; there is no import. Settings: max value on this wallet (arming is refused without it or above it, valued with Jupiter's price API), RPC URL (blank: the public RPC, rate-limited), Jupiter API key. The key can be exported after typing the wallet address. The Exchange list shows the wallet, its value against the cap and a SOL-for-fees warning.
- Known answers: transaction signing (v0 and legacy) against solders 0.29 (`venues/jupiter_kat.json`). Live, public: quotes and prices (`--ignored jupiter_live`).
- The canary (one switch there and back with a small amount, fills and fees against the explorer) is the user's (Automation 5 step 3).

### 10.4 Built (card Venues 4/5 perp safety, 2026-11-04)

- **Execution core on perp**: a run's target can be `short` besides `hold` and `flat`. On a perp venue the engine closes every position that is not the target with **reduce-only** orders, then opens the target long or short on the owner's equity × leverage. The books carry a short as a negative quantity, and its proceeds as cash: PnL = entry − exit. Journal rows are `long`, `short` (closed with entry − exit) and `funding`. Paper mode shorts on perp books too. A short on a spot venue fails the run. Spot stays long-only. (Card 2's perp venue was refused by the engine's spot-only check; fixed here.)
- **Leverage**: isolated margin, 1x default. Manual systems (Execution → Market Perp → Leverage) and bots (Execution → Hyperliquid → Perp → leverage) are capped by QuantAlgo Settings → Perp → Leverage cap (default 3x, at most 20x). Spot is always 1x. Hyperliquid's `updateLeverage` sets the leverage before the first opening order of a coin.
- **Liquidation guard**: before a position opens, its distance to liquidation (1 / leverage − maintenance margin; Hyperliquid's maintenance margin is half the initial margin at max leverage) must be at least Settings → Min. liquidation distance (default 20 %). Below it, the run stops with `liquidation_distance` and sends nothing, and a bot does not start. The run notes give the estimated liquidation price. Open positions show the venue's own liquidation price and distance from the mark on the Trading overview, the Bots page and the system's Execution view; below the limit they are marked.
- **Funding in the journal**: every 30 minutes `userFunding` of each Hyperliquid account with perp holdings is read. Each payment is split over the owners holding the coin on its side (Manual books, running live perp bots) and booked once per (account, hour, coin, owner) as a journal row with side `funding`. It also goes into the owner's cash (the system's books, the bot's balance), so equity and the next run's sizing include it. Stats count funding in the PnL and show it apart, never as a trade.
- **Bots that reverse**: a live bot can send its orders to a Hyperliquid account as perp. The live broker runs over any execution venue: `BTC/USDT` signals trade `BTC/USDC`, a sell opens a short, a buy covers it reduce-only, `reverse` does both. A partly covered short keeps its open rest. Margin is booked like paper (the notional set aside). Spot bots stay long-only.
- Tests: short open → cover → long → flat through the core with reduce-only checks on a mock perp venue, leverage sizing, the liquidation guard, spot refusing a short, paper shorts, funding split and booking (idempotent, journal + books, bots), stats, a live perp bot shorting and reversing.
- The user's testnet check (ladder step 2b): a 1x short and a 1x long for a single-asset system, funding rows after an hour.

### 10.5 Built (card Venues 5/5 Lighter, 2026-11-04)

- **Lighter vs dYdX v4, re-checked first.** Both list every coin of the systems' universes as perp. Liquidity is not close: BTC open interest 353 M$ on Lighter vs 16 M$ on dYdX, 24 h volume 1.4 B$ vs 8 M$, and ETH, SOL, HYPE and ZEC similar. Lighter standard accounts pay no trading fee, and Lighter also has a few spot pairs (ETH, LINK, AAVE, UNI, LDO …). Key permissions: a Lighter API key signs orders, cancels, leverage, and withdrawals **only to the account's own L1 address**; transfers to anyone else need the L1 wallet's key. dYdX's permissioned keys would be stricter, but its books are too thin for the systems → **Lighter**.
- `modules/algo/crate/src/venues/lighter.rs` (Tauri-free): transactions are Goldilocks field elements hashed with Poseidon2 and signed with Schnorr over ECgFp5 (`goldilocks-crypto` + `poseidon-hash`, MIT/Apache; Lighter's own signer is a Go DLL). Known answers from the official signer (lighter-sdk 1.1.8): the public key of a generated key, the hashes of two IOC orders (one reduce-only), a cancel and a 1x isolated leverage update, and the official signatures, all of which our verifier accepts; the auth token too. Schnorr is randomized, so our signatures are checked by verification. Every signature is verified before it is sent.
- IOC limit at the slippage cap (prices and sizes in the market's integer units, a sell limit rounded up), 48-bit client order indexes `0x71…` derived from our id (so a resumed run finds its order in `accountActiveOrders` / `accountInactiveOrders`, and the kill switch only cancels ours), nonces from `nextNonce` then increasing per API key, an auth token for private reads (1 h). Perp and spot are separate market types. Positions carry the venue's liquidation price. The maintenance margin comes from each market's details (BTC 1.2 %). Funding (`positionFunding`) is booked like Hyperliquid's. Daily candles come from the venue.
- Accounts: DEX → Lighter with the account index, the API key index (2–254) and the key's private key, plus a Testnet switch. The connection test reads the balance and makes a private read with the key's token. Manual systems and bots pick it like a Hyperliquid account. A system moves between the two by changing only its Execution account. Venues 4/5's perp rules apply unchanged.
- Live (2026-11-04, `--ignored lighter_live`): 214 perps and 11 spot pairs on mainnet; 3 perps and 2 spot on testnet.

## 11. Cards

| Card | What | Blocked by |
|---|---|---|
| QuantAlgo Venues 1/5 | Data vs execution split, venue selection for Manual and Automated, mapping, coverage, cost preview | Automation 2, 3 |
| QuantAlgo Venues 2/5 | Hyperliquid adapter, spot + perp | Automation 1, 2 |
| QuantAlgo Venues 3/5 | Jupiter (Solana) spot adapter, generated trading wallet | Automation 1, 2 |
| QuantAlgo Venues 4/5 | Perp safety: leverage cap, liquidation guard, funding in the journal, shorts | Venues 2 |
| QuantAlgo Venues 5/5 | Lighter adapter (redundancy venue) | Venues 4 |

## 12. Decisions taken (change them on the card if you disagree)

| Decision | Default | Why |
|---|---|---|
| First DEX | Hyperliquid | Spot + perp in one adapter, deepest books, API wallets cannot withdraw |
| Spot DEX for wider coverage | Jupiter; CoW Protocol if coverage is better | Widest single-chain token set, cheap transactions |
| Redundancy perp DEX | Lighter (re-checked 2026-11-04: 10–100× dYdX's liquidity, keys withdraw only to the own L1 address) | Ethereum-settled order book |
| Manual market type | Spot | Funding over multi-day holds (§5.1) |
| Leverage | 1x isolated, cap 3x | A rotation system has no reason for leverage |

## Sources (checked 2026-11-03)

- Hyperliquid fees: https://hyperliquid.gitbook.io/hyperliquid-docs/trading/fees (perp 0.045% / 0.015%, spot 0.070% / 0.040% at the entry tier)
- Hyperliquid funding: https://hyperliquid.gitbook.io/hyperliquid-docs/trading/funding (0.01% per 8 h interest part, hourly, cap 0.5% per hour)
- Hyperliquid API wallets: https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/nonces-and-api-wallets ; user-signed actions (withdraw, transfer) need the main key: https://docs.chainstack.com/docs/hyperliquid-user-signed-actions
- Jupiter Swap V2 replaces Ultra: https://developers.jup.ag/docs/ultra/fees , https://developers.jup.ag/changelog/2026-03
- Unit / UBTC guardians: https://pm.wiki/de/projects/hyperunit
- Perp DEX landscape: https://coinbureau.com/analysis/dex-platforms-for-advanced-traders , https://perpdexrank.com/
- Lighter: https://hummingbot.org/exchanges/lighter/ , https://l2beat.com/layer2s/projects/lighter
- CoW Protocol: https://defillama.com/protocol/cowswap
