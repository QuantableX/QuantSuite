# QuantAlgo trading automation — Manual and Automated trade by themselves

Status: plan, 2026-11-03. Built so far: card 1, the secret vault; card 2, the execution core; card 3, Manual systems trading by themselves; card 4, unattended bots (2026-11-04). Cards "QuantAlgo Automation 1/5" to "5/5" on the QuantSuite board. Where orders go is [PLAN-EXECUTION-VENUES.md](PLAN-EXECUTION-VENUES.md); running it 24/7 is [PLAN-SERVER-CLIENT.md](PLAN-SERVER-CLIENT.md).

## In short

- **Today:** Manual (the QuantSystems systems such as LCES) only *tells* you what to hold. You trade by hand after the daily candle closes at 00:00 UTC, which is 01:00 in winter and 02:00 in summer in Germany. Automated (bots) can trade, but the live path exists only for Binance spot and Bybit spot, and it has only been tested in paper.
- **Goal:** both modes place their own trades, on the exchange you choose, with hard safety limits and a full record of every decision. You sleep; in the morning QuantSuite shows what it did.
- **How it gets safe:** paper first, then the exchanges' test networks, then a small live amount, then the real amount. QuantSuite never withdraws money, and stopping never sells.
- **One weakness to fix first:** the exchange API keys are encrypted with a key anyone can recompute from the public source code. Card 1 moves them into a real vault before any more live trading is built.

## 1. What the code does today

| Area | Where | What it does |
|---|---|---|
| Manual systems | `modules/systems` (shown as `/algo/manual`), Python `rotation_lab.rpc` | `live` returns `best`: the coin to hold, or `USD` (rotation mode); single-asset mode returns `position` long / short / cash. Nothing places orders. |
| Automated bots | `modules/algo/crate/src/bot.rs` | One `python -m quantalgo.runner` per bot. A feed thread polls the exchange's public candles and calls `on_candle`; the strategy answers with orders. |
| Fill engine | `runtime.rs` | Tauri-free; turns orders into `Effect`s. Paper uses the fee and slippage settings; live books what the venue reports. |
| Live routing | `broker.rs` (`Broker` trait) | Binance spot and Bybit spot only, long-only, market orders. `exchanges.sandbox` routes private calls to Binance spot testnet / Bybit demo. |
| Restart | `lib.rs` | A bot that was running when the suite exited is marked `stopped` ("suite restarted"). Bots are never restarted on their own. |
| API keys | `exchanges.rs` | AES-256-GCM in `quantalgo.db`, but the key is `SHA-256("quantalgo:v2:<USERNAME>:<data dir>")`, with a fallback to a constant legacy key written in the source. The source is public, the username and data dir are easy to guess, so a copy of `quantalgo.db` is enough to read the keys. The key also changes with the username or folder, so the keys cannot move to another machine (see [PLAN-MIGRATION.md](PLAN-MIGRATION.md)). `docs/ARCHITECTURE.md` §5 "Secrets" planned the OS keychain; it never happened for QuantAlgo. |
| Scheduling | — | There is no scheduler for Manual. QuantPilot's scheduled prompts (card "QuantPilot round 3 03/07") are the first timer of this kind in the suite. |
| Notifications | — | No OS notifications yet (no notification plugin). |

## 2. The design

### 2.1 Secret vault (card 1)

One vault for every secret the suite keeps: exchange API keys, wallet keys (venues), the server's identity key and paired devices (server), and the migration passphrase step.

- A random 256-bit master key, stored in the OS keystore through the `keyring` crate: Windows Credential Manager (DPAPI), macOS Keychain, Linux Secret Service. Where no keystore exists (a headless Linux server later), a passphrase-protected key file (Argon2id).
- Each secret: AES-256-GCM under the master key, with the secret's id as associated data, so a ciphertext cannot be swapped onto another row.
- Write-only: the frontend can set, test, replace and delete a secret, and sees a masked fingerprint (`…7f3a`). It never receives the value back.
- One-time migration on first start: every `config_encrypted` row is decrypted with today's derivation (or the legacy key), re-encrypted into the vault and verified, then the old column is cleared. After that the legacy key is removed from the code.
- Tests: round trip, wrong associated data fails, migration of rows written with both old keys, a missing keystore falls back to the passphrase file.

**Built (card 1, 2026-11-04).** `crates/qs-core/src/vault.rs` + `modules/algo/crate/src/secrets.rs`:

