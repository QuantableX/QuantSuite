//! The bot's fill engine — Tauri-free (PLAN-QUANTALGO §3.2, §4.3).
//!
//! One `BotRuntime` per running bot holds the cash, the open positions and
//! the mark price, and turns a strategy's order requests into fills. Paper
//! mode simulates the fills here (slippage, fee, one position per pair and
//! side, a position cap, collateral for shorts). Live mode routes the same
//! requests to the bot's [`Broker`] and books what the venue reports back:
//! the average price, the base actually credited, the fee in whatever asset
//! it was charged.
//!
//! The engine never touches Tauri, the database or the runner process: it
//! returns [`Effect`]s, and `bot.rs` applies them — persist and announce a
//! trade, tell the strategy about a fill, log a line, snapshot the equity.
//! That is what makes the bookkeeping unit-testable, live included (the
//! tests drive it with a mock broker).

use serde_json::{json, Value};
use uuid::Uuid;

use crate::broker::{Broker, Fill, SymbolFilters, Venue};
use crate::Trade;

/// Trading mode of a runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Paper,
    Live,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "paper" => Some(Mode::Paper),
            "live" => Some(Mode::Live),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Paper => "paper",
            Mode::Live => "live",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Position {
    pub id: String,
    pub pair: String,
    pub side: String,
    pub entry_price: f64,
    pub quantity: f64,
    /// The entry fee valued in quote. For a live fill charged in base the
    /// quantity is already net of it, and this is its quote value.
    pub entry_fee: f64,
    /// Cash set aside for a short (100 % of the notional) — returned on close.
    pub reserved_margin: f64,
    pub entry_time: String,
    /// The venue's order id of the entry (live bots).
    pub entry_order_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

impl OrderSide {
    fn opens(self) -> &'static str {
        match self {
            OrderSide::Buy => "long",
            OrderSide::Sell => "short",
        }
    }

    fn closes(self) -> &'static str {
        match self {
            OrderSide::Buy => "short",
            OrderSide::Sell => "long",
        }
    }
}

/// What a strategy asked for, decoded from its `buy` / `sell` / `reverse` RPC.
#[derive(Debug, Clone)]
pub struct OrderRequest {
    pub side: OrderSide,
    pub pair: String,
    /// Explicit base quantity; 0 means "size it for me".
    pub quantity: f64,
    /// Fraction of the cash balance to commit when `quantity` is 0.
    pub position_size: Option<f64>,
    /// After closing an opposing position, open the new side too.
    pub reverse: bool,
}

/// What the engine wants done after a request. Applied in order by `bot.rs`.
#[derive(Debug, Clone)]
pub enum Effect {
    Log { level: &'static str, message: String },
    /// Persist and announce this trade row (`bot:trade`). An open position
    /// has no exit; a closed one carries pnl and fees. Boxed: a `Trade` is
    /// ten times the size of the other variants.
    Trade(Box<Trade>),
    /// Tell the strategy about a fill (`on_trade`).
    Notify {
        trade_id: String,
        pair: String,
        side: String,
        price: f64,
        quantity: f64,
        pnl: f64,
        action: &'static str,
    },
    /// The equity changed — snapshot it and announce (`bot:equity`).
    Equity,
}

pub struct BotRuntime {
    pub bot_id: String,
    pub strategy_id: String,
    pub exchange_id: String,
    pub pair: String,
    pub mode: Mode,
    /// Free cash in the quote currency. For a live bot this is the budget
    /// the bot may spend, not the account balance.
    pub balance: f64,
    pub last_price: f64,
    pub open_positions: Vec<Position>,
    pub fee_rate: f64,
    pub slippage_pct: f64,
    pub risk_per_trade: f64,
    pub max_positions: usize,
    /// The venue behind a live bot; `None` for paper.
    pub broker: Option<Box<dyn Broker>>,
    /// The pair's lot and notional limits, fetched once at start (live).
    pub filters: Option<SymbolFilters>,
}

impl std::fmt::Debug for BotRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BotRuntime")
            .field("bot_id", &self.bot_id)
            .field("strategy_id", &self.strategy_id)
            .field("exchange_id", &self.exchange_id)
            .field("pair", &self.pair)
            .field("mode", &self.mode)
            .field("balance", &self.balance)
            .field("last_price", &self.last_price)
            .field("open_positions", &self.open_positions)
            .field("fee_rate", &self.fee_rate)
            .field("slippage_pct", &self.slippage_pct)
            .field("risk_per_trade", &self.risk_per_trade)
            .field("max_positions", &self.max_positions)
            .field("broker", &self.broker.as_ref().map(|b| b.venue()))
            .field("filters", &self.filters)
            .finish()
    }
}

