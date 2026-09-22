"""Core Strategy base class for QuantAlgo."""

from __future__ import annotations

import json
import sys
from typing import Any, Dict, List, Optional

from quantalgo.models import Candle, Order, Position, Tick, Trade
from quantalgo import indicators as ind

# Orders are only kept for introspection, and a live run is long-lived, so
# keep a bounded tail instead of every order ever placed.
MAX_ORDER_HISTORY = 1000


class Strategy:
    """Base class for all QuantAlgo trading strategies.

    Subclass this and override the ``on_*`` event handlers.  Order methods
    (``buy``, ``sell``, ``close``, ``cancel``) emit JSON-RPC messages that
    the host process (Rust/Tauri) will handle.
    """

    params: Dict[str, Any] = {}

    def __init__(self) -> None:
        self.params = dict(getattr(self, "params", {}))
        self._positions: Dict[str, Position] = {}
        self._orders: List[Order] = []
        self._balance: Dict[str, float] = {}
        self._candle_history: List[Candle] = []
        self._pair: str = "BTC/USDT"  # set by runner/engine
        self._mark_price: Optional[float] = None
        self._mark_prices: Dict[str, float] = {}
        self._rpc: Optional[Any] = None  # set by runner

    # ------------------------------------------------------------------
    # Event handlers (override in subclass)
    # ------------------------------------------------------------------

    def on_candle(self, candle: Candle) -> None:
        """Called when a new candle closes."""

    def on_tick(self, tick: Tick) -> None:
        """Called on every price tick."""

    def on_trade(self, trade: Trade) -> None:
        """Called when one of the strategy's orders fills."""

    def on_start(self) -> None:
        """Called once when the strategy starts."""

    def on_stop(self) -> None:
        """Called once when the strategy stops."""

    # ------------------------------------------------------------------
    # Order methods
    # ------------------------------------------------------------------

    def buy(self, pair: str, quantity: float, price: float = None) -> str:
        """Place a buy order.  Returns the generated order id.

        If *price* is ``None`` the order is treated as a market order.
        """
        order = Order(pair=pair, side="buy", quantity=quantity, price=price)
        self._record_order(order)
        self._send_rpc("buy", order.to_dict())
        return order.id

    def sell(self, pair: str, quantity: float, price: float = None) -> str:
        """Place a sell order.  Returns the generated order id."""
        order = Order(pair=pair, side="sell", quantity=quantity, price=price)
        self._record_order(order)
        self._send_rpc("sell", order.to_dict())
        return order.id

    def reverse(self, pair: str, side: str, position_size: float = None) -> None:
        """Close the opposing position first, then open ``side`` with fresh balance.

        ``side`` is ``"buy"`` for long or ``"sell"`` for short.  When
        ``position_size`` is provided, the host sizes the new entry from the
        post-close cash balance.
        """
        if side not in ("buy", "sell"):
            raise ValueError("reverse side must be 'buy' or 'sell'")
        payload = {"pair": pair, "side": side, "quantity": 0.0}
        if position_size is not None:
            payload["position_size"] = float(position_size)
        self._send_rpc("reverse", payload)

    def close(self, position_id: str = None) -> None:
        """Close a specific position, or all positions when *position_id* is ``None``."""
        self._send_rpc("close", {"position_id": position_id})

    def cancel(self, order_id: str) -> None:
        """Cancel a pending order by id."""
        self._orders = [o for o in self._orders if o.id != order_id]
        self._send_rpc("cancel", {"order_id": order_id})

    # ------------------------------------------------------------------
    # Data access
    # ------------------------------------------------------------------

    def get_position(self, pair: str = None) -> Optional[Position | Dict[str, Position]]:
        """Return the position for *pair*, or all positions if *pair* is ``None``."""
        if pair is None:
            return dict(self._positions)
        return self._positions.get(pair)

    def get_balance(self, asset: str = None) -> Optional[float | Dict[str, float]]:
        """Return balance for *asset*, or all balances if *asset* is ``None``."""
        if asset is None:
            return dict(self._balance)
        return self._balance.get(asset)

    def get_mark_price(self, pair: str = None) -> Optional[float]:
        """Return the latest mark price for *pair*, or the active pair."""
        if pair is None:
            pair = self._pair
        return self._mark_prices.get(pair, self._mark_price)

    def get_candles(self, pair: str, timeframe: str, limit: int = 100) -> List[Candle]:
        """Return the most recent candles from local history.

        In live mode this issues an RPC to the host.  In backtest mode
        the engine populates ``_candle_history`` directly.
        """
        if self._rpc is not None:
            self._send_rpc(
                "get_candles",
                {"pair": pair, "timeframe": timeframe, "limit": limit},
            )
        # Return from local cache (backtest or after host populates)
        return list(self._candle_history[-limit:])

    # ------------------------------------------------------------------
    # Indicator convenience wrappers
    # ------------------------------------------------------------------

    def sma(self, series: List[float], period: int) -> List[Optional[float]]:
        """Simple Moving Average."""
        return ind.sma(series, period)

    def ema(self, series: List[float], period: int) -> List[Optional[float]]:
        """Exponential Moving Average."""
        return ind.ema(series, period)

    def rsi(self, series: List[float], period: int = 14) -> List[Optional[float]]:
        """Relative Strength Index (0-100)."""
        return ind.rsi(series, period)

    def macd(
        self,
        series: List[float],
        fast: int = 12,
        slow: int = 26,
        signal: int = 9,
    ) -> Dict[str, List[Optional[float]]]:
        """MACD (macd line, signal line, histogram)."""
        return ind.macd(series, fast, slow, signal)

    def bollinger(
        self,
        series: List[float],
        period: int = 20,
        std: float = 2.0,
    ) -> Dict[str, List[Optional[float]]]:
        """Bollinger Bands (upper, middle, lower)."""
        return ind.bollinger(series, period, std)

    def atr(self, candles: list, period: int = 14) -> List[Optional[float]]:
        """Average True Range."""
        return ind.atr(candles, period)

    # ------------------------------------------------------------------
    # Regime indicators (the Indicator Smithery, PLAN-QUANTALGO §6)
    # ------------------------------------------------------------------

    def regime(self, indicator: str, params: Optional[dict] = None) -> int:
        """The forge indicator's regime on the latest candle: +1 uptrend,
        -1 downtrend, 0 while it still warms up. ``indicator`` is a registry
        key (``"hilbert"``, ``"slopes"``, …); ``params`` overrides its
        certified defaults. Needs numpy/pandas, imported on first use."""
        from quantalgo import regime as _regime

        return _regime.regime_signal(self._candle_history, indicator, params)

    def regime_series(self, indicator: str, params: Optional[dict] = None) -> List[int]:
        """The forge indicator's whole ±1 stream over the candle history."""
        from quantalgo import regime as _regime

        return _regime.regime_series(self._candle_history, indicator, params)

    # ------------------------------------------------------------------
    # Logging
    # ------------------------------------------------------------------

    def log(self, message: str, level: str = "info") -> None:
        """Send a log message to the host process."""
        self._send_rpc("log", {"level": level, "message": message})

    # ------------------------------------------------------------------
    # Internal
    # ------------------------------------------------------------------

    def _record_order(self, order: Order) -> None:
        """Append *order* to the local order log, keeping it bounded."""
        self._orders.append(order)
        if len(self._orders) > MAX_ORDER_HISTORY:
            del self._orders[:-MAX_ORDER_HISTORY]

    def _send_rpc(self, method: str, params: dict) -> None:
        """Write a JSON-RPC message to stdout.

        In backtest mode the runner replaces this with a local handler
        so no actual I/O happens.
        """
        msg = json.dumps({"method": method, "params": params})
        sys.stdout.write(msg + "\n")
        sys.stdout.flush()