- Files: `~/.quantsuite/secrets/vault.db` (ciphertexts, fingerprints, a random vault id and a key check) and, only without a keystore, `secrets/vault.key` (the master key wrapped under Argon2id, 64 MiB / 3 passes). The keystore entry is service `QuantSuite Vault`, account `master-key:<vault id>`. A copy of `vault.db` alone decrypts nothing. An isolated profile (`QUANTSUITE_HOME`) gets its own vault.
- No keystore: the vault is created only with a passphrase (at least 12 characters) from `QUANTSUITE_VAULT_PASSPHRASE` or the exchange page's "Open vault" field (`plugin:qs|vault_unlock`). It never falls back silently. A keystore vault whose key is missing refuses to open while it holds secrets. An empty one starts over.
- Exchange keys: id `algo/exchange/<exchange id>`, the JSON of the form fields. `add_exchange` stores into the vault, or the exchange is not added. `update_exchange` keeps the stored value for every blank field (the old code wiped the keys on a rename). `delete_exchange` removes the entry. `list_exchanges` returns `key_storage` (`vault` / `none` / `unreadable` / `locked`) and the fingerprint, never a key. The page shows "Keys in vault …7f3a".
- Migration: on start (only when a row still holds old keys), after an import and when a row is first read. It decrypts with today's derivation or the old constant, stores, reads back, compares, then sets `config_encrypted = NULL` with `secure_delete` on and checkpoints the WAL (TRUNCATE), so the old ciphertext is gone from the file. A row neither key opens stays and shows "Saved keys unreadable, enter them again". Saving new keys clears it.
- The two old keys survive only in `secrets::legacy`, decrypt-only, to read rows written before the vault. Delete that module once every installation has started this version once.
- Data export no longer carries keys; importing an old export moves its keys into the vault and prunes vault entries of exchanges the import dropped.
- `ExchangeConfig` (the only IPC struct with typed-in keys) has a redacting `Debug`; `StoredCredentials` and `ExchangeSecret` wipe themselves on drop.
- Real-data check (2026-11-04, a copy in `artifacts/vault-migration-20261104`): 1 of 1 row migrated, the column cleared, the old ciphertext no longer in the file. Bybit's read-only credential test reached the account but answered "Your api key has expired". The key itself has to be renewed at Bybit.

### 2.2 Execution core (card 2)

One engine for Manual and Automated. Input: a **target** ("hold 100% ETH", "flat", "long BTC") and an **account**. It makes the target true on the venue and records everything.

1. **Lock:** one run per account at a time.
2. **Read the venue:** balances, positions, open orders.
3. **Plan:** the difference between now and the target, inside the system's own budget (2.4). A switch takes the cheaper route: a direct pair, or sell to the account's quote asset and buy (USDT on Binance/Bybit, USDC on DEXs) — see "Route choice" below. Through the quote asset it sells first, then buys. Round to the lot step; a leg below the venue's minimum stays as dust and is logged.
4. **Guards** — every one must pass, or the run stops with the reason:
   - data freshness (the signal is from the candle that just closed);
   - wrong coin, buys only: the venue's price is within 3 times of the signal's reference price (a wrong instrument mapping or a broken book; no market moves that far);
   - max order value and daily turnover cap (set by the user);
   - the global kill switch is off;
   - the account belongs to this system;
   - live is armed (2.3).
5. **Execute:** each order carries a client order id made from the run id and the leg. After a crash or restart the engine looks the id up on the venue before it sends anything again, so an order is never sent twice. Orders have no price limit (2.6): a market order where the venue has one, otherwise an immediate-or-cancel order with a far worst price.
6. **Confirm:** fills are read back from the venue. A partial fill retries the rest up to three times, then stops and reports; the scheduler tries a partial or failed run again every 5 minutes while its window is open.
7. **Book:** journal rows (`trades` gains a source: bot id or system run id), fees in the asset the venue charged, equity.
8. **Report:** a run record with the signal snapshot, plan, guard results, orders, fills, errors and timings, plus a notification (2.5).

`Broker` becomes `ExecutionVenue`: capabilities (spot, perp, IOC limit, client order ids, sub-accounts), instruments, balances, positions, place / cancel / query by client id, fee schedule. Binance spot and Bybit spot are ported with unchanged behaviour; today's broker tests stay green. Hyperliquid, Jupiter and Lighter plug in later ([PLAN-EXECUTION-VENUES.md](PLAN-EXECUTION-VENUES.md)).

**Route choice for a switch** (user decision 2026-11-03: take the cheaper way, not always through USD).