impl BotRuntime {
    /// Decode the runner's order RPC. `reverse` carries `side` = buy|sell
    /// (or long|short); `buy` / `sell` may carry `reverse: true` themselves.
    pub fn parse_order(&self, method: &str, params: &Value) -> Result<OrderRequest, String> {
        let side = if method == "reverse" {
            match params.get("side").and_then(|v| v.as_str()).unwrap_or_default() {
                "buy" | "long" => OrderSide::Buy,
                "sell" | "short" => OrderSide::Sell,
                other => return Err(format!("Invalid reverse side: {other}")),
            }
        } else if method == "buy" {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };
        let reverse = method == "reverse"
            || params.get("reverse").and_then(|v| v.as_bool()).unwrap_or(false);
        let pair = params
            .get("pair")
            .and_then(|v| v.as_str())
            .filter(|p| !p.is_empty())
            .map(|p| p.to_string())
            .unwrap_or_else(|| self.pair.clone());
        let quantity = params.get("quantity").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let position_size = params
            .get("position_size")
            .or_else(|| params.get("sizing_pct"))
            .and_then(|v| v.as_f64())
            .map(|v| if v > 1.0 { v / 100.0 } else { v })
            .filter(|v| *v > 0.0 && *v <= 1.0);
        Ok(OrderRequest { side, pair, quantity, position_size, reverse })
    }

    /// Cash plus the market value of every open position at the mark price.
    pub fn equity(&self) -> f64 {
        self.open_positions.iter().fold(self.balance, |equity, position| {
            if position.side == "long" {
                equity + self.last_price * position.quantity
            } else {
                let unrealized = (position.entry_price - self.last_price) * position.quantity;
                equity + position.reserved_margin + unrealized
            }
        })
    }

    pub fn quote_asset(&self) -> String {
        self.pair.split('/').nth(1).unwrap_or("USDT").to_string()
    }

    pub fn base_asset(&self) -> String {
        self.pair.split('/').next().unwrap_or("").to_string()
    }

    pub fn venue(&self) -> Option<Venue> {
        self.broker.as_ref().map(|b| b.venue())
    }

    pub fn balance_json(&self) -> Value {
        json!({ self.quote_asset(): self.balance })
    }

    pub fn positions_json(&self) -> Value {
        let mut positions = serde_json::Map::new();
        for position in &self.open_positions {
            let unrealized = if position.side == "long" {
                (self.last_price - position.entry_price) * position.quantity
            } else {
                (position.entry_price - self.last_price) * position.quantity
            };
            positions.insert(
                position.pair.clone(),
                json!({
                    "id": position.id,
                    "pair": position.pair,
                    "side": position.side,
                    "entry_price": position.entry_price,
                    "quantity": position.quantity,
                    "entry_fee": position.entry_fee,
                    "reserved_margin": position.reserved_margin,
                    "unrealized_pnl": unrealized,
                    "entry_order_id": position.entry_order_id,
                }),
            );
        }
        Value::Object(positions)
    }

    /// The open positions as journal rows (no exit yet).
    pub fn open_trades(&self) -> Vec<Trade> {
        self.open_positions
            .iter()
            .map(|p| self.trade_row(p, None, None, None, p.entry_fee, None))
            .collect()
    }

    fn trade_row(
        &self,
        position: &Position,
        exit_price: Option<f64>,
        exit_time: Option<String>,
        pnl: Option<f64>,
        fee: f64,
        exit_order_id: Option<&str>,
    ) -> Trade {
        let notes = match (&position.entry_order_id, exit_order_id) {
            (Some(entry), Some(exit)) => Some(format!("live · entry order {entry} · exit order {exit}")),
            (Some(entry), None) => Some(format!("live · entry order {entry}")),
            _ => None,
        };
        Trade {
            id: position.id.clone(),
            bot_id: Some(self.bot_id.clone()),
            trading_mode: Some(self.mode.as_str().to_string()),
            strategy_id: self.strategy_id.clone(),
            exchange: self.exchange_id.clone(),
            pair: position.pair.clone(),
            side: position.side.clone(),
            entry_price: position.entry_price,
            exit_price,
            quantity: position.quantity,
            entry_time: position.entry_time.clone(),
            exit_time,
            pnl,
            pnl_pct: pnl.map(|p| (p / (position.entry_price * position.quantity).max(0.0001)) * 100.0),
            fee,
            is_backtest: false,
            backtest_id: None,
            notes,
            created_at: position.entry_time.clone(),
        }
    }

