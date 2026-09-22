"""Entry point: ``python -m rotation_lab``.

Runs the JSON-RPC engine server that QuantSystems' Tauri backend drives.
The legacy customtkinter GUI was dropped when the engine became the
compute core for the QuantSystems desktop app.
"""

from .rpc import main

if __name__ == "__main__":
    main()
