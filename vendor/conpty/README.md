# Microsoft ConPTY runtime

Pinned package: `Microsoft.Windows.Console.ConPTY` **1.24.260710001** (MIT).

Source: https://www.nuget.org/packages/Microsoft.Windows.Console.ConPTY/1.24.260710001

Package download: https://api.nuget.org/v3-flatcontainer/microsoft.windows.console.conpty/1.24.260710001/microsoft.windows.console.conpty.1.24.260710001.nupkg

Package SHA-256: `175640566a3b59c4b132070ee96c2c77e5ab7edd2e92732a5eb3610bbf63d90e`

Each architecture directory contains an unmodified matching pair:

| Directory | DLL in package | Host in package |
| --- | --- | --- |
| x86_64 | runtimes/win-x64/native/conpty.dll | build/native/runtimes/x64/OpenConsole.exe |
| aarch64 | runtimes/win-arm64/native/conpty.dll | build/native/runtimes/arm64/OpenConsole.exe |
| x86 | runtimes/win-x86/native/conpty.dll | build/native/runtimes/x86/OpenConsole.exe |

The system ConPTY on Windows 11 build 26100 emits synchronized-output markers
before their screen changes. This makes a TUI's intermediate cursor positions
visible even when xterm supports DEC mode 2026. Shipping Microsoft's runtime
keeps these updates ordered independently of the installed Windows version.

`qs-pty/build.rs` stages the selected architecture next to Cargo binaries/tests.
The Windows Tauri bundle includes this directory. `windows_runtime.rs` loads
the absolute application-owned DLL before portable-pty initializes, and reports
missing runtime files instead of silently returning to the affected system host.

After an update, run `cargo test -p qs-pty` on Windows and
`node --test scripts/test-terminal-rendering.mjs scripts/test-space-dock.mjs`.
The native regression exercises real ConPTY output, including the frame
boundaries; feeding synthetic escape sequences directly to xterm misses this bug.