    /// Fill an order: simulated in paper mode, routed to the broker in live
    /// mode (PLAN-QUANTALGO §3.2, §4.3).
    pub fn place(&mut self, req: OrderRequest, now: &str) -> Vec<Effect> {
        if self.mode == Mode::Live {
            return self.place_live(req, now);
        }
        let mut effects = Vec::new();

        let opening = req.side.opens();
        let opposing = req.side.closes();
        let has_opposing = self
            .open_positions
            .iter()
            .any(|p| p.pair == req.pair && p.side == opposing);
        let has_same = self
            .open_positions
            .iter()
            .any(|p| p.pair == req.pair && p.side == opening);
        if !has_opposing && has_same {
            effects.push(Effect::Log {
                level: "warn",
                message: format!(
                    "One open {opening} position per pair is supported; skipping duplicate {} signal.",
                    req.pair
                ),
            });
            return effects;
        }
        if !has_opposing && self.open_positions.len() >= self.max_positions {
            effects.push(Effect::Log {
                level: "warn",
                message: format!(
                    "Max positions ({}) reached, skipping {} signal.",
                    self.max_positions,
                    if req.side == OrderSide::Buy { "buy" } else { "sell" }
                ),
            });
            return effects;
        }

        // Buys fill a little above the mark, sells a little below.
        let slip = if req.side == OrderSide::Buy {
            1.0 + self.slippage_pct / 100.0
        } else {
            1.0 - self.slippage_pct / 100.0
        };
        let price = (self.last_price * slip).max(0.0001);

        let mut closed_opposing = false;
        if let Some(idx) = self
            .open_positions
            .iter()
            .position(|p| p.pair == req.pair && p.side == opposing)
        {
            self.close_at(idx, price, now, &mut effects);
            closed_opposing = true;
        }

        if closed_opposing && !req.reverse {
            effects.push(Effect::Equity);
            return effects;
        }
        if self.open_positions.len() >= self.max_positions {
            effects.push(Effect::Log {
                level: "warn",
                message: format!(
                    "Max positions ({}) reached after close, skipping {opening} entry.",
                    self.max_positions
                ),
            });
            effects.push(Effect::Equity);
            return effects;
        }

        // Sizing: an explicit quantity wins; else the strategy's fraction of
        // the (post-close) cash; else the risk-per-trade default.
        let quantity = if req.quantity > 0.0 {
            req.quantity
        } else if let Some(fraction) = req.position_size {
            ((self.balance * fraction) / price).max(0.001)
        } else {
            ((self.balance * self.risk_per_trade / 100.0) / price).max(0.001)
        };

        let entry_fee = price * quantity * self.fee_rate;
        let notional = price * quantity;
        let required = notional + entry_fee;
        if self.balance < required {
            effects.push(Effect::Log {
                level: "warn",
                message: if opening == "long" {
                    "Insufficient balance to open long position.".into()
                } else {
                    "Insufficient collateral to open short position.".into()
                },
            });
            effects.push(Effect::Equity);
            return effects;
        }
        self.balance -= required;

        let position = Position {
            id: Uuid::new_v4().to_string(),
            pair: req.pair.clone(),
            side: opening.to_string(),
            entry_price: price,
            quantity,
            entry_fee,
            reserved_margin: if opening == "short" { notional } else { 0.0 },
            entry_time: now.to_string(),
            entry_order_id: None,
        };
        let row = self.trade_row(&position, None, None, None, entry_fee, None);
        effects.push(Effect::Trade(Box::new(row)));
        effects.push(Effect::Notify {
            trade_id: position.id.clone(),
            pair: position.pair.clone(),
            side: opening.to_string(),
            price,
            quantity,
            pnl: 0.0,
            action: "open",
        });
        effects.push(Effect::Log {
            level: "trade",
            message: format!("Opened {opening} {} at {price:.4} (qty {quantity:.6})", req.pair),
        });
        self.open_positions.push(position);
        effects.push(Effect::Equity);
        effects
    }

    /// Close one position (by id) or every position: at the mark with the
    /// closing slippage in paper mode, with a market sell in live mode.
    pub fn close(&mut self, position_id: Option<&str>, now: &str) -> Vec<Effect> {
        let mut effects = Vec::new();
        let indices: Vec<usize> = self
            .open_positions
            .iter()
            .enumerate()
            .filter(|(_, p)| position_id.map(|id| id == p.id).unwrap_or(true))
            .map(|(i, _)| i)
            .collect();
        for idx in indices.into_iter().rev() {
            if self.mode == Mode::Live {
                self.sell_live(idx, now, &mut effects);
                continue;
            }
            let side_long = self.open_positions[idx].side == "long";
            let slip = if side_long {
                1.0 - self.slippage_pct / 100.0
            } else {
                1.0 + self.slippage_pct / 100.0
            };
            let price = (self.last_price * slip).max(0.0001);
            self.close_at(idx, price, now, &mut effects);
        }
        effects.push(Effect::Equity);
        effects
    }

    /// Book the exit of `open_positions[idx]` at `price` (paper).
    fn close_at(&mut self, idx: usize, price: f64, now: &str, effects: &mut Vec<Effect>) {
        let position = self.open_positions.remove(idx);
        let exit_fee = price * position.quantity * self.fee_rate;
        let total_fee = position.entry_fee + exit_fee;
        let pnl = if position.side == "long" {
            (price - position.entry_price) * position.quantity - total_fee
        } else {
            (position.entry_price - price) * position.quantity - total_fee
        };
        if position.side == "long" {
            self.balance += price * position.quantity - exit_fee;
        } else {
            self.balance += position.reserved_margin
                + (position.entry_price - price) * position.quantity
                - exit_fee;
        }
        let row = self.trade_row(&position, Some(price), Some(now.to_string()), Some(pnl), total_fee, None);
        effects.push(Effect::Trade(Box::new(row)));
        effects.push(Effect::Notify {
            trade_id: position.id.clone(),
            pair: position.pair.clone(),
            side: position.side.clone(),
            price,
            quantity: position.quantity,
            pnl,
            action: "close",
        });
        effects.push(Effect::Log {
            level: "trade",
            message: format!(
                "Closed {} {} at {price:.4} (PnL {pnl:+.2})",
                position.side, position.pair
            ),
        });
    }

