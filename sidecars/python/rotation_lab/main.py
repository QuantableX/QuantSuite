"""Legacy module shim.

QuantSystems runs the engine headless via the JSON-RPC server in
``rotation_lab.rpc``. The old customtkinter GUI entry point has been
removed; this just forwards to the RPC server for backwards-compatible
``from .main import main`` imports.
"""

from .rpc import main

__all__ = ["main"]
