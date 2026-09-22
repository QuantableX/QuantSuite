"""JSON-RPC runner for QuantAlgo strategies.

Usage
-----
Live / paper mode (reads line-delimited JSON-RPC on stdin):
    python -m quantalgo.runner path/to/strategy.py

Backtest mode (reads a single JSON blob with config + candle data):
    python -m quantalgo.runner path/to/strategy.py --backtest

The runner dynamically imports the first ``Strategy`` subclass found in
the given Python file, instantiates it, then either enters the live
JSON-RPC loop or runs a backtest.
"""

from __future__ import annotations

import argparse
import importlib.util
import inspect
import json
import logging
import sys
import traceback
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple, Type

from quantalgo.backtest_engine import BacktestConfig, BacktestEngine
from quantalgo.models import Candle, Tick, Trade
from quantalgo.strategy import Strategy

# ---------------------------------------------------------------------------
# Logging -- always goes to stderr so stdout stays clean for JSON-RPC.
# ---------------------------------------------------------------------------

logging.basicConfig(
    stream=sys.stderr,
    level=logging.INFO,
    format="[quantalgo] %(levelname)s %(message)s",
)
log = logging.getLogger("quantalgo.runner")

# The live loop runs for as long as the bot does (a year of 1m candles is
# ~525k objects) and ``get_candles`` only ever reads the tail, so the
# incoming history is trimmed to a bounded window.
MAX_CANDLE_HISTORY = 5000


# ---------------------------------------------------------------------------
# Strategy loader
# ---------------------------------------------------------------------------

def _load_strategy_class(path: str) -> Tuple[Type[Strategy], List[str]]:
    """Import *path* and return the deterministic Strategy subclass."""
    filepath = Path(path).resolve()
    if not filepath.exists():
        raise FileNotFoundError(f"Strategy file not found: {filepath}")

    spec = importlib.util.spec_from_file_location("_user_strategy", str(filepath))
    if spec is None or spec.loader is None:
        raise ImportError(f"Cannot create module spec for {filepath}")

    module = importlib.util.module_from_spec(spec)
    # Make quantalgo importable inside the user strategy
    sys.modules["_user_strategy"] = module
    spec.loader.exec_module(module)

    candidates: list[tuple[int, str, Type[Strategy]]] = []
    for name, obj in inspect.getmembers(module, inspect.isclass):
        if issubclass(obj, Strategy) and obj is not Strategy:
            try:
                source_file = Path(inspect.getsourcefile(obj) or "").resolve()
            except OSError:
                source_file = filepath
            if source_file != filepath:
                continue
            line_no = getattr(obj, "__firstlineno__", None)
            if line_no is None:
                try:
                    _, line_no = inspect.getsourcelines(obj)
                except OSError:
                    line_no = 0
            candidates.append((line_no, name, obj))

    if not candidates:
        raise ValueError(
            f"No Strategy subclass found in {filepath}. "
            "Define a class that inherits from quantalgo.Strategy."
        )

    candidates.sort(key=lambda item: (item[0], item[1]))
    warnings: List[str] = []
    if len(candidates) > 1:
        warnings.append(
            "Multiple Strategy subclasses found; using "
            f"{candidates[0][1]} by source order."
        )

    return candidates[0][2], warnings


# ---------------------------------------------------------------------------
# JSON-RPC helpers
# ---------------------------------------------------------------------------

def _write(msg: dict) -> None:
    """Write a JSON line to stdout and flush."""
    sys.stdout.write(json.dumps(msg) + "\n")
    sys.stdout.flush()


def _snapshot_positions(raw: Any) -> Dict[str, Any]:
    if isinstance(raw, dict):
        return raw
    return {}


def _apply_state_snapshot(strategy: Strategy, params: dict) -> None:
    if "pair" in params and params["pair"]:
        strategy._pair = str(params["pair"])

    if "candles" in params and isinstance(params["candles"], list):
        strategy._candle_history = [
            Candle.from_dict(candle)
            for candle in params["candles"]
            if isinstance(candle, dict)
        ]

    if "balance" in params and isinstance(params["balance"], dict):
        strategy._balance = {
            str(asset): float(amount)
            for asset, amount in params["balance"].items()
        }

    if "positions" in params:
        from quantalgo.models import Position

        strategy._positions = {
            str(pair): Position.from_dict(position)
            for pair, position in _snapshot_positions(params.get("positions")).items()
        }

    mark_price = params.get("mark_price")
    pair = params.get("pair") or strategy._pair
    if mark_price is not None:
        strategy._mark_price = float(mark_price)
        if pair:
            strategy._mark_prices[str(pair)] = float(mark_price)


def _apply_trade_update(strategy: Strategy, trade: Trade, params: dict) -> None:
    _apply_state_snapshot(strategy, params)
    strategy._pair = trade.pair or strategy._pair
    strategy._mark_price = trade.price
    if trade.pair:
        strategy._mark_prices[trade.pair] = trade.price

    if "positions" in params:
        return

    if trade.action == "close":
        strategy._positions.pop(trade.pair, None)
        return

    if trade.action == "open":
        from quantalgo.models import Position

        strategy._positions[trade.pair] = Position(
            pair=trade.pair,
            side=trade.side,
            entry_price=trade.price,
            quantity=trade.quantity,
            unrealized_pnl=0.0,
        )


def _read_line() -> Optional[dict]:
    """Read one JSON line from stdin. Returns None on EOF."""
    line = sys.stdin.readline()
    if not line:
        return None
    line = line.strip()
    if not line:
        return None
    return json.loads(line)