    // ── Live ──

    /// Route an order to the broker (PLAN-QUANTALGO §4.3). Spot is long-only:
    /// a buy opens the long, a sell closes it, a short is never opened.
    fn place_live(&mut self, req: OrderRequest, now: &str) -> Vec<Effect> {
        let mut effects = Vec::new();
        if req.pair != self.pair {
            effects.push(Effect::Log {
                level: "error",
                message: format!(
                    "A live bot trades its own pair only ({}); the {} order was not sent.",
                    self.pair, req.pair
                ),
            });
            return effects;
        }
        let Some(filters) = self.filters.clone() else {
            effects.push(Effect::Log {
                level: "error",
                message: "Live order routing is not connected for this bot; the order was not sent.".into(),
            });
            return effects;
        };
        if self.broker.is_none() {
            effects.push(Effect::Log {
                level: "error",
                message: "Live order routing is not connected for this bot; the order was not sent.".into(),
            });
            return effects;
        }
        let long_idx = self
            .open_positions
            .iter()
            .position(|p| p.pair == req.pair && p.side == "long");

        match req.side {
            OrderSide::Sell => {
                match long_idx {
                    Some(idx) => {
                        self.sell_live(idx, now, &mut effects);
                        if req.reverse {
                            effects.push(Effect::Log {
                                level: "warn",
                                message: "Spot live trading is long-only: the long was closed, no short was opened.".into(),
                            });
                        }
                    }
                    None => effects.push(Effect::Log {
                        level: "warn",
                        message: "Spot live trading is long-only; a sell signal without an open long is ignored.".into(),
                    }),
                }
                effects.push(Effect::Equity);
                effects
            }
            OrderSide::Buy => {
                if long_idx.is_some() {
                    effects.push(Effect::Log {
                        level: "warn",
                        message: format!(
                            "One open long position per pair is supported; skipping duplicate {} signal.",
                            req.pair
                        ),
                    });
                    return effects;
                }
                if self.open_positions.len() >= self.max_positions {
                    effects.push(Effect::Log {
                        level: "warn",
                        message: format!("Max positions ({}) reached, skipping buy signal.", self.max_positions),
                    });
                    return effects;
                }
                let price = self.last_price.max(0.0001);
                let wanted = if req.quantity > 0.0 {
                    req.quantity * price
                } else if let Some(fraction) = req.position_size {
                    self.balance * fraction
                } else {
                    self.balance * self.risk_per_trade / 100.0
                };
                let quote_amount = wanted.min(self.balance);
                let quote = self.quote_asset();
                if quote_amount < filters.min_notional || quote_amount <= 0.0 {
                    effects.push(Effect::Log {
                        level: "warn",
                        message: format!(
                            "Budget left for this order ({quote_amount:.2} {quote}) is below the venue minimum of {:.2} {quote}; buy skipped.",
                            filters.min_notional
                        ),
                    });
                    effects.push(Effect::Equity);
                    return effects;
                }
                let result = self
                    .broker
                    .as_ref()
                    .map(|b| b.market_buy(&filters, quote_amount))
                    .unwrap_or_else(|| Err("no broker".into()));
                match result {
                    Ok(fill) => self.book_live_entry(fill, now, &mut effects),
                    Err(e) => effects.push(Effect::Log {
                        level: "error",
                        message: format!("Live buy of {quote_amount:.2} {quote} on {} rejected: {e}", req.pair),
                    }),
                }
                effects.push(Effect::Equity);
                effects
            }
        }
    }

    /// Market-sell `open_positions[idx]` through the broker and book the fill.
    /// A rejected sell leaves the position open — the next signal or a manual
    /// close tries again.
    fn sell_live(&mut self, idx: usize, now: &str, effects: &mut Vec<Effect>) {
        let (Some(filters), Some(broker)) = (self.filters.clone(), self.broker.as_ref()) else {
            effects.push(Effect::Log {
                level: "error",
                message: "Live order routing is not connected for this bot; the position stays open.".into(),
            });
            return;
        };
        let position = self.open_positions[idx].clone();
        if position.side != "long" {
            effects.push(Effect::Log {
                level: "error",
                message: format!("A live bot holds no {} positions; nothing to close.", position.side),
            });
            return;
        }
        match broker.market_sell(&filters, position.quantity) {
            Ok(fill) => self.book_live_exit(idx, fill, now, effects),
            Err(e) => effects.push(Effect::Log {
                level: "error",
                message: format!(
                    "Live sell of {} {} rejected: {e}; the position stays open.",
                    filters.qty_string(position.quantity), self.base_asset()
                ),
            }),
        }
    }

