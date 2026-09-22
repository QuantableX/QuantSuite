"""Data models for QuantAlgo strategy SDK."""

from dataclasses import dataclass, field, asdict
from uuid import uuid4


@dataclass
class Candle:
    """OHLCV candle data."""
    time: str
    open: float
    high: float
    low: float
    close: float
    volume: float
    pair: str = ""

    @classmethod
    def from_dict(cls, d: dict) -> "Candle":
        return cls(
            time=d["time"],
            open=float(d["open"]),
            high=float(d["high"]),
            low=float(d["low"]),
            close=float(d["close"]),
            volume=float(d["volume"]),
            pair=str(d.get("pair", "")),
        )

    def to_dict(self) -> dict:
        return asdict(self)


@dataclass
class Tick:
    """Real-time price tick."""
    time: str
    price: float
    volume: float
    pair: str

    @classmethod
    def from_dict(cls, d: dict) -> "Tick":
        return cls(
            time=d["time"],
            price=float(d["price"]),
            volume=float(d["volume"]),
            pair=d["pair"],
        )

    def to_dict(self) -> dict:
        return asdict(self)


@dataclass
class Trade:
    """Executed trade record."""
    id: str
    pair: str
    side: str       # 'long' | 'short'
    price: float
    quantity: float
    time: str
    pnl: float = 0.0
    action: str = "fill"  # 'open' | 'close' | 'fill'

    @classmethod
    def from_dict(cls, d: dict) -> "Trade":
        return cls(
            id=d["id"],
            pair=d["pair"],
            side=d["side"],
            price=float(d["price"]),
            quantity=float(d["quantity"]),
            time=d["time"],
            pnl=float(d.get("pnl", 0.0)),
            action=str(d.get("action", "fill")),
        )

    def to_dict(self) -> dict:
        return asdict(self)


@dataclass
class Order:
    """Order to be submitted."""
    pair: str
    side: str       # 'buy' | 'sell'
    quantity: float
    price: float = None   # None = market order
    id: str = field(default_factory=lambda: str(uuid4()))

    def to_dict(self) -> dict:
        d = {
            "id": self.id,
            "pair": self.pair,
            "side": self.side,
            "quantity": self.quantity,
        }
        if self.price is not None:
            d["price"] = self.price
        return d

    @classmethod
    def from_dict(cls, d: dict) -> "Order":
        return cls(
            pair=d["pair"],
            side=d["side"],
            quantity=float(d["quantity"]),
            price=float(d["price"]) if d.get("price") is not None else None,
            id=d.get("id", str(uuid4())),
        )


@dataclass
class Position:
    """Open position."""
    pair: str
    side: str           # 'long' | 'short'
    entry_price: float
    quantity: float
    unrealized_pnl: float = 0.0
    id: str = ""
    entry_fee: float = 0.0
    reserved_margin: float = 0.0

    @classmethod
    def from_dict(cls, d: dict) -> "Position":
        return cls(
            pair=d["pair"],
            side=d["side"],
            entry_price=float(d["entry_price"]),
            quantity=float(d["quantity"]),
            unrealized_pnl=float(d.get("unrealized_pnl", 0.0)),
            id=str(d.get("id", "")),
            entry_fee=float(d.get("entry_fee", 0.0)),
            reserved_margin=float(d.get("reserved_margin", 0.0)),
        )

    def to_dict(self) -> dict:
        return asdict(self)