# ---------------------------------------------------------------------------
# Live / paper-trading loop
# ---------------------------------------------------------------------------

def _run_live(strategy: Strategy) -> None:
    """Enter the main JSON-RPC read loop."""
    strategy._rpc = _write  # mark as live mode

    log.info("Entering live JSON-RPC loop")

    while True:
        try:
            msg = _read_line()
        except json.JSONDecodeError as exc:
            log.error("Malformed JSON on stdin: %s", exc)
            continue

        if msg is None:
            log.info("stdin closed -- exiting")
            break

        method = msg.get("method")
        params: dict = msg.get("params", {})
        msg_id = msg.get("id")  # may be present for request/response

        try:
            result = _dispatch(strategy, method, params)
            if msg_id is not None:
                _write({"id": msg_id, "result": result})
            if result == "exit":
                log.info("Stop requested -- exiting")
                break
        except Exception:
            tb = traceback.format_exc()
            log.error("Error handling method=%s:\n%s", method, tb)
            if msg_id is not None:
                _write({
                    "id": msg_id,
                    "error": {"code": -1, "message": tb},
                })


def _dispatch(strategy: Strategy, method: str, params: dict) -> Any:
    """Route an incoming JSON-RPC method to the right handler."""
    if method == "on_candle":
        candle = Candle.from_dict(params)
        if candle.pair:
            strategy._pair = candle.pair
            strategy._mark_prices[candle.pair] = candle.close
        strategy._mark_price = candle.close
        strategy._candle_history.append(candle)
        if len(strategy._candle_history) > MAX_CANDLE_HISTORY:
            del strategy._candle_history[:-MAX_CANDLE_HISTORY]
        strategy.on_candle(candle)
        return "ok"

    if method == "on_tick":
        tick = Tick.from_dict(params)
        strategy._pair = tick.pair
        strategy._mark_price = tick.price
        strategy._mark_prices[tick.pair] = tick.price
        strategy.on_tick(tick)
        return "ok"

    if method == "on_trade":
        trade = Trade.from_dict(params)
        _apply_trade_update(strategy, trade, params)
        strategy.on_trade(trade)
        return "ok"

    if method == "start":
        new_params = params.get("params", {})
        if isinstance(new_params, dict):
            strategy.params.update(new_params)
        _apply_state_snapshot(strategy, params)
        strategy.on_start()
        return "ok"

    if method == "stop":
        strategy.on_stop()
        return "exit"

    if method == "configure":
        new_params = params.get("params", {})
        strategy.params.update(new_params)
        _apply_state_snapshot(strategy, params)
        return "ok"

    log.warning("Unknown method: %s", method)
    return None


# ---------------------------------------------------------------------------
# Backtest mode
# ---------------------------------------------------------------------------

def _run_backtest(strategy: Strategy) -> None:
    """Read a JSON blob from stdin, run the backtest, output results."""
    log.info("Backtest mode -- reading config from stdin")

    raw = sys.stdin.read()
    if not raw.strip():
        log.error("Empty stdin -- expected JSON backtest config")
        sys.exit(1)

    blob: dict = json.loads(raw)

    config_data = blob.get("config", {})
    config = BacktestConfig.from_dict(config_data)

    # Apply strategy params from blob
    strategy_params = blob.get("params", {})
    strategy.params.update(strategy_params)

    candles = blob.get("candles", [])
    if not candles:
        log.error("No candles provided in backtest input")
        sys.exit(1)

    log.info(
        "Running backtest: %d candles, initial_balance=%.2f",
        len(candles),
        config.initial_balance,
    )

    engine = BacktestEngine(strategy, candles, config)
    result = engine.run()

    _write(result)
    log.info("Backtest complete -- %d trades", len(result["trades"]))


def _run_validate(strategy_path: str) -> int:
    """Validate that a strategy can be imported and instantiated."""
    try:
        cls, warnings = _load_strategy_class(strategy_path)
        strategy = cls()
        if not isinstance(strategy, Strategy):
            raise TypeError(f"{cls.__name__} did not instantiate a Strategy")
        _write({
            "ok": True,
            "class_name": cls.__name__,
            "warnings": warnings,
        })
        return 0
    except Exception:
        tb = traceback.format_exc()
        log.error("Strategy validation failed:\n%s", tb)
        _write({
            "ok": False,
            "error": tb,
        })
        return 1


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main() -> None:
    parser = argparse.ArgumentParser(
        description="QuantAlgo strategy runner",
    )
    parser.add_argument(
        "strategy_path",
        help="Path to the .py file containing a Strategy subclass",
    )
    parser.add_argument(
        "--backtest",
        action="store_true",
        help="Run in backtest mode (read JSON blob from stdin)",
    )
    parser.add_argument(
        "--validate",
        action="store_true",
        help="Validate strategy import and instantiation, then exit",
    )
    args = parser.parse_args()

    if args.validate:
        sys.exit(_run_validate(args.strategy_path))

    try:
        cls, warnings = _load_strategy_class(args.strategy_path)
    except Exception as exc:
        log.error("Failed to load strategy: %s", exc)
        sys.exit(1)

    strategy = cls()
    log.info("Loaded strategy: %s", cls.__name__)
    for warning in warnings:
        log.warning(warning)

    if args.backtest:
        _run_backtest(strategy)
    else:
        _run_live(strategy)


if __name__ == "__main__":
    main()