    /// Book a live buy: the base actually credited becomes the position, the
    /// quote spent (plus a fee charged in quote) leaves the cash.
    fn book_live_entry(&mut self, fill: Fill, now: &str, effects: &mut Vec<Effect>) {
        let quote = self.quote_asset();
        self.balance -= fill.quote_amount + fill.fee_from_cash(&quote);
        let position = Position {
            id: Uuid::new_v4().to_string(),
            pair: self.pair.clone(),
            side: "long".into(),
            entry_price: fill.avg_price,
            quantity: fill.base_received,
            entry_fee: fill.fee_quote,
            reserved_margin: 0.0,
            entry_time: now.to_string(),
            entry_order_id: Some(fill.order_id.clone()),
        };
        let row = self.trade_row(&position, None, None, None, fill.fee_quote, None);
        effects.push(Effect::Trade(Box::new(row)));
        effects.push(Effect::Notify {
            trade_id: position.id.clone(),
            pair: position.pair.clone(),
            side: "long".into(),
            price: fill.avg_price,
            quantity: fill.base_received,
            pnl: 0.0,
            action: "open",
        });
        effects.push(Effect::Log {
            level: "trade",
            message: format!(
                "LIVE bought {} {} at {:.4} for {:.2} {quote} (fee {:.6} {}{}) — order {}",
                self.filters.as_ref().map(|f| f.qty_string(fill.base_received)).unwrap_or_else(|| format!("{:.6}", fill.base_received)),
                self.base_asset(),
                fill.avg_price,
                fill.quote_amount,
                fill.fee_amount,
                fill.fee_asset,
                if fill.fee_estimated { ", estimated" } else { "" },
                fill.order_id
            ),
        });
        self.open_positions.push(position);
    }