| Route | Trades | When it exists |
|---|---|---|
| Through the quote asset: sell X → USD, buy Y | 2 | Always |
| Direct pair X/Y (or Y/X) | 1 | Only where the venue lists the pair — on a CEX for some coins, mostly against BTC, ETH or BNB; not on Hyperliquid spot or any perp |
| One aggregator swap (Jupiter) | 1 transaction | Always on Jupiter; its quote already is its best route, so there is nothing to choose |

- For exactly this switch size, the engine estimates both routes: the fee per leg from the venue's fee schedule (the account's tier), plus spread and slippage from walking the live order book for that size — not just the top price.
- The direct pair is taken only when it is cheaper by at least 0.02% of the switch value; otherwise, and whenever the estimate is unsure, it goes through USD.
- The direct order passes the same guards: its price is checked against the cross price of the signal's references (X in USD ÷ Y in USD), plus max slippage and the venue's minimums.
- If the direct order is rejected or only partly filled, the rest goes through USD.
- The run record keeps the chosen route, both estimates and the actual cost.
- Paper takes the same decision (the PaperVenue walks the real book).
- Perp switches are always close + open; Hyperliquid spot quotes everything in USDC. The check still runs there, so a direct pair a venue adds later is used without a code change.

Example, 10,000$ BTC → ETH on Binance spot: through USDT 0.10% × 2 = 20$; direct on ETH/BTC 0.10% = 10$ plus a very small spread → direct. For a thin cross pair the spread and slippage can cost more than the 10$ saved → through USD.

The backtest stays conservative: it charges two legs per switch (`rotation_lab` `_switch_legs`, `fee_rate` 0.1% per side). A cheaper live route can only make live better than the backtest, never worse.

**Paper** uses the same core with a `PaperVenue`: fills at the real venue's current best bid/ask plus the configured fee and slippage, balances persisted per system. Paper and live share every step except the final API call.

**Built (card 2, 2026-11-04).** `modules/algo/crate/src/execution/`:

