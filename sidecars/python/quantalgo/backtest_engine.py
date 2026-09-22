"""Local backtest simulation engine for QuantAlgo strategies.

The engine feeds candles one-by-one into a strategy instance, simulates
order fills, tracks positions, equity, and computes performance stats.
All arithmetic uses plain Python floats -- no external dependencies.
"""

from __future__ import annotations

import math
import logging
from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Dict, List, Optional
from uuid import uuid4

from quantalgo.models import Candle, Order, Position, Trade

log = logging.getLogger("quantalgo.backtest")


# -----------------------------------------------------------------------
# Configuration
# -----------------------------------------------------------------------

@dataclass
class BacktestConfig:
    """Parameters for a backtest run."""
    initial_balance: float = 10_000.0
    commission_pct: float = 0.1       # 0.1 %  per fill
    slippage_pct: float = 0.05        # 0.05 % simulated slippage on market orders
    base_asset: str = "USDT"
    pair: str = "BTC/USDT"

    @classmethod
    def from_dict(cls, d: dict) -> "BacktestConfig":
        return cls(
            initial_balance=float(d.get("initial_balance", 10_000.0)),
            commission_pct=float(d.get("commission_pct", 0.1)),
            slippage_pct=float(d.get("slippage_pct", 0.05)),
            base_asset=d.get("base_asset", "USDT"),
            pair=d.get("pair", "BTC/USDT"),
        )


# -----------------------------------------------------------------------
# Internal order tracking
# -----------------------------------------------------------------------

@dataclass
class _PendingOrder:
    order: Order
    created_idx: int  # candle index when the order was placed


@dataclass
class _OpenPosition:
    id: str
    pair: str
    side: str           # 'long' | 'short'
    entry_price: float
    quantity: float
    entry_time: str
    entry_fee: float = 0.0   # commission already debited at open


# -----------------------------------------------------------------------
# Engine
# -----------------------------------------------------------------------

