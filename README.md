# QuantSuite

[![Build QuantSuite](https://github.com/QuantableX/QuantSuite/actions/workflows/build.yml/badge.svg)](https://github.com/QuantableX/QuantSuite/actions/workflows/build.yml)

One desktop application for the Quant apps: one installation, one tray icon and
one update channel. Built with Tauri 2, Rust, Nuxt and Vue.

## Download

Get **QuantSuite 1.0.0** from [GitHub Releases](https://github.com/QuantableX/QuantSuite/releases/latest).

| Platform | Download |
| --- | --- |
| Windows x64 | `.exe` installer or `.msi` |
| macOS Intel and Apple Silicon | Universal `.dmg` |
| Linux x64 (Ubuntu 24.04+ or equivalent) | `.AppImage`, `.deb` or `.rpm` |

QuantSuite checks for updates once at every start. When one is available, a
download button appears next to the settings button on the home screen; it
installs the update and restarts. **Settings → General** shows the installed
version and **Check for updates**. Save your work before installing. Updates are signed and apply to the entire suite;
individual modules do not check their former repositories. Linux automatic
updates require the AppImage installation; package-manager installations can
install the new `.deb` or `.rpm` from Releases.

The initial builds do not have commercial Windows signing or Apple notarization.
Operating systems may display their usual first-run security prompts. Update
signatures verify the release separately from operating-system code signing.

## Apps

| App | Modules |
| --- | --- |
| QuantZen | Notes, tasks, calendar, habits and finance |
| QuantView | Terminal, scripts, systems and algorithmic trading |
| QuantAgent | Pilot, MCP and memory |
| QuantSpace | Code, canvas and console |
| QuantHUD | Desktop overlay with everyday tools |

Module availability is configurable in suite settings. Workspaces connect the
editor, code index, kanban board and memory. Local data stays under the QuantSuite
user directory, separate from the repository.

QuantScript ships with an empty personal indicator library. Add your own scripts
in the app. Private indicators and research results are excluded from this
repository and installers. See [the library guide](docs/QUANTSCRIPT-PRIVATE-LIBRARY.md).

## Development

Install Node.js 22+, Rust stable, and the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/).
Optional Python features use Python 3.11+ and the requirements in `sidecars/python`.

```sh
npm ci
npm run tauri:dev
```

The frontend development port is 1420. The local MCP server uses port 3100 in
release builds and 3101 in development builds.

```sh
npm run check
npm run test:distribution
npm run test:release
npm run build
```

To build an unsigned local installer without the private release key:

```sh
npm run tauri:build -- --config apps/src-tauri/tauri.local-build.conf.json
```

## Source layout

- `apps/shell`: the shared Nuxt frontend and main menu.
- `apps/src-tauri`: the desktop binary, packaging and suite updater.
- `crates`: shared application services, terminal runtime and window lifecycle.
- `modules`: module frontends and Rust plugins.
- `packages`: shared TypeScript APIs and UI components.
- `sidecars/python`: optional Python engines.
- `scripts`: validation, regression tests and release tooling.

See [architecture](docs/ARCHITECTURE.md), [design](docs/DESIGN.md),
[AgentOS integration](docs/AGENTOS-IMPORT.md) and [releasing](docs/RELEASING.md).

Public history starts at version 1.0.0.