- `venue.rs`: the `ExecutionVenue` trait (capabilities, instruments, balances, positions, order book, fee tier, IOC place, cancel, lookup by client id, dry run) and the book walk.
- `cex.rs`: Binance spot and Bybit spot. Orders are IOC limits with `newClientOrderId` / `orderLinkId`. A lookup by client id answers "never seen" on Binance -2013 or an empty Bybit list. Fee tiers come from `account/commission` and `v5/account/fee-rate`, with 0.1 % when the venue does not answer. All instruments are cached 15 minutes.
- `route.rs`, `guards.rs`, `engine.rs`, `store.rs`, `paper.rs`: as specified above. Tables `execution_runs` (with a unique index on owner + close date for Manual's exactly-once), `execution_orders` (written before sending), `execution_holdings`, `execution_flags` (kill switch), `paper_balances` and `paper_orders`. `trades` gains `source` (`bot:<id>` / `run:<id>`), `fees_json` (fees in the charged assets) and `mode`.
- Resuming is re-planning: holdings are rebuilt from the run's recorded fills. An order whose answer was lost is looked up by its client id, and an order the venue never saw is planned again under a new id.
- A leg whose last order filled completely is done. What remains of it (lot rounding, the 0.1 % sizing room of a buy) stays as dust in the run record.
- Bots: `LiveBroker` keeps the runtime's interface, but its orders now go through `CexVenue` the core's way (client id, lookup, three retries; no price limit, see 2.6). Paper bots keep the runtime's simulation. The bots' run-level guards and the kill switch come with card 4. A sell or cover that fills only partly closes what filled: the rest stays a position with its share of the entry fee and margin and its own journal row. Only a rest the venue would not take an order for (lot step, minimum quantity, minimum order value) is dust and stays on the exchange. Exits have no price limit, see "A bot acts on candle close" in 2.6.
- Tests: 24 in `execution` (route cases incl. the 10 $ vs 20 $ example, every guard, crash between legs, an order that never arrived, partial-fill retries, paper fills and persistence) plus the bot tests unchanged. Live public checks (books, instruments of both venues) are `--ignored live_`.

### 2.3 Manual systems trade by themselves (card 3)

A new **Execution** section in every Manual system's settings:

| Setting | Values | Default |
|---|---|---|
| Mode | Off / Paper / Live | Off |
| Venue, account, market type | from [PLAN-EXECUTION-VENUES.md](PLAN-EXECUTION-VENUES.md) | Spot |
| Capital | fixed amount / % of the account's quote balance / the whole (sub-)account | — |
| Run time | after the daily close + delay | 00:15 UTC |
| Retry window | until N hours after the close | 6 h |
| If the suite was off at run time | run once if still inside the retry window, else skip | — |

**Timing.** The engine works in UTC, so summer time does not matter: the daily candle closes at 00:00 UTC, the run starts at 00:15 UTC (01:15 winter / 02:15 summer in Germany). The delay gives the exchanges and the daily Top-N ranking time to publish.

**Data freshness.** The run uses the snapshot as of the candle that just closed. It checks that this candle exists for every coin of the day's universe and that the day's ranking exists. If not, it retries every 5 minutes until the retry window ends, then skips and reports. It never trades on yesterday's data.

**The decision.** Rotation: `best` (a coin or USD). Single asset: long or cash on spot; short only on a perp market ([PLAN-EXECUTION-VENUES.md](PLAN-EXECUTION-VENUES.md) §6).

**A coin the venue does not list.** Before Live can be armed, the coverage check shows which coins of the universe the venue can trade. If one is missing, the user picks:
- (a) restrict the universe to tradable coins — the backtest honours the same restriction, so backtest = live (recommended);
- (b) hold USD when the pick is not tradable — live-only behaviour, flagged "backtest ≠ live".

**Exactly once.** One run per (system, close date), kept in a runs table; a restart in the middle resumes through the client order ids (2.2) instead of starting over.

**Scheduler.** In-process, persisted, it sleeps until the next due time (no per-second polling) — the same design as QuantPilot's scheduled prompts. It runs only while QuantSuite runs: on this PC the PC must be on and awake at 00:15 UTC. On the server ([PLAN-SERVER-CLIENT.md](PLAN-SERVER-CLIENT.md)) it runs 24/7.

**Arming Live** needs all of:
1. at least one completed paper run of this system;
2. a successful account test on the venue (balance read, a test order where the venue offers one);
3. the system's robustness verdict ([ROBUSTNESS.md](ROBUSTNESS.md), card "Robustness 4/5"). A missing or failing verdict is shown, and Live then needs the user's own tick "I know this system is not robust" — the same rule as the save gate there.

**Runs tab** per system: date, decision, orders, fills, cost, status, and achieved vs Live Eval's expected result.

**Built (card 3, 2026-11-04).** `modules/algo/crate/src/manual.rs` and the page `/algo/manual/:id/execution` (view "Execution" next to Settings, Live and Backtest):

- Settings live in `quantalgo.db` (`manual_execution`), never in the system's `config.json`. They hold the mode Off / Paper / Live, the account (Binance or Bybit), Spot, the capital (fixed / % of the USDT balance / the whole account), the run time (UTC), the retry window, the optional max order value and daily turnover cap, and the paper fee and slippage. Changing mode, account, market or capital disarms Live.
- Scheduler: one thread that sleeps until the next due time or a settings change. A day's run is due from the run time until the window ends. A late start runs once inside the window, and after it the day is skipped. The run id is `manual-<system>-<close date>`, and the unique index on (owner, close date) makes a second run impossible.
- Freshness without touching `rotation_lab`: the engine's `live` result gives the pick and the scored coins. The run then reads the closed daily candle of the day before from the account's venue (Binance when the venue does not list the coin). Every scored coin must have that candle, or the run waits and retries every 5 minutes until the window ends, then records a skip. The same closes are the reference prices of the price-sanity guard.
- Coverage: "Check coverage" lists today's universe against the venue's USDT pairs. A pick the venue cannot trade is held as USD and flagged "backtest ≠ live". Option (a), restricting the universe so the backtest honours it, needs a `rotation_lab` change and is card 3b (not merged while the Gauntlet run uses `sidecars/python`).
- Arming checks: Live is saved, a completed paper run exists, the account reads its balance, and the newest System Gauntlet verdict is robust or the user ticked "I know this system is not robust".
- After each run: an `algo:manual-run` event, a line in the bot log and a Windows toast through `tauri-winrt-notification` (the same wrapper QuantPilot uses), e.g. "LCES: BTC → ETH, 2 orders, fees 13.98$".
- MCP: `manual_runs` (read), `manual_arm` and `manual_disarm` (external).
- Checks: unit tests for the due times and window, exactly-once, freshness and skip, the target and coverage policy, the arming rule and the verdict file reading. A paper run against Binance's live book: 1,000 $ bought BTC, and the switch to ETH took ETH/BTC directly (1.17 $ against 2.00 $). Run it with `--ignored live_paper`.
- Not run by the agent: the scheduler firing inside the app, the toast, and a restart between snapshot and orders in the app. These belong to the user's paper nights, step 1 of card 5.

### 2.4 Budget: what a system may touch

A system only trades what it owns: the capital assigned to it and the coins it bought with it. Other holdings on the same account are never sold. The clean setup — and the recommendation in the UI — is **one sub-account per system** (Binance and Bybit sub-accounts, Hyperliquid sub-accounts, or a separate wallet). Then "the whole account" is simply right, and one system's mistake cannot touch another's money.

### 2.5 Notifications

In-app (Trading overview, the system's Runs tab) and Windows notifications (`tauri-plugin-notification`) for: trade done, run skipped, guard stopped a run, error. One line each, e.g. "LCES: BTC → ETH, 2 orders, fees 13.98$".

### 2.6 Automated bots run unattended (card 4)

- **Resume after restart**, opt-in per bot (off by default for live). On start the bot reconciles with the venue: positions and open orders must match its books. A mismatch keeps it stopped with a clear message; it never trades to "fix" it.
- **Health:** no new candle for two bar lengths → pause and notify; three refused entries in a row → pause and notify; a heartbeat per bot on the Trading overview. A refused exit never pauses (below).
- **A bot acts on candle close (user rule, 2026-11-05).** Like Manual and the backtest, the strategy sees finished candles only: within a poll (30 s) of the close it gets the bar that just ended, and its order goes out at the current price, which is about this bar's open. The forming candle is never a signal, at the start neither: a new bot acts from the next close, a resumed bot also on what closed while it was down, and it knows what it holds.
- **Orders follow the signal, for bots and Manual (user rule, 2026-11-05).** "If it drops by a lot, it happens, manual and automated, just follow the system's signal of the close/open, not the current % points." No order has a price limit, and neither the move since the close nor the book's depth stops one: a market order on Binance and Bybit; Hyperliquid and Lighter have none, so an IOC order with its worst price 50 % from the best (20 %, 5 %, then 1 % if the venue refuses that), which fills against the book like one; on Jupiter the swap takes the slippage Jupiter sets. Still in force: the wrong-coin check on buys (2.2), the system being armed, the kill switch, the budget, the venue's minimums, the leverage and liquidation limits on perp, and the order value and turnover limits if the user set them.
- **A bot's exit goes on until the position is closed.** What a thin book leaves over and what the venue refuses is retried on every poll. The third refusal in a row tells the user but does not pause the bot, and neither a stale nor a failed feed stops it while an exit is open. A position marked as closing says so in its journal row, so a resumed bot goes on selling. "Stop all trading" and the stop button still stop the bot, and then the selling, and a position under the venue's minimum order is reported once and left on the exchange. Refused entries still pause after three in a row.
- **Kill switch** "Stop all trading" for bots and Manual execution: cancels open orders and disarms. It never sells (today's rule). "Close positions" stays a separate confirmed action.
- Bots route their live orders through the execution core, so they get every new venue and guard.

**Built (card 4, 2026-11-04).** `modules/algo/crate/src/unattended.rs`:

- "Resume after a restart" is a checkbox in the bot form (`resume_after_restart` in the bot's config, off by default). At start every running bot is marked stopped as before. Five seconds later the bots that asked to resume get their books rebuilt from the journal: the budget, plus what closed since the session began, minus what open positions tie up, with the positions themselves. A live bot is then reconciled with the venue. The venue must hold at least the open base (0.1 % tolerance for lot rounding and base fees), and no QuantSuite order (`qs…` client id) may still rest on it. A mismatch leaves the bot stopped with "not resumed: …" and a toast. A resumed bot keeps its session start.
- Health: every feed tick is a heartbeat (`bots.heartbeat_at`, event `bot:heartbeat`, "· 12s" next to a running bot on the Trading overview). No new candle for two bar lengths pauses the bot, and so do three refused orders in a row (`Effect::Pause`). A pause stops the bot, keeps "paused: …" on its row and sends a toast. It never sells.
- Kill switch: "Stop all trading" on the Trading overview (inline confirm) and the MCP tool `stop_all_trading` (external). It sets the flag every engine run checks, stops every bot, disarms every Manual system and cancels QuantSuite's open orders on the live bots' pairs and the last two days' live Manual pairs. The user's own orders are never touched, nothing is sold, and the report lists what is still held. While it is on, live bots refuse to start. "Allow trading again" clears the flag and re-arms nothing.
- Bots' live orders already went through the execution core's venue since card 2.
- Tests: books rebuilt from the journal, reconcile match and mismatch, the stale-feed rule, the error streak (a fill resets it, a pause never sells), the kill switch cancelling only `qs…` orders and never placing one, and that only bots that asked resume.
- Not run by the agent: the QA in paper with the app (resume across a restart, the kill switch in the UI). That belongs to the user's step 1 of card 5.

### 2.7 Verification ladder (card 5)

| Step | What | Passes when |
|---|---|---|
| Paper | Manual paper runs every night, bots in paper | 7 nights without an error; paper result within fees of Live Eval |
| Test networks | Binance spot testnet, Bybit demo, Hyperliquid testnet | A switch, a partial fill, a rejected order, a restart mid-run and the kill switch all behave as specified |
| Canary | Live, small amount (e.g. 50–100$) per venue | 7 nights; fees and fills match the venue's statements |
| Full | The real amount | User's decision |

The agent never places a live order and never starts a live bot (AGENT.md: `quantsuite.algo.start_bot` is the user's call). The user triggers each step; the agent reads the run records and logs and reports.

## 3. Safety rules (always true)

- Everything starts **Off**. Live is armed per system and per bot, by the user.
- QuantSuite never withdraws and never asks for withdrawal rights. Exchange keys: trade permission only, withdrawals disabled, IP whitelist set to the executing machine (the UI says how per exchange).
- Stopping never sells.
- Every decision is logged and the logs are kept.
- Limits are enforced in code, not in prompts.
- MCP: any tool that arms, trades or closes is `external` — never auto-approved.

## 4. Worked example

LCES, Live on Hyperliquid spot, 10,000$ in its own sub-account. Yesterday it held BTC.

| UTC | What happens |
|---|---|
| 00:00 | Daily candle closes. |
| 00:15 | Run starts. Every coin's candle and today's ranking are there. |
| 00:15 | Decision: ETH. Route: Hyperliquid spot has no UBTC/UETH pair → through USDC. Plan: sell 0.1 UBTC → ≈10,000 USDC, buy ≈9,993$ of UETH. Guards pass. |
| 00:16 | Both orders filled. Fees 7.00$ + 6.99$. Notification: "LCES: BTC → ETH, 2 orders, fees 13.99$". |

The same night with missing data:

| UTC | What happens |
|---|---|
| 00:15 | SOL's candle is missing on the data source. Retry every 5 minutes. |
| 03:05 | Still missing … |
| 06:00 | Retry window over. Skip. Notification: "LCES: skipped — no SOL candle for 2026-11-10. Still holding BTC." |

## 5. Cards

| Card | What | Blocked by |
|---|---|---|
| QuantAlgo Automation 1/5 | Secret vault, migration of today's keys | — |
| QuantAlgo Automation 2/5 | Execution core, `ExecutionVenue`, PaperVenue, CEX ported | 1 |
| QuantAlgo Automation 3/5 | Manual systems execute (settings, scheduler, runs, notifications) | 2 |
| QuantAlgo Automation 4/5 | Automated bots unattended (resume, health, kill switch) | 2 |
| QuantAlgo Automation 5/5 | Verification ladder with the user | 3, 4 |

## 6. Decisions taken (change them on the card if you disagree)

| Decision | Default | Why |
|---|---|---|
| Run time | 00:15 UTC, retry until 06:00 UTC | Candles are final at 00:00; the delay covers late data; six hours later the signal is stale. |
| Order type | A market order: "just follow the system's signal of the close/open, not the current % points" (user rule, 2026-11-05) | A system's result depends on following its signal. In a thin book or a fast move it fills far from the price: that happens. |
| Switch route | The cheaper of direct pair and through USD, from fees + the live book for the size; through USD unless direct saves ≥ 0.02% | One trade instead of two saves up to 0.1% per switch on a CEX; a thin cross pair's spread can cost more than that (user decision 2026-11-03). |
| Live arming | Paper run + account test + robustness verdict or explicit acknowledgement | Matches the robustness rule without blocking a system the user already trades by hand. |
| Restart of live bots | Opt-in per bot, reconcile first | A silent restart after a crash must not trade on a wrong state. |

## 7. Options — not planned unless you ask

- "Ask me first": the plan is ready at 00:15 and a one-click notification executes it.
- Push to the phone (Telegram, ntfy) — most useful with the server.
- Wake this PC from sleep at 00:10 UTC with Windows Task Scheduler, until the server exists.
- TWAP or limit-order execution for large amounts.