class BacktestEngine:
    """Run a strategy against historical candle data."""

    def __init__(self, strategy, candles: List[dict], config: BacktestConfig = None):
        self.strategy = strategy
        self.raw_candles = candles
        self.cfg = config or BacktestConfig()

        # State
        self.balance: float = self.cfg.initial_balance
        self.equity_curve: List[float] = []
        self.trades: List[Dict[str, Any]] = []
        self.pending_orders: List[_PendingOrder] = []
        self.positions: Dict[str, _OpenPosition] = {}  # id -> position
        self._candle_idx: int = 0

        # Intercept strategy RPC calls
        self.strategy._send_rpc = self._handle_rpc

    # ----- RPC interception ------------------------------------------------

    def _handle_rpc(self, method: str, params: dict) -> None:
        """Replace the strategy's ``_send_rpc`` to capture orders locally."""
        if method == "buy":
            order = Order.from_dict(params)
            self.pending_orders.append(_PendingOrder(order=order, created_idx=self._candle_idx))
        elif method == "sell":
            order = Order.from_dict(params)
            self.pending_orders.append(_PendingOrder(order=order, created_idx=self._candle_idx))
        elif method == "close":
            pos_id = params.get("position_id")
            if pos_id is not None:
                if pos_id in self.positions:
                    self._close_position(pos_id, self._current_candle)
            else:
                # Close all
                for pid in list(self.positions.keys()):
                    self._close_position(pid, self._current_candle)
        elif method == "reverse":
            self._reverse(params)
        elif method == "cancel":
            oid = params.get("order_id")
            self.pending_orders = [
                p for p in self.pending_orders if p.order.id != oid
            ]
        elif method == "log":
            level = params.get("level", "info")
            msg = params.get("message", "")
            getattr(log, level, log.info)(msg)
        else:
            log.warning("Unsupported strategy RPC in backtest mode: %s", method)

    def _reverse(self, params: dict) -> None:
        """Flip the position on a pair in one operation, like the live host.

        The opposing position is closed at the current candle and the new
        side is opened straight away, so the entry is sized from the
        post-close cash balance instead of resting until the next candle.
        """
        raw_side = params.get("side")
        if raw_side in ("buy", "long"):
            side = "buy"
        elif raw_side in ("sell", "short"):
            side = "sell"
        else:
            log.warning("Invalid reverse side: %s", raw_side)
            return

        pair = params.get("pair") or self.cfg.pair
        opposing = "short" if side == "buy" else "long"
        for pid in [
            p.id for p in self.positions.values()
            if p.pair == pair and p.side == opposing
        ]:
            self._close_position(pid, self._current_candle)

        price = float(self._current_candle["close"])
        quantity = float(params.get("quantity") or 0.0)
        if quantity <= 0.0:
            # Same sizing rule as the host: a fraction of the balance left
            # after the close. Percentages above 1 are read as percent.
            fraction = params.get("position_size")
            if fraction is None or price <= 0.0:
                log.warning(
                    "reverse on %s carries neither quantity nor position_size; "
                    "closed only, no new entry", pair,
                )
                return
            fraction = float(fraction)
            if fraction > 1.0:
                fraction /= 100.0
            if not 0.0 < fraction <= 1.0:
                log.warning("Invalid reverse position_size: %s", params.get("position_size"))
                return
            quantity = (self.balance * fraction) / price
        if quantity <= 0.0:
            return

        self._fill_order(
            Order(pair=pair, side=side, quantity=quantity), self._current_candle
        )

    # ----- Fill simulation -------------------------------------------------

    @property
    def _current_candle(self) -> dict:
        return self.raw_candles[self._candle_idx]

    def _apply_slippage(self, price: float, side: str) -> float:
        slip = price * (self.cfg.slippage_pct / 100.0)
        if side == "buy":
            return price + slip
        return price - slip

    def _fill_order(self, order: Order, candle: dict) -> None:
        """Attempt to fill an order against *candle*."""
        close_price = float(candle["close"])
        high = float(candle["high"])
        low = float(candle["low"])
        time = candle["time"]

        if order.price is not None:
            # Limit order: check if the price was reachable in this candle
            if order.side == "buy" and low <= order.price:
                fill_price = order.price
            elif order.side == "sell" and high >= order.price:
                fill_price = order.price
            else:
                return  # not filled
        else:
            # Market order: fill at close with slippage
            fill_price = self._apply_slippage(close_price, order.side)

        commission = fill_price * order.quantity * (self.cfg.commission_pct / 100.0)
        cost = fill_price * order.quantity + commission

        if order.side == "buy":
            # Check if we are closing a short position
            short_pos = self._find_position(order.pair, "short")
            if short_pos is not None:
                self._close_position_at(short_pos, fill_price, time, commission)
            else:
                # Open long
                if cost > self.balance:
                    log.warning("Insufficient balance for buy order %s", order.id)
                    if order.price is None:
                        # A market order does not rest: drop it instead of
                        # refilling many candles later at an unrelated price.
                        self.pending_orders = [
                            p for p in self.pending_orders if p.order.id != order.id
                        ]
                    return
                self.balance -= cost
                pos = self._find_position(order.pair, "long")
                if pos is not None:
                    self._add_to_position(pos, fill_price, order.quantity, commission)
                else:
                    pos_id = str(uuid4())
                    pos = _OpenPosition(
                        id=pos_id,
                        pair=order.pair,
                        side="long",
                        entry_price=fill_price,
                        quantity=order.quantity,
                        entry_time=time,
                        entry_fee=commission,
                    )
                    self.positions[pos_id] = pos
                # Update strategy view
                self.strategy._positions[order.pair] = Position(
                    pair=order.pair,
                    side="long",
                    entry_price=pos.entry_price,
                    quantity=pos.quantity,
                )
        else:
            # sell
            long_pos = self._find_position(order.pair, "long")
            if long_pos is not None:
                self._close_position_at(long_pos, fill_price, time, commission)
            else:
                # Open short
                self.balance -= commission
                pos = self._find_position(order.pair, "short")
                if pos is not None:
                    self._add_to_position(pos, fill_price, order.quantity, commission)
                else:
                    pos_id = str(uuid4())
                    pos = _OpenPosition(
                        id=pos_id,
                        pair=order.pair,
                        side="short",
                        entry_price=fill_price,
                        quantity=order.quantity,
                        entry_time=time,
                        entry_fee=commission,
                    )
                    self.positions[pos_id] = pos
                self.strategy._positions[order.pair] = Position(
                    pair=order.pair,
                    side="short",
                    entry_price=pos.entry_price,
                    quantity=pos.quantity,
                )

        # Remove from pending
        self.pending_orders = [
            p for p in self.pending_orders if p.order.id != order.id
        ]

    def _add_to_position(
        self,
        pos: _OpenPosition,
        fill_price: float,
        quantity: float,
        commission: float,
    ) -> None:
        """Fold an extra fill into an open position (volume-weighted entry).

        Pyramiding stays one position per pair/side so the engine and the
        strategy's ``_positions`` view -- which is keyed by pair -- cannot
        drift apart.
        """
        total_qty = pos.quantity + quantity
        if total_qty <= 0:
            return
        pos.entry_price = (
            pos.entry_price * pos.quantity + fill_price * quantity
        ) / total_qty
        pos.quantity = total_qty
        pos.entry_fee += commission

    def _find_position(self, pair: str, side: str) -> Optional[_OpenPosition]:
        for pos in self.positions.values():
            if pos.pair == pair and pos.side == side:
                return pos
        return None

    def _close_position(self, pos_id: str, candle: dict) -> None:
        pos = self.positions.get(pos_id)
        if pos is None:
            return
        close_price = float(candle["close"])
        commission = close_price * pos.quantity * (self.cfg.commission_pct / 100.0)
        self._close_position_at(pos, close_price, candle["time"], commission)

    def _close_position_at(
        self, pos: _OpenPosition, exit_price: float, time: str, commission: float
    ) -> None:
        # Both commissions belong to the trade: the entry side was already
        # debited from the balance at open, so without it the reported PnL
        # never reconciles with final_balance - initial_balance.
        if pos.side == "long":
            pnl = (exit_price - pos.entry_price) * pos.quantity - commission - pos.entry_fee
            self.balance += exit_price * pos.quantity - commission
        else:
            pnl = (pos.entry_price - exit_price) * pos.quantity - commission - pos.entry_fee
            self.balance += pnl + pos.entry_fee  # short PnL, entry fee already paid

        self.trades.append({
            "id": str(uuid4()),
            "pair": pos.pair,
            "side": pos.side,
            "entry_price": pos.entry_price,
            "exit_price": exit_price,
            "quantity": pos.quantity,
            "pnl": pnl,
            "entry_time": pos.entry_time,
            "exit_time": time,
            "commission": commission + pos.entry_fee,
        })

        # Notify strategy
        trade = Trade(
            id=self.trades[-1]["id"],
            pair=pos.pair,
            side=pos.side,
            price=exit_price,
            quantity=pos.quantity,
            time=time,
            pnl=pnl,
        )
        self.strategy.on_trade(trade)

        del self.positions[pos.id]
        self.strategy._positions.pop(pos.pair, None)

    # ----- Equity tracking -------------------------------------------------

    def _compute_equity(self, candle: dict) -> float:
        """Balance + market value of all open positions."""
        equity = self.balance
        close_price = float(candle["close"])
        for pos in self.positions.values():
            if pos.side == "long":
                # balance already had entry_price*qty deducted, so add back full market value
                equity += close_price * pos.quantity
            else:
                equity += (pos.entry_price - close_price) * pos.quantity
        return equity

    # ----- Main loop -------------------------------------------------------

    def run(self) -> Dict[str, Any]:
        """Execute the backtest and return a result dict."""
        self.strategy._balance = {self.cfg.base_asset: self.cfg.initial_balance}
        self.strategy._pair = self.cfg.pair
        self.strategy.on_start()

        # One `PROGRESS done/total` line per percent on stderr: the Rust host
        # reads them as they come and moves the page's bar — a month of
        # 1-minute candles is minutes of silence otherwise.
        total = len(self.raw_candles)
        step = max(1, total // 100)

        for idx, raw in enumerate(self.raw_candles):
            self._candle_idx = idx
            if (idx + 1) % step == 0 or idx + 1 == total:
                log.info("PROGRESS %d/%d", idx + 1, total)
            candle = Candle.from_dict(raw)

            # Append to strategy history
            self.strategy._candle_history.append(candle)

            # Try to fill pending orders
            for pending in list(self.pending_orders):
                self._fill_order(pending.order, raw)

            # Update unrealized PnL on strategy positions
            for pair, pos in list(self.strategy._positions.items()):
                opos = self._find_position(pair, pos.side)
                if opos is not None:
                    if pos.side == "long":
                        pos.unrealized_pnl = (candle.close - opos.entry_price) * opos.quantity
                    else:
                        pos.unrealized_pnl = (opos.entry_price - candle.close) * opos.quantity

            # Update balance view
            self.strategy._balance[self.cfg.base_asset] = self.balance

            # Feed candle to strategy
            self.strategy.on_candle(candle)

            # Record equity
            equity = self._compute_equity(raw)
            self.equity_curve.append(equity)

        # Close any remaining positions at last candle
        if self.raw_candles:
            last = self.raw_candles[-1]
            for pid in list(self.positions.keys()):
                self._close_position(pid, last)

        self.strategy.on_stop()

        return self._build_result()

    # ----- Result / stats --------------------------------------------------

    def _build_result(self) -> Dict[str, Any]:
        stats = self._compute_stats()
        return {
            "initial_balance": self.cfg.initial_balance,
            "final_balance": self.balance,
            "equity_curve": self.equity_curve,
            "trades": self.trades,
            "stats": stats,
        }

    def _compute_stats(self) -> Dict[str, Any]:
        initial = self.cfg.initial_balance
        final = self.balance

        total_return = ((final - initial) / initial) * 100.0 if initial else 0.0

        # Max drawdown
        max_dd = 0.0
        peak = initial
        for eq in self.equity_curve:
            if eq > peak:
                peak = eq
            dd = (peak - eq) / peak * 100.0 if peak else 0.0
            if dd > max_dd:
                max_dd = dd

        # Trade stats
        wins = [t for t in self.trades if t["pnl"] > 0]
        losses = [t for t in self.trades if t["pnl"] <= 0]
        total_trades = len(self.trades)
        win_rate = (len(wins) / total_trades * 100.0) if total_trades else 0.0

        gross_profit = sum(t["pnl"] for t in wins) if wins else 0.0
        gross_loss = abs(sum(t["pnl"] for t in losses)) if losses else 0.0
        profit_factor = (gross_profit / gross_loss) if gross_loss else float("inf") if gross_profit > 0 else 0.0

        # Average trade duration
        durations: List[float] = []
        for t in self.trades:
            try:
                entry = datetime.fromisoformat(t["entry_time"])
                exit_ = datetime.fromisoformat(t["exit_time"])
                durations.append((exit_ - entry).total_seconds())
            except (ValueError, KeyError):
                pass
        avg_duration_s = (sum(durations) / len(durations)) if durations else 0.0

        # Sharpe ratio (annualised, based on per-candle returns)
        sharpe = 0.0
        if len(self.equity_curve) > 1:
            returns = []
            for i in range(1, len(self.equity_curve)):
                prev = self.equity_curve[i - 1]
                if prev != 0:
                    returns.append((self.equity_curve[i] - prev) / prev)
            if returns:
                mean_r = sum(returns) / len(returns)
                var_r = sum((r - mean_r) ** 2 for r in returns) / len(returns)
                std_r = math.sqrt(var_r) if var_r > 0 else 0.0
                if std_r > 0:
                    # Assume ~365 candles/year for daily candles
                    sharpe = (mean_r / std_r) * math.sqrt(365)

        return {
            "total_return_pct": round(total_return, 4),
            "max_drawdown_pct": round(max_dd, 4),
            "sharpe_ratio": round(sharpe, 4),
            "total_trades": total_trades,
            "win_rate_pct": round(win_rate, 2),
            "profit_factor": round(profit_factor, 4) if profit_factor != float("inf") else "Infinity",
            "gross_profit": round(gross_profit, 4),
            "gross_loss": round(gross_loss, 4),
            "avg_trade_duration_seconds": round(avg_duration_s, 2),
        }