    /// Book a live sell of `open_positions[idx]`. The proceeds net of a fee
    /// charged in quote come back to the cash; the fees of both legs, valued
    /// in quote, are in the pnl. Base the venue could not sell (below the lot
    /// step) stays on the exchange as dust and is logged.
    fn book_live_exit(&mut self, idx: usize, fill: Fill, now: &str, effects: &mut Vec<Effect>) {
        let position = self.open_positions.remove(idx);
        let quote = self.quote_asset();
        let proceeds = fill.quote_amount - fill.fee_from_cash(&quote);
        self.balance += proceeds;
        let total_fee = position.entry_fee + fill.fee_quote;
        let pnl = (fill.avg_price - position.entry_price) * fill.base_qty - total_fee;
        let dust = position.quantity - fill.base_qty;
        let row = self.trade_row(
            &position,
            Some(fill.avg_price),
            Some(now.to_string()),
            Some(pnl),
            total_fee,
            Some(&fill.order_id),
        );
        effects.push(Effect::Trade(Box::new(row)));
        effects.push(Effect::Notify {
            trade_id: position.id.clone(),
            pair: position.pair.clone(),
            side: "long".into(),
            price: fill.avg_price,
            quantity: fill.base_qty,
            pnl,
            action: "close",
        });
        effects.push(Effect::Log {
            level: "trade",
            message: format!(
                "LIVE sold {} {} at {:.4} for {:.2} {quote} (fee {:.6} {}{}) — PnL {pnl:+.2} — order {}",
                self.filters.as_ref().map(|f| f.qty_string(fill.base_qty)).unwrap_or_else(|| format!("{:.6}", fill.base_qty)),
                self.base_asset(),
                fill.avg_price,
                fill.quote_amount,
                fill.fee_amount,
                fill.fee_asset,
                if fill.fee_estimated { ", estimated" } else { "" },
                fill.order_id
            ),
        });
        if dust > 1e-12 {
            effects.push(Effect::Log {
                level: "info",
                message: format!(
                    "{dust:.8} {} below the lot step stays on the exchange as dust.",
                    self.base_asset()
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    fn runtime() -> BotRuntime {
        BotRuntime {
            bot_id: "bot-1".into(),
            strategy_id: "strat".into(),
            exchange_id: "ex".into(),
            pair: "BTC/USDT".into(),
            mode: Mode::Paper,
            balance: 10_000.0,
            last_price: 100.0,
            open_positions: Vec::new(),
            fee_rate: 0.001,
            slippage_pct: 0.0,
            risk_per_trade: 2.0,
            max_positions: 2,
            broker: None,
            filters: None,
        }
    }

    fn buy(qty: f64) -> OrderRequest {
        OrderRequest { side: OrderSide::Buy, pair: "BTC/USDT".into(), quantity: qty, position_size: None, reverse: false }
    }

    fn sell(qty: f64) -> OrderRequest {
        OrderRequest { side: OrderSide::Sell, pair: "BTC/USDT".into(), quantity: qty, position_size: None, reverse: false }
    }

    fn trades(effects: &[Effect]) -> Vec<&Trade> {
        effects.iter().filter_map(|e| match e { Effect::Trade(t) => Some(t.as_ref()), _ => None }).collect()
    }

    fn logs(effects: &[Effect]) -> Vec<&str> {
        effects.iter().filter_map(|e| match e { Effect::Log { message, .. } => Some(message.as_str()), _ => None }).collect()
    }

    #[test]
    fn a_long_costs_notional_plus_fee_and_closes_with_both_fees_in_pnl() {
        let mut rt = runtime();
        let opened = rt.place(buy(10.0), "t0");
        assert_eq!(rt.open_positions.len(), 1);
        assert!((rt.balance - (10_000.0 - 1_000.0 - 1.0)).abs() < 1e-9, "cash minus notional minus fee");
        let row = trades(&opened)[0];
        assert_eq!(row.bot_id.as_deref(), Some("bot-1"));
        assert!(row.exit_price.is_none());
        assert!(row.notes.is_none());

        rt.last_price = 110.0;
        assert!((rt.equity() - (8_999.0 + 1_100.0)).abs() < 1e-9);

        let closed = rt.close(None, "t1");
        let row = trades(&closed)[0];
        assert_eq!(row.exit_price, Some(110.0));
        // (110 - 100) * 10 - entry fee 1.0 - exit fee 1.1
        assert!((row.pnl.unwrap() - 97.9).abs() < 1e-9);
        assert!((rt.balance - (8_999.0 + 1_100.0 - 1.1)).abs() < 1e-9);
        assert!(rt.open_positions.is_empty());
        assert!(matches!(closed.last(), Some(Effect::Equity)));
    }

    #[test]
    fn a_short_reserves_full_collateral_and_returns_it_on_close() {
        let mut rt = runtime();
        rt.place(sell(10.0), "t0");
        let pos = &rt.open_positions[0];
        assert_eq!(pos.side, "short");
        assert_eq!(pos.reserved_margin, 1_000.0);
        assert!((rt.balance - (10_000.0 - 1_000.0 - 1.0)).abs() < 1e-9);

        rt.last_price = 90.0;
        assert!((rt.equity() - (8_999.0 + 1_000.0 + 100.0)).abs() < 1e-9);
        let closed = rt.close(None, "t1");
        // (100 - 90) * 10 - 1.0 - 0.9
        assert!((trades(&closed)[0].pnl.unwrap() - 98.1).abs() < 1e-9);
        assert!((rt.balance - (8_999.0 + 1_000.0 + 100.0 - 0.9)).abs() < 1e-9);
    }

    #[test]
    fn a_sell_against_a_long_closes_it_and_reverse_opens_the_short_on_post_close_cash() {
        let mut rt = runtime();
        rt.place(buy(10.0), "t0");
        rt.last_price = 120.0;
        let effects = rt.place(sell(0.0), "t1");
        assert!(rt.open_positions.is_empty(), "a plain sell only closes");
        assert_eq!(trades(&effects).len(), 1);

        rt.place(buy(10.0), "t2");
        let cash_before = rt.balance;
        let effects = rt.place(
            OrderRequest { side: OrderSide::Sell, pair: "BTC/USDT".into(), quantity: 0.0, position_size: Some(0.5), reverse: true },
            "t3",
        );
        let rows = trades(&effects);
        assert_eq!(rows.len(), 2, "one close, one open");
        assert_eq!(rt.open_positions.len(), 1);
        assert_eq!(rt.open_positions[0].side, "short");
        // sized from the cash AFTER the close: (cash_before + proceeds) * 0.5 / price
        let proceeds = 120.0 * 10.0 - 120.0 * 10.0 * 0.001;
        let expected_qty = (cash_before + proceeds) * 0.5 / 120.0;
        assert!((rt.open_positions[0].quantity - expected_qty).abs() < 1e-9);
    }

    #[test]
    fn duplicates_and_the_position_cap_are_refused_with_a_warning() {
        let mut rt = runtime();
        rt.place(buy(1.0), "t0");
        let dup = rt.place(buy(1.0), "t1");
        assert!(logs(&dup)[0].contains("One open long position per pair"));
        assert_eq!(rt.open_positions.len(), 1);

        let mut other = buy(1.0);
        other.pair = "ETH/USDT".into();
        rt.place(other, "t2");
        let mut third = buy(1.0);
        third.pair = "SOL/USDT".into();
        let capped = rt.place(third, "t3");
        assert!(logs(&capped)[0].contains("Max positions (2) reached"));
        assert_eq!(rt.open_positions.len(), 2);
    }

    #[test]
    fn insufficient_cash_leaves_everything_untouched() {
        let mut rt = runtime();
        let effects = rt.place(buy(1_000.0), "t0");
        assert!(logs(&effects)[0].contains("Insufficient balance"));
        assert!(rt.open_positions.is_empty());
        assert_eq!(rt.balance, 10_000.0);
    }

    #[test]
    fn slippage_moves_buys_up_and_sells_down() {
        let mut rt = runtime();
        rt.slippage_pct = 1.0;
        rt.place(buy(1.0), "t0");
        assert!((rt.open_positions[0].entry_price - 101.0).abs() < 1e-9);
        let closed = rt.close(None, "t1");
        assert!((trades(&closed)[0].exit_price.unwrap() - 99.0).abs() < 1e-9);
    }

    #[test]
    fn the_order_rpc_is_decoded_like_the_runner_sends_it() {
        let rt = runtime();
        let req = rt.parse_order("reverse", &json!({ "side": "sell", "position_size": 95.0 })).unwrap();
        assert_eq!(req.side, OrderSide::Sell);
        assert!(req.reverse);
        assert_eq!(req.pair, "BTC/USDT");
        assert!((req.position_size.unwrap() - 0.95).abs() < 1e-12);
        let req = rt.parse_order("buy", &json!({ "pair": "ETH/USDT", "quantity": 2.5 })).unwrap();
        assert_eq!(req.pair, "ETH/USDT");
        assert_eq!(req.quantity, 2.5);
        assert!(!req.reverse);
        assert!(rt.parse_order("reverse", &json!({ "side": "sideways" })).is_err());
    }

    #[test]
    fn live_mode_without_a_broker_refuses_to_pretend() {
        let mut rt = runtime();
        rt.mode = Mode::Live;
        let effects = rt.place(buy(1.0), "t0");
        assert!(logs(&effects)[0].contains("not connected"));
        assert!(rt.open_positions.is_empty());
    }

    // ── Live, against a mock broker ──

    /// Records every order and hands out scripted fills.
    struct MockBroker {
        fills: RefCell<VecDeque<Result<Fill, String>>>,
        calls: RefCell<Vec<String>>,
    }

    impl MockBroker {
        fn new(fills: Vec<Result<Fill, String>>) -> Self {
            Self { fills: RefCell::new(fills.into()), calls: RefCell::new(Vec::new()) }
        }
    }

    impl Broker for MockBroker {
        fn venue(&self) -> Venue { Venue::Binance }
        fn sandbox(&self) -> bool { true }
        fn symbol_filters(&self, _pair: &str) -> Result<SymbolFilters, String> { Ok(filters()) }
        fn available_balance(&self, _asset: &str) -> Result<f64, String> { Ok(1_000.0) }
        fn market_buy(&self, f: &SymbolFilters, quote_amount: f64) -> Result<Fill, String> {
            self.calls.borrow_mut().push(format!("buy {}", f.quote_string(quote_amount)));
            self.fills.borrow_mut().pop_front().unwrap_or_else(|| Err("no scripted fill".into()))
        }
        fn market_sell(&self, f: &SymbolFilters, base_qty: f64) -> Result<Fill, String> {
            self.calls.borrow_mut().push(format!("sell {}", f.qty_string(f.round_qty_down(base_qty))));
            self.fills.borrow_mut().pop_front().unwrap_or_else(|| Err("no scripted fill".into()))
        }
        fn test_market_buy(&self, _f: &SymbolFilters, _q: f64) -> Result<String, String> { Ok("ok".into()) }
    }

    fn filters() -> SymbolFilters {
        SymbolFilters {
            symbol: "BTCUSDT".into(),
            base: "BTC".into(),
            quote: "USDT".into(),
            qty_step: 0.00001,
            min_qty: 0.00001,
            min_notional: 5.0,
            price_tick: 0.01,
        }
    }

    fn live_runtime(fills: Vec<Result<Fill, String>>) -> BotRuntime {
        let mut rt = runtime();
        rt.mode = Mode::Live;
        rt.balance = 1_000.0;
        rt.last_price = 64_000.0;
        rt.risk_per_trade = 10.0;
        rt.broker = Some(Box::new(MockBroker::new(fills)));
        rt.filters = Some(filters());
        rt
    }

    fn calls(rt: &BotRuntime) -> Vec<String> {
        // The mock is the only broker the tests install.
        let dbg = format!("{:?}", rt);
        assert!(dbg.contains("Binance"), "debug output names the venue");
        rt.broker
            .as_ref()
            .map(|b| {
                let ptr = b.as_ref() as *const dyn Broker as *const MockBroker;
                // SAFETY: the tests only ever install a MockBroker.
                unsafe { (*ptr).calls.borrow().clone() }
            })
            .unwrap_or_default()
    }

    fn buy_fill_fee_in_base() -> Fill {
        Fill {
            order_id: "buy-1".into(),
            avg_price: 64_000.0,
            base_qty: 0.0015625,
            base_received: 0.0015609375,
            quote_amount: 100.0,
            fee_quote: 0.1,
            fee_asset: "BTC".into(),
            fee_amount: 0.0000015625,
            fee_estimated: false,
        }
    }

    #[test]
    fn a_live_buy_books_the_venue_fill_not_the_mark() {
        let mut rt = live_runtime(vec![Ok(buy_fill_fee_in_base())]);
        let effects = rt.place(buy(0.0), "t0");
        assert_eq!(calls(&rt), vec!["buy 100"], "10 % of the 1000 budget");
        assert_eq!(rt.open_positions.len(), 1);
        let pos = &rt.open_positions[0];
        assert!((pos.quantity - 0.0015609375).abs() < 1e-12, "the base credited, net of the base fee");
        assert_eq!(pos.entry_price, 64_000.0);
        assert!((pos.entry_fee - 0.1).abs() < 1e-12, "the fee valued in quote");
        assert_eq!(pos.entry_order_id.as_deref(), Some("buy-1"));
        assert!((rt.balance - 900.0).abs() < 1e-9, "a base fee leaves the cash alone");
        assert!((rt.equity() - (900.0 + 0.0015609375 * 64_000.0)).abs() < 1e-9);
        let row = trades(&effects)[0];
        assert_eq!(row.notes.as_deref(), Some("live · entry order buy-1"));
        assert!(logs(&effects).iter().any(|l| l.starts_with("LIVE bought")));
    }

    #[test]
    fn a_quote_fee_leaves_the_cash_and_a_live_sell_settles_pnl_from_both_fills() {
        let buy_fill = Fill {
            order_id: "buy-2".into(),
            avg_price: 64_000.0,
            base_qty: 0.0015625,
            base_received: 0.0015625,
            quote_amount: 100.0,
            fee_quote: 0.1,
            fee_asset: "USDT".into(),
            fee_amount: 0.1,
            fee_estimated: false,
        };
        let sell_fill = Fill {
            order_id: "sell-2".into(),
            avg_price: 65_000.0,
            base_qty: 0.00156,
            base_received: 0.00156,
            quote_amount: 101.4,
            fee_quote: 0.1014,
            fee_asset: "USDT".into(),
            fee_amount: 0.1014,
            fee_estimated: false,
        };
        let mut rt = live_runtime(vec![Ok(buy_fill), Ok(sell_fill)]);
        rt.place(buy(0.0), "t0");
        assert!((rt.balance - (1_000.0 - 100.0 - 0.1)).abs() < 1e-9, "quote fee comes out of the cash");

        rt.last_price = 65_000.0;
        let effects = rt.place(sell(0.0), "t1");
        assert_eq!(calls(&rt)[1], "sell 0.00156", "rounded down to the lot step");
        assert!(rt.open_positions.is_empty());
        let row = trades(&effects)[0];
        assert_eq!(row.exit_price, Some(65_000.0));
        // (65000 - 64000) * 0.00156 - 0.1 - 0.1014
        assert!((row.pnl.unwrap() - (1.56 - 0.2014)).abs() < 1e-9);
        assert!((row.fee - 0.2014).abs() < 1e-12);
        assert_eq!(row.notes.as_deref(), Some("live · entry order buy-2 · exit order sell-2"));
        assert!((rt.balance - (899.9 + 101.4 - 0.1014)).abs() < 1e-9);
        assert!(logs(&effects).iter().any(|l| l.contains("stays on the exchange as dust")));
    }

    #[test]
    fn spot_live_is_long_only() {
        let mut rt = live_runtime(vec![Ok(buy_fill_fee_in_base()), Ok(Fill {
            order_id: "sell-3".into(),
            avg_price: 64_000.0,
            base_qty: 0.00156,
            base_received: 0.00156,
            quote_amount: 99.84,
            fee_quote: 0.09984,
            fee_asset: "USDT".into(),
            fee_amount: 0.09984,
            fee_estimated: false,
        })]);
        let effects = rt.place(sell(0.0), "t0");
        assert!(logs(&effects)[0].contains("long-only"));
        assert!(calls(&rt).is_empty(), "no order was sent");

        rt.place(buy(0.0), "t1");
        let effects = rt.place(
            OrderRequest { side: OrderSide::Sell, pair: "BTC/USDT".into(), quantity: 0.0, position_size: Some(0.5), reverse: true },
            "t2",
        );
        assert!(rt.open_positions.is_empty(), "the long was closed");
        assert!(logs(&effects).iter().any(|l| l.contains("no short was opened")));
        assert_eq!(calls(&rt).len(), 2, "one buy, one sell, never a short");
    }

    #[test]
    fn a_rejected_live_order_changes_nothing_and_a_tiny_budget_is_skipped_before_sending() {
        let mut rt = live_runtime(vec![Err("Binance 400 (-2010): insufficient balance".into())]);
        let effects = rt.place(buy(0.0), "t0");
        assert!(logs(&effects)[0].contains("rejected: Binance 400"));
        assert!(rt.open_positions.is_empty());
        assert_eq!(rt.balance, 1_000.0);

        rt.balance = 30.0; // 10 % of it is below the 5 USDT minimum
        let effects = rt.place(buy(0.0), "t1");
        assert!(logs(&effects)[0].contains("below the venue minimum"));
        assert_eq!(calls(&rt).len(), 1, "nothing new was sent");
    }

    #[test]
    fn a_live_bot_never_trades_another_pair() {
        let mut rt = live_runtime(vec![Ok(buy_fill_fee_in_base())]);
        let mut other = buy(0.0);
        other.pair = "ETH/USDT".into();
        let effects = rt.place(other, "t0");
        assert!(logs(&effects)[0].contains("its own pair only"));
        assert!(calls(&rt).is_empty());
    }

    #[test]
    fn closing_a_live_bot_sells_through_the_broker_and_keeps_the_position_on_failure() {
        let mut rt = live_runtime(vec![Ok(buy_fill_fee_in_base()), Err("Bybit 503".into())]);
        rt.place(buy(0.0), "t0");
        let effects = rt.close(None, "t1");
        assert!(logs(&effects)[0].contains("rejected: Bybit 503"));
        assert_eq!(rt.open_positions.len(), 1, "the position stays on the books");
        assert!(matches!(effects.last(), Some(Effect::Equity)));
    }
}
