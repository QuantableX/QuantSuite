# QuantSuite — Architecture

Technical contracts for the fused application. Read `../project.md` first for
scope and rationale.

> **V3 (2026-08-15, see [PLAN-V3.md](PLAN-V3.md)):** where this document
> disagrees with PLAN-V3, PLAN-V3 wins. The deltas: manifests declare
> `"app"` + `"appOrder"` instead of a `rail` block (apps live in
> `modules/apps.json`; the rail lists apps); `status` gained `"stub"`;
> the modules `view`/`code` are `terminal`/`canvas` now; every module renders
> the shared chrome from `packages/ui` (QModuleHeader/QSidebar/QRightPanel,
> settings via `QSettingsModal` + `registerSettingsSections`); §10's circular
> launcher model was replaced in V2 already and `/` is the Dashboard.
>
> **QuantZen split (2026-08-25, PLAN-V3 §7):** QuantZen is an *app* now — the
> rail's top entry over `notes`, `plan`, `finance` — and the module formerly
> called QuantZen is `notes` / QuantNotes (plugin `notes`, route `/notes`,
> CSS `[data-module="notes"]` with the `--qn-` prefix). QuantAgent is
> `pilot` (since 2026-09-02, replacing `control`), `mcp`, `memory`. **The
> inventories below are pre-migration
> records and still spell the old ids** (`view`, `code`, `zen`) — they are the
> list the collisions were found in, not a description of today's tree.

---

## 1. Module Contract

A module is a directory under `modules/<id>/` that satisfies this contract. The
shell discovers modules from `module.json`; nothing else is hardcoded.

```
modules/systems/
├─ module.json              # manifest (below)
├─ nuxt.config.ts           # Nuxt Layer
├─ app/
│  ├─ pages/systems/        # ALL pages namespaced under the module id
│  ├─ components/           # auto-imported, prefixed by convention
│  ├─ composables/
│  ├─ stores/               # Pinia ids: "systems/engine", "systems/config"
│  └─ assets/css/main.css   # the module's ORIGINAL stylesheet, scoped per §9
└─ crate/                   # optional Rust side
   ├─ Cargo.toml            # package tauri-plugin-systems, library qs_mod_systems
   └─ src/lib.rs            # exposes init() -> TauriPlugin<R>
```

The package name is not a style choice: Tauri derives the plugin name, and
therefore the permission identifiers, from it (§2). The *library* name is what
`apps/src-tauri` calls `init()` on.

`console` is the first module written inside the suite rather than migrated into
it, so it is what the contract looks like with nothing inherited: pages under
`app/pages/console/`, components declared with the `Console` prefix, composables
imported explicitly, a stylesheet scoped from its first line, and no palette of
its own. Nothing below in "Layer gotchas" applied to it — those are the costs of
arriving from a standalone app, and a module born here pays none of them.

### `module.json`

```jsonc
{
  "id": "systems",
  "title": "QuantSystems",
  "description": "Multi-System Quantitative Evaluation Suite",
  "icon": "systems",
  "version": "0.1.0",

  "routes": {
    "root": "/systems",
    "home": "/systems"
  },

  "rail": { "order": 30, "shortcut": "Ctrl+3" },

  "plugin": "systems",                    // Tauri plugin name, if crate/ exists

  "publishes": [
    "systems.backtest.completed",
    "systems.signal.generated",
    "systems.engine.status"
  ],
  "subscribes": [
    "market.symbol.selected",
    "core.theme.changed"
  ],

  "sidecars": [
    { "id": "python-engine", "kind": "python", "shared": true, "lazy": true }
  ],

  "capabilities": [
    {
      "name": "run_backtest",
      "command": "plugin:systems|run_backtest",
      "description": "Run a rotation backtest for a system over a date range.",
      "sideEffects": "compute",
      "schema": { "type": "object", "properties": { "system": { "type": "string" } } }
    }
  ],

  "entities": ["system", "backtest", "universe_snapshot"],

  "migrateFrom": "~/.quantsystems"
}
```

`capabilities` drives the MCP bridge (§7). `entities` declares which kinds this
module registers in `core.db` (§5). `publishes`/`subscribes` are validated at
build time — a topic no module publishes is a build warning.

### Nuxt Layer

```ts
// modules/systems/nuxt.config.ts
import tailwindcss from '@tailwindcss/vite'

export default defineNuxtConfig({
  modules: ['@pinia/nuxt', '@vueuse/nuxt'],
  css: ['~/assets/css/main.css'],   // the module's own, unchanged in appearance
  vite: {
    plugins: [tailwindcss()],
    optimizeDeps: { include: ['lightweight-charts'] },
  },
})
```

The shell composes them:

```ts
// apps/shell/nuxt.config.ts
export default defineNuxtConfig({
  ssr: false,
  srcDir: 'app',
  extends: [
    '../../modules/systems',
    '../../modules/notes',
    '../../modules/algo',
    '../../modules/mcp',
    '../../modules/control',
    '../../modules/canvas',
    '../../modules/terminal',
    '../../modules/console',
    '../../modules/flow',
    '../../modules/plan',
    '../../modules/finance',
    '../../modules/memory',
    '../../modules/hud',
  ],
  css: ['~/assets/css/shell.css'],  // shell chrome + the single tailwind import (§9)
  devServer: { port: 1420 },
  compatibilityDate: '2025-01-01',
})
```

### Route namespacing is mandatory

Nuxt Layers merge `pages/` by directory, so unnamespaced pages collide. The
current collisions across the eight apps:

| Page | Colliding apps |
|---|---|
| `index.vue` | all eight |
| `settings.vue` | mcp, zen, control, algo |
| `terminal.vue` | view, algo |
| `journal.vue` | view, algo |
| `logs.vue` | mcp, control |
| `backtest.vue` | algo, systems |

Resolution: every module's pages move into `app/pages/<id>/`. `index.vue`
becomes `<id>/index.vue`; the suite home page lives only in the shell.

### Layer gotchas

Found by migrating `systems` in Phase 1. Every one of them fails quietly or
confusingly, and every subsequent module hits them.

**A layer's `app.vue` is ignored.** The shell owns `app.vue`; a layer that ships
one has it silently dropped, taking its store bootstrap and event wiring with it.
Merge that logic into the module's layout.

**Module layouts are named after the module, never `default`.** A layer's
`layouts/default.vue` would apply to shell pages too. QuantSystems' became
`layouts/systems.vue`, and every module page declares
`definePageMeta({ layout: 'systems' })`.

**The shell must render `<NuxtLayout>`.** With a bare `<NuxtPage />` the module's
layout never mounts — the page content appears without its chrome, and the only
signal is a `NUXT_E4007` warning in the console.

**`~/` does not resolve to the layer.** In a layer's `nuxt.config.ts` it points
at the *shell's* srcDir, so `css: ['~/assets/css/main.css']` fails the build.
Resolve from `import.meta.url` instead. Inside a layer's *source* files `~/`
works for Vite but not for `vue-tsc`, and with eight modules it is ambiguous
anyway — each module therefore declares an alias and imports via `#<id>/...`:

```ts
const moduleSrc = fileURLToPath(new URL('./app', import.meta.url))
export default defineNuxtConfig({ alias: { '#systems': moduleSrc } })
```

**Modules must not use viewport units.** QuantSystems' layout was
`width: 100vw; height: 100vh`; it is now `100%`. A module fills the box the shell
gives it. This is what keeps a future multi-pane shell a shell-only change —
verified in Phase 1: the module root measured 1224×682 inside a 1280×720 window.

**Window controls belong to the shell.** QuantSystems' titlebar had real
minimize/maximize/close wired to `getCurrentWindow()`, plus `startDragging()`.
Left in place, a module's close button would hide the entire suite to tray. The
control cluster and the drag handler were removed; the rest of its titlebar —
system switcher, view menu, logo action — stays. This is the one place Phase 1
departed from "identical to the standalone app", and deliberately.

**Scope more than element selectors.** §9 originally listed `html`, `body` and
friends. QuantSystems' stylesheet also defined `.card`, `.btn`, `.input`,
`.label` and `.pill` — generic enough to collide with the next module to arrive.
Everything except `:root` and `[data-theme]` gets scoped.

### More, found in Phase 2

**Shortcuts collide between shell and module.** QuantZen binds `Ctrl+K` to its
own palette; so does the shell. Both listeners are on `window`, so both fired.
The protocol: a module claims a key by calling `preventDefault()`, and the shell
checks `e.defaultPrevented` before acting. Modules mount below the shell, so
their handlers run first. Any module that claims a shell shortcut must call
`preventDefault()` or it will get two handlers.

**Assets referenced outside `app/` break.** QuantZen's header imported
`'../../../quantzen.png'` — the standalone repo root, which is not copied. Module
assets must live under the module's own `app/assets/`.

**Fixed-position overlays legitimately use viewport units.** QuantZen's command
palette and modal panel use `100vw`/`100vh`, and for `position: fixed` the
viewport *is* the containing block, so they are correct today. They are the known
exception to the no-viewport-units rule and will need revisiting only if the
shell ever renders modules as panes.

**Migrated crates arrived with inherited lints — the debt is now paid.** The
eight apps had never run `clippy -D warnings`, and the migration carried 63
warnings in with them: `manual_range_contains`, `redundant_closure`,
`needless_borrow`, dead variants, identical `if` branches. They were left visible
rather than silenced, then cleared: most by `cargo clippy --fix`, the rest by
hand. Three were real findings, not style —

- `ConfigError::NoConfigDir` in `hud` was dead because the suite resolves the
  directory with the infallible `qs_core::paths::module_dir`, so its only
  construction site vanished in the migration.
- A `match` arm in `kanban_db` had two identical branches, so the condition
  never mattered. Collapsed to the one value rather than guessing what the other
  case was meant to be — that would be a behaviour change.
- Three `from_str` methods already had `FromStr`'s exact signature and are now
  the trait, not inherent look-alikes.

**Every crate now denies warnings.** Four narrow exceptions are declared in the
crate's own `[lints]` table with a reason: Win32 FFI names in `hud` mirror the
Windows API and must not be renamed, and three migrated functions take 8–9
arguments where splitting the signature is a refactor rather than a lint fix. A
crate cannot merge `workspace = true` with its own entries, so those three spell
the policy out; everything else inherits it.

**Tauri capabilities are not user confirmation.** An early draft withheld
`start_bot`, `stop_bot` and the deletes from `algo:default`, reasoning from the
`sideEffects` classification. That was wrong: capabilities control which *webview*
may call a command, and the suite has exactly one trusted webview — withholding
only breaks the UI. Agent-facing gating belongs in the MCP bridge (§7).

### The worst one, found in Phase 3: auto-imports are global

Nuxt registers auto-imported **components and composables globally, by name,
across every layer**. Last layer wins, silently. With five modules loaded:

| Name | Defined in |
|---|---|
| `RightSidebar` | systems, zen, algo, terminal |
| `ConfirmModal` | zen, mcp |
| `Modal` | zen, algo |

The composable collisions are gone: the per-module `useTheme` copies went with
V3's suite-owned theming, `useTauriEvent` and the systems/zen `useShortcuts`
were deleted with their modules' old shells, and `useLogs` now exists only in
mcp. They are worth recording because of how one of them failed — QuantMCP's
`useTheme` exposed `init()`/`toggle()` while QuantSystems' exposed
`setTheme`/`toggleTheme`; mcp called it without an import, got systems'
version, and it only surfaced because the missing method happened to be a
*type* error. The component collisions above produce no error at all — without
the prefix rule the wrong sidebar simply renders.

Two fixes, both mandatory for every module:

1. **Components get a per-module prefix.** The layer declares
   `components: [{ path: <app/components>, prefix: '<Id>' }]`, so
   `components/Layout/RightSidebar.vue` registers as `SystemsLayoutRightSidebar`.
2. **Composables are imported explicitly**, `#<id>/composables/useX`, never
   relied on by auto-import.

### Pinia store ids collide too — found in Phase 4

Same failure mode as auto-imports, different registry. Pinia keys stores by id
in one global map, so `defineStore('app')` in two modules means the second
silently gets the first's state. Four modules had a bare `'app'`; `backtest`,
`canvas` and `workspaces` also repeated.

Ids are now `<module>/<store>` everywhere, enforced by
`scripts/check-module-isolation.mjs`. Safe to rename because nothing keys off
them — verified there is no persistence plugin and no string reference to a store
id anywhere in the suite.

### Module structure is not uniform

QuantCode keeps `composables/`, `lib/`, `shared/` and `stores/` *beside* `app/`,
not inside it. Its layer therefore declares two aliases — `#code` for `app/` and
`#code-root` for the module root — and the tooling has to look in both places.
Do not assume the shape of the next module.

### TypeScript strictness

`noUncheckedIndexedAccess` is disabled in `apps/shell/tsconfig.json`. Nuxt 4
turns it on; none of the migrated modules was written under it, and it produced
roughly twenty errors in `code` alone — all of the form "`arr[0]` is possibly
undefined" directly after a `length > 0` guard.

The alternative was scattering non-null assertions through code the migration is
not supposed to redesign. Same reasoning as the lint split below. Disabling it
also had a concrete payoff: it took `code` from ~20 errors to 3, and **all three
were real defects** that had been buried in the noise.

### Hoisting the Tailwind import silently disabled it for every module

This one caused the visible damage.

The shell issues the single `@import 'tailwindcss'` (§9). Tailwind v4 detects
which files to scan **relative to its own project**, and `modules/` sits outside
`apps/shell/`. So it scanned the shell, found no module templates, and generated
**zero utilities for module code**. No error, no warning.

QuantCode builds its entire layout from utilities — `flex`, `h-full`,
`absolute top-0 left-0 bottom-0`, `z-20`. Without them its root collapsed from
1144px to 412px and every panel stacked into whatever space was left. The
hand-written-CSS modules looked fine, which made it look like a QuantCode
problem rather than a build-configuration one.

```css
@import 'tailwindcss';
@source '../../../../../modules';
@source '../../../../../packages/ui';
```

`scripts/check-css-isolation.mjs` fails the build if the Tailwind import is
present without a `@source` covering `modules/`.

**Verification lesson.** An earlier check measured `.qss-module-root` and
reported it correct — it was. The module's own root inside it was 412px. Checking
a container without checking what it contains proves nothing; the geometry
assertions now compare parent and child.

### The one that made everything look broken

Module commands live behind `plugin:<id>|` (§2). **282 frontend call sites still
used the bare name** after six migrations — `invoke('read_file')` instead of
`invoke('plugin:code|read_file')`.

Nothing catches this. It fails per call, at runtime, with "command not found", so
a module does not error — it just sits there dead. This, not any CSS problem, is
why QuantCode appeared broken.

Alongside it, three ways component tags stopped resolving:

| Cause | Example |
|---|---|
| Nuxt collapses a directory segment the filename repeats | `Workspace/WorkspaceSwitcher.vue` is `WorkspaceSwitcher`, not `WorkspaceWorkspaceSwitcher` |
| Nuxt renders an all-caps directory in PascalCase | `UI/NotesBar.vue` is `UiNotesBar`; naive casing gives `UINotesBar` |
| Nuxt does not double a prefix the filename already carries | `McpFormModal.vue` under prefix `Mcp` stays `McpFormModal` |

Deriving these names by hand got all three wrong, which orphaned tags that had
been working. `scripts/check-component-resolution.mjs` therefore reads Nuxt's
generated `components.d.ts` — the authoritative registry — instead of guessing,
and `--fix` only rewrites when exactly one registered component matches.

It also surfaced tags that never resolved in the *standalone* apps: QuantAlgo's
`<EquityCurve>`, `<BalanceDisplay>`, `<TradeTable>`, `<BotTerminal>` and three
more were registered under path-prefixed names all along, so those pages have
been rendering nothing since before the migration.

### Lint policy

Migrated module crates carry style lints from apps that never ran clippy under
`-D warnings`, and rewriting module internals is out of scope. The split is
expressed in `Cargo.toml`, not on the command line:

- workspace root defines `[workspace.lints]` denying warnings and `clippy::all`;
- `qs-core`, `qs-mcp-bridge` and the binary opt in with `[lints] workspace = true`;
- module crates do not, so their warnings stay visible as debt.

`cargo clippy -- -D warnings` cannot express this: the flag also reaches
workspace path dependencies and would deny the module crates too.

### Tooling

Four scripts exist so these steps are not redone by hand for every module:

| Script | Does |
|---|---|
| `scripts/scope-module-css.mjs <id> <css> --write` | drops the Tailwind import, leaves `:root`/`[data-theme]` global, scopes everything else. Preserves comments outside selectors — an early version folded a `/* banner */` into a selector and produced a rule with no block, which lightningcss rejects as a dangling combinator |
| `scripts/fix-layer-collisions.mjs <id> --write` | prefixes the module's component tags and adds explicit composable imports. Rewrites tags **only inside `<template>`** — a first version corrupted TypeScript generics, turning `ref<Exchange[]>` into `ref<AlgoExchange[]>` because `Exchange` is both a component and a type |
| `scripts/gen-permissions.mjs <crate-dir>` | generates `permissions/default.toml` from the `COMMANDS` list in `build.rs` |
| `scripts/check-css-isolation.mjs` | **guard**: fails if any migrated module has a selector that is not scoped, `:root`, or `[data-theme]` — including inside `@media` |
| `scripts/check-module-isolation.mjs` | **guard**: un-namespaced or duplicated Pinia store ids, composables relied on by auto-import, missing component prefix, viewport units outside fixed-position overlays |

Both guards run in `npm run check` and both have been verified to fail on
injected violations — a guard that has never gone red is not a guard.

---

## 2. Rust Module Backends — Tauri Plugins

Each module's Rust side is a **Tauri v2 plugin**, not a set of free functions
registered on the root builder. Tauri namespaces plugin commands automatically as
`plugin:<name>|<command>`, which resolves the command collisions structurally
rather than by mass renaming.

```rust
// modules/systems/crate/src/lib.rs
use tauri::{plugin::{Builder, TauriPlugin}, Manager, Runtime};

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::<R>::new("systems")
        .invoke_handler(tauri::generate_handler![
            commands::list_systems,
            commands::run_backtest,
            commands::live_eval,
            commands::browse_universe,
            // ...
        ])
        .setup(|app, _api| {
            // Lazy: register state holders only. No engine start, no DB open.
            app.manage(SystemsState::deferred());
            Ok(())
        })
        .build()
}
```

```rust
// apps/src-tauri/src/lib.rs
tauri::Builder::default()
    .plugin(qs_core::init())              // bus, db, settings, process register, tray
    .plugin(qs_mod_systems::init())
    .plugin(qs_mod_zen::init())
    .plugin(qs_mod_algo::init())
    .plugin(qs_mod_mcp::init())
    .plugin(qs_mod_control::init())
    .plugin(qs_mod_canvas::init())
    .plugin(qs_mod_terminal::init())
    .plugin(qs_mod_console::init())
    .plugin(qs_mod_hud::init())
    .run(tauri::generate_context!())
    .expect("error while running QuantSuite");
```

`flow` is a Nuxt Layer with no crate, so it registers nothing here.

### Command inventory and collisions

274 commands exist across the eight apps today.

| App | Commands |
|---|---|
| QuantMCP | 74 |
| QuantZen | 54 |
| QuantHUD | 39 |
| QuantCode | 36 |
| QuantAlgo | 34 |
| QuantControl | 23 |
| QuantSystems | 14 |
| QuantView | 0 (Nitro-based — see §8) |

That table is the *pre-migration* inventory, and it is kept because it is what
the collisions below are a list of. What each plugin registers today, counted
from its `build.rs` — the list Tauri derives the permission identifiers from:

| Plugin | Commands |
|---|---|
| `mcp` | 76 |
| `notes` | 26 (was 54 before the Notion rewrite, PLAN-V3 §7a) |
| `hud` | 41 |
| `algo` | 35 |
| `canvas` | 34 declared, 27 real |
| `control` | 24 |
| `console` | 20 |
| `systems` | 14 |
| `terminal` | 7 |
| `qs` (core) | 27 |

Three of those rows need a word of explanation.

`canvas` declares seven commands it no longer has: `spawn_terminal`,
`write_terminal`, `resize_terminal`, `close_terminal`, `disconnect_terminal`,
`reconnect_terminal`, `check_terminal_alive`. They left the crate with P4.5, when
the terminals moved to `plugin:console`, but their names stayed in `build.rs` and
so their permission files are still generated. Harmless — a permission for a
command that does not exist grants nothing — but it is dead weight, and `build.rs`
is supposed to be the authoritative list.

`terminal`'s 7 are the market-data commands that replaced QuantView's six Nitro
routes, which is why it moved from 0 to 7 rather than staying Nitro-based (§7).

`console` is the module that was never an app: 20 commands covering session
lifecycle (`open_session`, `write_session`, `send_command`, `resize_session`,
`close_session`, `detach_session`, `attach_session`, `session_alive`,
`list_sessions`, `list_shells`), history and blocks (`history_status`,
`history_sessions`, `session_blocks`, `get_block`, `search_history`), the input
editor's `complete`, the agent one-shot `run_command`, and the workflow trio.

`run_command` is deliberately left **out** of its plugin's
`permissions/default.toml` — the worked example of the paragraph above. Arbitrary
shell execution offered to the agent layer gets two locks, and both have to be
open: the setting
`console / agent.exec.enabled`, which ships off, and the approval gate its
`external` classification earns it in the bridge (§7). Being outside the default
set is what makes the second lock meaningful — a command in the default set is
callable by the trusted webview without asking anyone.

Colliding names, all resolved by plugin namespacing:

| Command | Apps | Resolution |
|---|---|---|
| `read_file`, `write_file` | mcp, code | `plugin:mcp\|read_file`, `plugin:code\|read_file` |
| `clear_logs` | mcp, control | plugin-scoped |
| `run_backtest` | systems, algo | plugin-scoped |
| `get_app_settings`, `update_app_settings` | zen, systems | plugin-scoped |
| `get_settings` / `save_settings` / `update_settings` | control, algo | plugin-scoped |
| `check_for_update`, `download_and_install_update` | hud, mcp, code | **deduplicated into `core`** |

Suite-wide concerns move to `qs-core` and are removed from modules: updater,
file/folder dialogs, clipboard, notifications, theme, window management, tray.

### Two constraints Tauri imposes on plugins

Both were found by building Phase 0, and both bite at runtime rather than at
compile time.

**1. `core` is a reserved plugin name.** Registering a plugin called `core`
panics at startup with `ReservedName("core")` — Tauri owns that namespace for its
own built-ins. The core plugin is therefore named **`qs`**, and its commands are
invoked as `plugin:qs|get_settings`. Module plugins are unaffected; no module id
collides with a Tauri built-in.

**2. Plugin commands need declared permissions or they are denied.** A plugin
crate must:

- name its package `tauri-plugin-<name>` — Tauri derives the plugin name, and
  therefore the permission identifiers, from it;
- set `links = "tauri-plugin-<name>"` in `Cargo.toml`, which is what exposes the
  generated permission files to the app's build script. Without it the app fails
  to build with `Permission <name>:default not found`;
- run `tauri_plugin::Builder::new(COMMANDS).build()` in `build.rs`, listing every
  command;
- ship `permissions/default.toml` naming the `allow-*` entries that make up its
  default set;
- and be listed in the app's `capabilities/default.json` as `<name>:default`.

`qs-core` keeps its directory name and its `qs_core` library name — only the
Cargo *package* is `tauri-plugin-qs`. Module crates follow the same shape:
directory `modules/<id>/crate`, package `tauri-plugin-<id>`, library
`qs_mod_<id>`.

Not every command belongs in the default set. Anything genuinely privileged is
excluded there and granted explicitly where it is needed, rather than handed to
every webview. `qs` itself no longer starts anything (§6), so its default set is
read-only where processes are concerned: `process_list` and nothing more.

### Not every crate is a plugin — `crates/qs-pty`

`qs-pty` is a **plain library crate**. It has no `tauri` dependency, no `init()`,
no commands, and it is not registered anywhere in `apps/src-tauri`. It owns three
things and nothing else: spawning a shell on a real PTY (ConPTY on Windows),
getting its output out at a rate the UI survives, and surviving a frontend that
comes and goes (`detach` / `attach`).

Being framework-free is the point, not an accident of packaging. The caller hands
in a `PtySink` closure and decides where the bytes go, which means the two
threads, the coalescing and the detach buffer can be tested without an app
handle — and it is what let the same session first feed an xterm pane (bytes
straight to the webview) and then feed a VT parser (bytes into a block model)
without touching this layer.

It was extracted from `modules/canvas/crate/src/commands/terminal.rs`, and today
exactly one crate depends on it: `modules/console/crate`. That is the correction
worth making, because the extraction was done so that *two* modules could share
one PTY implementation and the sharing ended up happening one level higher.
QuantCanvas has no PTY code and no `qs-pty` dependency any more: its
`TerminalWindow.vue` opens a session through `plugin:console` and renders
`ConsolePane`. So the suite runs one PTY implementation *and* one emulator, and
a canvas terminal gets blocks, per-block copy, search and history for free.

The cost is recorded rather than hidden: `canvas` renders a component that
belongs to `console`. Nuxt registers components globally across layers so it
resolves, but it is a genuine cross-module dependency — the only one in the
suite — and the alternative was keeping a second block renderer alive.

The rule this generalises to: infrastructure that does not need a webview should
not become a plugin to be shared. A plugin is how a *webview* reaches Rust; a
library is how Rust reaches Rust.

### The block engine

The other reason `console` has a Rust side at all. `vt.rs` takes PTY bytes and
emits block deltas: Alacritty's `vte` provides the state machine for escape
sequence *syntax*, and everything the sequences mean is applied to a **document**
— a growing list of logical, unwrapped lines per block — rather than to a screen.

That choice is what makes the feature possible, and the failure it avoids is
specific. A block terminal has to remember where each command's output began and
ended. Expressed as row indices into a terminal buffer, those boundaries are
destroyed by the first window resize, because xterm's buffer is pinned to the
column count it was written at. Logical lines have no column count to be pinned
to: reflow happens in the DOM, and the plain text stays byte-exact for copy and
for the FTS index.

Two consequences follow directly:

- **The cursor cannot leave its block.** A real terminal lets a program address
  the whole screen; here `CUU` clamps at the block's first line. Programs that
  genuinely need the screen take the alternate screen (DECSET 1049), and those
  are handed to an xterm pane instead, with the block released afterwards. This
  is why `@xterm/*` is still a dependency — for the alt-screen case only, not
  for the normal one.
- **The marks are the open convention, not a private protocol.** `marks.rs`
  parses OSC 133 semantic prompts, OSC 7 / OSC 9;9 for the cwd and OSC 8 for
  hyperlinks, so hooks a user already has for WezTerm, kitty or VS Code keep
  working here. `vte`'s `ansi` feature is deliberately off: its `Handler` trait
  swallows unknown OSC sequences, and OSC 133 is exactly that. The bootstrap
  scripts that emit them are documented in `modules/console/shell/README.md`.

Known limitation, stated rather than hidden: every character counts as one
column, so a CJK or emoji glyph that a program redraws by cursor arithmetic can
misalign by the number of wide characters. Fixing it properly needs a width
table.

### Frontend access

`packages/core` provides typed wrappers so no component writes raw `invoke`:

```ts
// packages/core/src/commands.ts
import { invoke } from '@tauri-apps/api/core'

export const qs = {
  core: {
    checkForUpdate: () => invoke<UpdateInfo>('plugin:qs|check_for_update'),
    pickFolder: () => invoke<string | null>('plugin:qs|pick_folder'),
  },
  systems: {
    listSystems: () => invoke<SystemMeta[]>('plugin:systems|list_systems'),
    runBacktest: (req: BacktestRequest) =>
      invoke<BacktestResult>('plugin:systems|run_backtest', { req }),
  },
  // ... one namespace per module
}
```

---

## 3. Event Bus

`qs-core::bus` is the integration substrate. Rust owns it; webviews are clients.

### Envelope

```rust
#[derive(Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub topic: String,             // "<domain>.<entity>.<verb>"
    pub source: String,            // module id, or "core"
    pub ts: i64,                   // unix millis
    pub payload: serde_json::Value,
    pub correlation_id: Option<Uuid>,
}
```

### Delivery

1. In-process to Rust subscribers (module plugins register handlers in `setup`).
2. `app.emit("qs://event", &event)` to every webview.
3. Appended to `core.db.event_log` when `persist` is set on the topic.

Ordering is per-source FIFO. Delivery is best-effort and non-transactional — the
bus is for coordination, not for state transfer. Anything that must survive a
crash goes through `core.db` first and is announced on the bus afterwards.

### Topic catalog (v1)

| Topic | Source | Typical consumers | Payload |
|---|---|---|---|
| `core.theme.changed` | core | all | `{ theme }` |
| `core.module.focused` | core | all | `{ module }` |
| `core.entity.upserted` | core | palette, zen | `{ id, module, kind }` |
| `market.symbol.selected` | view, hud | view, algo, systems | `{ exchange, symbol, timeframe }` |
| `market.data.refreshed` | view | view, algo | `{ exchange, symbol, bars }` |
| `systems.engine.status` | systems | systems, control | `{ state, pid }` |
| `systems.backtest.completed` | systems | view, zen | `{ backtestId, system, metrics }` |
| `systems.signal.generated` | systems | algo, hud | `{ system, asset, score, forced }` |
| `algo.bot.started` / `algo.bot.stopped` | algo | hud, control, view | `{ strategyId }` |
| `algo.trade.executed` | algo | view, hud, zen | `{ tradeId, symbol, side, qty, price }` |
| `algo.backtest.completed` | algo | view, zen | `{ backtestId, metrics }` |
| `code.workspace.opened` | code | mcp, zen | `{ path, name }` |
| `mcp.server.started` / `mcp.server.stopped` | mcp | control, code | `{ mcpId, transport, port }` |
| `mcp.tool.invoked` | mcp | control | `{ tool, caller, approved }` |
| `zen.page.created` | zen | palette | `{ pageId, workspace, title }` |
| `hud.capture.taken` | hud | zen, view | `{ path, kind }` |

Naming rule: `<domain>.<entity>.<verb>`, verb in past tense for facts
(`completed`, `executed`) and present for state (`status`). Topics are additive;
never repurpose an existing one.

### Frontend client

```ts
// packages/core/src/bus.ts
const bus = useBus()

bus.on('systems.signal.generated', (e) => {
  algoStore.considerSignal(e.payload)
})

await bus.emit('market.symbol.selected', { exchange: 'binance', symbol: 'BTC/USDT' })
```

Subscriptions declared in `module.json` are enforced in dev: emitting an
undeclared topic logs a warning; subscribing to one nobody publishes fails the
build check.

---

## 4. Cross-Module Wiring

Wiring lives in the **subscribing** module, never in the shell. The shell knows
nothing about domain semantics.

```ts
// modules/algo/app/plugins/wiring.client.ts
export default defineNuxtPlugin(() => {
  const bus = useBus()
  const bots = useAlgoBotsStore()

  bus.on('systems.signal.generated', async ({ payload }) => {
    if (!bots.autoFollowSystems) return
    await bots.rotateTo(payload.asset, { reason: `systems:${payload.system}` })
  })
})
```

Rules:

- A module may subscribe to any topic. It may only publish topics it declares.
- Cross-module calls go through the bus or through `core.db`, never by importing
  another module's stores or composables directly.
- Auto-acting on another module's events is always behind a user-visible toggle.

---

## 5. Data Layer

Everything below lives under `~/.quantsuite/` (`qs_core::paths` is the single
source of every path). Debug builds use `~/.quantsuite-dev/` and their own app
identifier so `tauri dev` runs beside the installed suite; `QUANTSUITE_HOME`
overrides the root for either build.

### `core.db`

```sql
-- Anything addressable anywhere in the suite.
CREATE TABLE entities (
  id          TEXT PRIMARY KEY,   -- "algo:backtest:01HXYZ..."
  module      TEXT NOT NULL,
  kind        TEXT NOT NULL,      -- declared in module.json "entities"
  title       TEXT NOT NULL,
  subtitle    TEXT,
  route       TEXT NOT NULL,      -- "/algo/backtests/01HXYZ..."
  icon        TEXT,
  updated_at  INTEGER NOT NULL,
  payload     TEXT                -- JSON, module-defined, kept small
);
CREATE INDEX idx_entities_module_kind ON entities(module, kind);

-- Typed relations between entities of any module.
CREATE TABLE links (
  src        TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  dst        TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
  rel        TEXT NOT NULL,       -- "references" | "derived_from" | "annotates"
  created_at INTEGER NOT NULL,
  PRIMARY KEY (src, dst, rel)
);
CREATE INDEX idx_links_dst ON links(dst, rel);

-- Command palette index.
CREATE VIRTUAL TABLE entities_fts USING fts5(
  id UNINDEXED, title, subtitle, kind, module,
  content='entities', content_rowid='rowid'
);

CREATE TABLE event_log (
  id       TEXT PRIMARY KEY,
  topic    TEXT NOT NULL,
  source   TEXT NOT NULL,
  ts       INTEGER NOT NULL,
  payload  TEXT
);
CREATE INDEX idx_event_log_ts ON event_log(ts DESC);

CREATE TABLE settings (
  scope TEXT NOT NULL,            -- "core" | module id
  key   TEXT NOT NULL,
  value TEXT NOT NULL,            -- JSON
  PRIMARY KEY (scope, key)
);

CREATE TABLE schema_version (version INTEGER NOT NULL);
```

Entity IDs are `<module>:<kind>:<ulid>`. Modules own their real data; `entities`
holds only what is needed to find, name, link and route to a thing.

### Module databases

Each module keeps its own SQLite file under `~/.quantsuite/modules/<id>/`. No
module opens another module's database — cross-module reads go through the
owning module's commands or the bus.

#### `~/.quantsuite/modules/console/console.db`

The worked example of that rule, and the one whose contents someone will need to
reason about, because terminal history is the module data most likely to be
searched, exported or deleted:

| Table | Holds |
|---|---|
| `sessions` | one row per session: `shell`, `cwd`, `started_at`, `ended_at`, `integrated`, `tab_title`, `workspace` |
| `blocks` | one row per finished command: `command`, `cwd`, `started_at`, `duration_ms`, `exit_code`, `status`, `alt_screen`, `text`, `spans`, `truncated`, ordered by `(session_id, seq)` |
| `workflows` | saved parameterised commands: `name`, `command`, `params`, `tags`, `uses` |
| `blocks_fts` | FTS5 over `command` and `text`, kept in step by three triggers |

Four decisions in there are worth knowing before changing anything:

- **`text` is the substrate, stored once.** It is what a user copies and what the
  index searches, so `blocks_fts` is an *external content* table
  (`content='blocks'`). A contentless-or-duplicated index would double a
  `cargo build` block on disk for nothing.
- **FTS5 is optional at runtime.** The virtual table and its triggers are applied
  separately from the tables, because this is the one part that can legitimately
  fail: a SQLite built without FTS5. History search then falls back to `LIKE`
  over the same filter clause, so a filter cannot mean one thing in the fast path
  and another in the fallback.
- **Truncation is visible.** A block over the per-block text cap keeps its head
  and its tail with a marker line between them, and `truncated` is set on the row
  *and* on the restored block. A `cargo build --verbose` is tens of megabytes of
  one block; the choice is between a bounded lie the UI can explain and an
  unbounded database.
- **Migrations go through `PRAGMA user_version`.** `open` applies the steps above
  the stored version, so a later phase appends a branch instead of editing one
  that already shipped. `spans` is JSON today and the version is how a compact
  binary encoding arrives later without a rewrite.

Retention is enforced on this side, not in the frontend: `prune` runs during
plugin setup, before any webview exists, reading the caps the settings panel
wrote into `core.db`. A cap of `0` disables that limit rather than deleting
everything — the one mistake in this file that cannot be undone.

`console` declares three entity kinds in its manifest — `console_session`,
`console_block`, `console_workflow`. Note what that does and does not mean: the
kinds are declared, and no `entities` row is written for them today. Blocks live
in `console.db` and are reached through `plugin:console|session_blocks` and
`plugin:console|search_history`. That is the §5 rule working as intended rather
than a gap — `core.db` links, it does not own, and a row per block would put the
suite's palette index in charge of every line of terminal output.

### Migration from current paths

| Module | Current location | Action |
|---|---|---|
| code | `~/.quantcode` | copy into `~/.quantsuite/modules/code/` |
| zen | `~/.quantzen/workspaces.json` | copy registry; per-workspace DBs stay in place |
| systems | `~/.quantsystems` | copy |
| algo | `<data_dir>/quantalgo` | copy, incl. `strategies/`, `backtests/` |
| control | `<data_local_dir>` | copy `settings.json`, task/log DB |
| hud | `<config_dir>/quanthub` | copy config |
| mcp | `~/.config` + client config paths | copy own state; leave external client configs untouched |
| view | `quantview.db` in repo root | copy into module dir; **this path was a bug** |

Each importer runs once, is idempotent, writes a `.migrated` marker, and never
deletes the source. The old apps stay installable and working throughout.

### Secrets

API keys (exchange credentials, CMC key, model provider keys) move to the OS
keychain via `keyring`, with an encrypted file fallback. QuantControl's existing
`set_api_key` / `get_api_key_status` / `delete_api_key_cmd` becomes the core
implementation for the whole suite.

---

## 6. The Process Register

`qs-core::processes` is a **map of what the modules run, not an owner of it**.
Each module starts and stops its own process — it is the only party that knows
the protocol on those pipes — and announces what it started.

This replaced an owning supervisor (`SidecarSpec`, `RestartPolicy`, a log ring,
a reaper) that never had a single caller. The reason it never did is structural:
`systems` and `algo` need their child's `stdin`/`stdout` for JSON-RPC, so a log
pump would eat the answer channel; `mcp` runs a `tokio::process::Child` with
stdio on `null` and its own manager; `control`'s Docker containers are not child
processes of the app at all. A process whose pipes carry a protocol cannot be
owned by something that only knows how to tail them.

```rust
pub struct ProcessInfo {
    pub id: String,              // module-prefixed: "systems.engine"
    pub module: String,
    pub label: String,
    pub status: ProcessState,    // Stopped | Running { pid, since } | Failed { message }
    pub start_command: Option<String>,  // "plugin:systems|start_engine"
    pub stop_command: Option<String>,
    pub logs_command: Option<String>,
}
```

Modules call `announce` once at plugin setup (with `Stopped`, so the view can
start a process that is not running) and `mark_running` / `mark_stopped` /
`mark_failed` at the real transitions. The consumer reads `process_list` and
invokes the command names in each entry — start and stop take no arguments, logs
takes `{ limit }` and returns `string[]`; a `None` field means that button is not
rendered, because there is nothing honest to put behind it.

### Where the process centre lives

In **two** places, on purpose, and the split is the point.

`apps/shell/app/pages/processes.vue` is the process centre proper: a screen of
the shell's own, at `/processes`, that the rail's process button and the
dashboard's counter open (2026-08-26). It belongs to no module, which is the
whole reason it exists — P3.5 had made the button navigate into QuantConsole,
and pressing it while working in QuantTerminal cost you the terminal. A shell
page takes no module away: every stage stays warm behind it (`ModuleStage`).
Its layout follows what the screen is *for* — the register is a sidebar, the
selected process's log fills the rest, and only that one process is tailed per
poll.

QuantConsole keeps its read-only **session** (`/console?view=suite`,
`ConsoleSuiteSession`) unchanged: the same register, rendered as blocks beside
the shells, for when you are already in the console. Both read
`process_list` and invoke only the command names the entries carry; the log
text handling they share — ANSI stripping, the `
` progress-bar rule, the render
cap — is in `packages/core/src/logText.ts`, not in two copies.

Read-only is structural in both: no input line, no prefill, no `run_command`.
The only writes are the start/stop commands the modules named.

Three details that the move made explicit rather than introduced:

- **Some entries have to be asked.** `systems` and `algo` hold their child and
  report the moment it exits, so they name no `refresh_command`. The MCP servers
  and the Docker stack are only ever observed by asking, so they name one and it
  is invoked before each read. Without that, a process that died while the view
  was open would keep its "running" row until someone opened its module — the
  reported-not-observed property of the register (above), surfacing as a stale
  row.
- **Polling stops when nothing is looking.** Both halves matter: whether another
  tab of the module is on screen, and the V3 warm cache's
  `onActivated`/`onDeactivated` for when another *module* is on screen and this
  one is merely still mounted. A poll that keeps running means a `docker inspect`
  every two seconds for a view nobody is reading.
- **The log tail is not parsed.** Docker in particular answers with ANSI escapes
  in it, and this view renders plain spans, so the escapes are stripped rather
  than rendered as mojibake — and the UI says so instead of pretending the tail
  is styled output.

Two consequences are deliberate. There is **no restart policy**: a register
cannot relaunch a process whose pipes belong to someone else. And the state is
**reported, not observed** — a module that dies without calling `mark_stopped`
leaves the page claiming "running", so exit detection belongs in the owner, at
the place where it already learns of an exit (`systems` does it in its stdout
reader loop). Killing what a module started at shutdown is likewise the module's
own teardown hook (§10), not core's.

### The shared Python host

QuantSystems (`rotation_lab`) and QuantAlgo (`quantalgo`) both already speak
JSON-RPC over stdio to a spawned Python process. They collapse into one host
process with two registered namespaces:

```
sidecars/python/
├─ pyproject.toml
├─ host.py                  # JSON-RPC dispatcher, method prefix routing
├─ rotation_lab/            # from QuantSystems/engine
└─ quantalgo/               # from QuantAlgo/python
```

Requests are routed by prefix: `systems.run_backtest`, `algo.run_backtest`. One
interpreter, one dependency set (pandas/numpy are shared), one detection path
(QuantAlgo's `detect_python` becomes `plugin:qs|detect_python`).

Long-running work streams progress back as bus events rather than blocking the
RPC call.

### Docker

QuantControl's OpenClaw stack stays exactly as today — Docker Compose driven via
CLI (`docker_start`, `docker_stop`, `docker_status`, `docker_logs`,
`docker_restart`). Containers are not children of this app, so `control`
announces them to the register (with `pid: None`) and the suite knows they are
running without pretending to own them. Docker remains an optional
dependency: if it is absent, only the `control` module degrades.

---

## 7. MCP Bridge

`crates/qs-mcp-bridge` reads every `module.json`, collects `capabilities`, and
registers them with the QuantMCP host as tools named
`quantsuite.<module>.<name>`.

```
module.json capability          ->  MCP tool
{ name: "run_backtest",             quantsuite.systems.run_backtest
  command: "plugin:systems|run_backtest",
  sideEffects: "compute" }
```

`sideEffects` classifies the tool and drives gating:

| Value | Meaning | Default gate |
|---|---|---|
| `read` | no state change | auto-approve |
| `compute` | expensive, no external effect | auto-approve, rate-limited |
| `write` | changes local state | approval mode |
| `external` | network calls, orders, deploys | always prompt |

`external` covers exchange orders and bot deployment — those are never
auto-approved regardless of approval mode, and `algo.validate_bot_deploy` stays
in the path.

QuantMCP's existing approval machinery (`get_approval_mode`, `set_approval_mode`)
becomes suite-wide policy. Every invocation emits `mcp.tool.invoked`, so
QuantControl's mission-control view shows agent activity across all modules in
one timeline.

Result: agents in QuantControl and QuantCode can read market data, run backtests,
inspect systems, file notes and manage workspaces — the suite becomes
agent-operable without any module writing MCP-specific code.

### Connecting the AI clients (PLAN-QUANTMCP-CONNECT)

There is no client picker. `modules/mcp/crate/src/clients.rs` holds one table
(`CLIENTS: &[ClientSpec]`) — per client: how it is detected on the machine
(config file, directory, binary on PATH), how QuantMCP is written into it
(JSON file + server-map key + entry shape, the client's own CLI, a YAML list,
or manual) and which `clientInfo.name` values it reports. Detection, install,
the Connect report and the clientInfo → client match are derived from a row;
a new client is a new row. The user sees one **Connect** button (Settings, and
on every MCP card's Config) plus the standard snippet
(`mcpServers` → `{ "type": "http", "url" }`) for any client the table does not
know. Connect is idempotent and writes user scope only.

### What Phase 5 actually built

`modules/terminal/crate/src/market.rs` — **not** `crates/qs-market`. QuantAlgo was
left untouched by decision: extracting a shared crate from its exchange layer
requires proving identical backtest results first, which needs Python and a live
run. QuantView needed none of that, so it got its own layer now and the
unification waits for that gate.

Confirmed while porting: `better-sqlite3` was in QuantView's `package.json` but
referenced by no code, and `quantview.db` was never opened. There was no database
to migrate.

The port is verified against live upstream, not just compiled — `market.rs`
carries five `#[ignore]`d tests that hit Binance and CoinGecko:

```bash
cargo test -p tauri-plugin-view -- --ignored
```

They exist because a port like this compiles perfectly while silently mapping the
wrong field. They assert what the frontend depends on: candle timestamps in
seconds (CCXT returned millis), symbols still CCXT-style (`BTC/USDT`, not
`BTCUSDT`), `volume` a preformatted string, correlation symmetric with a unit
diagonal, market-cap page 1 starting at rank 1.

Two v3-era leftovers had to go with `@nuxtjs/tailwindcss`: the `@tailwind
base/components/utilities` directives (replaced by `@reference "tailwindcss"`, so
its `@apply` rules resolve against the shell's single import) and
`@nuxtjs/color-mode`, whose `.preference` API does not exist on VueUse's
`useColorMode`.

### Implementation status

`crates/qs-mcp-bridge` owns the **policy**, deliberately free of Tauri and of
QuantMCP internals so it can be tested on its own:

- the catalogue, built from every `module.json` via
  `modules/registry.generated.json`;
- `decide(capability, mode)` implementing the table above;
- unit tests asserting the invariants, including that no `external` capability
  is auto-approved in any mode.

It is reachable from the UI as `plugin:qs|agent_tools` and
`plugin:qs|agent_tool_decision`, so the policy is inspectable rather than
folklore.

**Not wired: the transport.** Serving this catalogue to real MCP clients means
registering the tools inside QuantMCP's `mcp_server.rs` and dispatching each call
back through Tauri's `invoke`, plus an approval UI for `Prompt` decisions. Until
that hook exists, no external agent can reach these tools — the catalogue and the
gate are in place, the door is not yet open.

---

## 8. Market Data — `qs-market`

### The QuantView problem

QuantView has **zero Tauri commands**. Its market data flows entirely through six
Nitro server routes:

```
server/api/{ohlcv,tickers,orderbook,marketcap,screener,correlation}.get.ts
```

A bundled Tauri app has no Nitro server, so these do not survive the move as-is.
Additionally `better-sqlite3` is a native Node module and cannot run in the
webview.

### Decision: port to Rust, reusing what QuantAlgo already built

The obvious first instinct is a Node sidecar preserving CCXT. Measurement says
otherwise.

**QuantView's CCXT surface is four methods, one exchange, no authentication**,
across 383 lines of TypeScript:

| Route | CCXT call | Exchange |
|---|---|---|
| `ohlcv.get.ts` | `fetchOHLCV` | binance |
| `correlation.get.ts` | `fetchOHLCV` | binance |
| `tickers.get.ts` | `fetchTickers`, `fetchOHLCV` | binance |
| `screener.get.ts` | `fetchTickers` | binance |
| `orderbook.get.ts` | `fetchOrderBook` | binance |
| `marketcap.get.ts` | *none* — CoinGecko / CoinMarketCap over plain HTTP | — |

**QuantAlgo already does this in Rust, across six exchanges.** Its `src-tauri`
uses `reqwest` against Binance, Bybit, OKX, Coinbase, Kraken and KuCoin for
klines and server-time sync, with `aes-gcm`, `ring`, `hmac` and `hex` present for
signed requests.

So the usual argument for CCXT — breadth of exchange coverage — is inverted here.
The existing Rust layer already has more coverage than QuantView's CCXT usage
consumes. CCXT is serving as a convenience wrapper around four public Binance
REST endpoints.

### `crates/qs-market`

QuantAlgo's exchange code is lifted into a shared crate rather than rewritten,
and QuantView becomes its second consumer.

```rust
pub trait MarketSource: Send + Sync {
    fn id(&self) -> &'static str;
    fn ohlcv(&self, req: OhlcvRequest) -> Result<Vec<Candle>>;
    fn tickers(&self, symbols: &[Symbol]) -> Result<Vec<Ticker>>;
    fn order_book(&self, symbol: &Symbol, depth: u32) -> Result<OrderBook>;
    fn server_time(&self) -> Result<i64>;
}

pub trait RankingSource: Send + Sync {          // CoinGecko, CoinMarketCap
    fn market_caps(&self, page: u32) -> Result<Vec<RankedCoin>>;
}
```

Responsibilities: symbol normalisation per venue (already implemented in
QuantAlgo — `"binance" => pair.replace('/', "")`), rate limiting, response
caching in `~/.quantsuite/modules/terminal/cache/`, and unified error mapping.

Net new work is small: `fetchTickers` maps to Binance `/api/v3/ticker/24hr` and
`fetchOrderBook` to `/api/v3/depth`. `fetchOHLCV` is the existing klines code.
`marketcap.get.ts` is plain HTTP and ports directly.

### What this buys

- No Node runtime in the bundle, and CCXT — among the largest npm dependencies in
  existence — is removed entirely.
- **One** market-data layer instead of two. Today QuantView has a TypeScript one
  and QuantAlgo a Rust one; QuantSystems' Python ranking registry is likely a
  third. All three collapse onto `qs-market`.
- Rate limiting and caching implemented once, shared by every module.
- One fewer supervised process, which is one fewer failure mode.

### The counterargument, honestly

QuantView's roadmap names **Open Interest** and **Liquidations**. CCXT offers
`fetchOpenInterest`; liquidation feeds are WebSocket-only in practice, and
WebSocket support is CCXT Pro, a paid product. So the roadmap argues only
partially for CCXT and would not deliver liquidation streams anyway.

Mitigation: `MarketSource` is a trait. If an exotic venue ever justifies it, a
CCXT-backed sidecar can be added as another implementation without any consumer
changing.

### `better-sqlite3`

Replaced by `rusqlite` in `qs-mod-view`. The schema is small and the access
patterns are simple. `quantview.db` also moves out of the repository root, where
it should never have been, into `~/.quantsuite/modules/terminal/terminal.db`.

`view` is scheduled last in the migration because it is the only module requiring
this architectural change.

---

## 9. CSS Isolation

Modules keep their existing stylesheets unchanged in appearance. That requires
work, because the eight stylesheets were each written assuming they own the
document.

### The collision

| Selector | Modules that define it |
|---|---|
| `html`, `body`, `#__nuxt` | all eight |
| `::-webkit-scrollbar*` | all eight |
| bare `a` | mcp, zen, systems, control |
| bare `button`, `input` | mcp, zen, systems |
| `@import 'tailwindcss'` | code, zen, systems, algo |
| `@theme { --color-* }` | code |

Loaded into one Nuxt application, the last stylesheet wins for every one of these
and the modules restyle each other. The `:root` custom properties are the one
part that is safe — `--qz-*`, `--qs-*`, `--qa-*`, `--qc-*` differ by prefix and
coexist without conflict.

### The fix: scope by module root

Every module route renders inside a container carrying its id:

```vue
<!-- apps/shell/app/app.vue -->
<div class="qs-module-root" :data-module="activeModule">
  <NuxtPage />
</div>
```

During a module's migration phase, its stylesheet is edited mechanically — about
ten selectors per file:

```css
/* before — modules/zen/app/assets/css/main.css */
html, body, #__nuxt { background: var(--qz-bg); color: var(--qz-text); }
a { color: var(--qz-accent); }
::-webkit-scrollbar { width: 10px; }

/* after */
[data-module="zen"] { background: var(--qz-bg); color: var(--qz-text); }
[data-module="zen"] a { color: var(--qz-accent); }
[data-module="zen"] ::-webkit-scrollbar { width: 10px; }
```

`:root` blocks stay exactly as they are. `[data-theme="light"]` blocks stay as
they are — they only set custom properties.

This is done by hand in the module's own phase rather than by a build-time
PostCSS plugin. Ten selectors are faster to edit than a plugin is to write, the
result is visible in the diff, and a plugin that silently mangles a selector is
much harder to debug than a wrong line of CSS.

### Tailwind

One `@import 'tailwindcss'` is hoisted into the shell's entry stylesheet and
removed from the four module files. Tailwind v4 scans sources automatically, so
every module's utility classes are still generated — from one pass instead of
four.

QuantCode's `@theme { --color-accent, --color-brand, ... }` block moves to the
shell's entry as well. It is global by design: it generates utility classes.
Other modules gaining unused `bg-brand` utilities is harmless.

Modules that use no Tailwind at all — `hud`, `mcp`, `control` — need only the
scoping edit.

### A module written inside the suite

Nine of the ten modules ship a stylesheet at
`modules/<id>/app/assets/css/main.css`; `flow` is a stub with no assets yet. The
scoping rule is unchanged for all of them — but `console` reaches it by
construction rather than by mechanical edit, because it never had a standalone
life in which it owned the document. Every rule in its file was written under
`[data-module="console"]` from the first line, it defines no tokens of its own
(the suite's `--qss-*` palette is the palette), and it imports no Tailwind.

Worth stating because the guard cannot tell the two cases apart, and the
temptation for a new module is to assume the guard is a migration artifact that
does not apply to it. It applies. An unscoped `.pane` or `.bar` in a new
module's stylesheet restyles every other module, and the failure is invisible
until someone compares screenshots.

The one place `console` legitimately needs a value that is not a suite token is
the configurable ANSI palette, and that is read by the pane in JS rather than
declared in CSS — so it is a setting, not a second set of module tokens.

### Verification

Appearance parity is checked per module at the end of its phase: same window
size, same route, screenshot before and after, compared. This is the acceptance
criterion that matters most, because a silent style regression is exactly the
failure mode this section exists to prevent.

---

## 10. Tray and Window Lifecycle

### The window model: one window

QuantSuite is **one window**. `main` holds every module; switching apps is a
route change, not a new window. The only exception is QuantHUD's always-on-top
overlay and its transparent helpers, which by their nature cannot live inside
another window.

Two earlier designs were tried and are recorded here so they are not tried
again:

- **Shell chrome plus each module's own titlebar.** Stacked a native titlebar,
  a suite titlebar and a 51px module titlebar, with a rail beside them. Apps
  built for a full window were squeezed into what was left.
- **A launcher Hub, one window per app.** Fixed the stacking but scattered the
  suite back into eight windows, which is the thing the suite exists to undo.

### Two shapes, one window

The window morphs rather than multiplying:

| | |
|---|---|
| **launcher** | 520×520 circle, transparent, fixed dead centre, not resizable, not draggable |
| **app** | maximised, resizable, running one module |

`useSuiteWindow` drives it; `apps/shell/app/pages/index.vue` draws the ring of
apps around the wordmark. Three things are easy to get wrong here:

**`transparent: true` is permanent.** The flag cannot be toggled at runtime, so
it is declared once and app mode simply paints an opaque page over the whole
surface. In launcher mode every ancestor from `<html>` down must stop painting
(`html.qss-launcher …` in `shell.css`) or the circle sits in a grey square.

**Never touch the window geometry on the first pass.** `tauri.conf.json`
already opens it as the circle. An earlier version hid and re-showed the window
during startup to suppress the resize animation — and the page came up
*rendering but deaf*: no click anywhere reached it, with no error. Skipping the
first transition entirely fixed it.

**Fade the page, do not hide the window.** Resizing a visible window is animated
by the OS. Hiding it across the change removes that, but blinks and flickers the
taskbar entry. Instead the page fades to `opacity: 0` — and a *transparent*
window painting nothing is an empty screen, so the resize is invisible however
the OS animates it. The window stays visible throughout.

**The order is the whole feature.** Opening a module is four steps and they do
not commute:

1. the module mounts behind a veil, still inside the 520px circle
2. wait until it has actually rendered — `moduleReady()` watches the DOM until
   the node count under `.qss-module-root` holds still for three frames and
   webfonts have loaded, capped at 15s
3. fade out; the screen goes empty
4. maximise, wait until the window *reports* it (`maximize()` returns when the
   request is queued, not when it has happened), then fade back in

Measured: veil at 40ms, module ready at 248ms, window swapped at 448ms — after
the fade completed — revealed at 494ms. In the binary: 520×520 → 1920×1152 in
256ms with `IsWindowVisible` true the whole way and no intermediate size.

**A round window needs a Win32 region, not `border-radius`.** CSS rounds only
what the page *paints*. The window stays a rectangle: its corners keep taking
clicks meant for the desktop, and with `transparent: true` they read as an
invisible pane. `qs_core::window::set_circular` does what QuantHUD does for its
notification popup — `CreateEllipticRgn` + `SetWindowRgn`, which changes the
window's shape at the OS level so outside it the window does not exist.

Two details it gets right and are easy to miss:

- `WS_THICKFRAME` and `WS_CAPTION` are stripped first. A borderless window still
  carries an *invisible* DWM resize border — measured at 14×8px here — which
  sits outside the client area and would offset the circle from where the page
  draws it. After stripping, window rect and client rect are both 520×520.
- The region is centred on the window rect **as it is when set**, so maximising
  leaves it off-centre. `swap()` re-applies it whenever the launcher look is
  still on.

**Never build a wait out of bare `requestAnimationFrame`.** It does not fire
while the window is hidden or minimised, so every loop built on it hangs
indefinitely — including the ones with their own deadline, because the deadline
is only checked inside the loop. Both waits race rAF against a 32ms timer.

App mode maximises rather than restoring a remembered size. That removes the
geometry-persistence problem entirely — there is no stored value for the
circle's 520×520 to leak into.

### The top row belongs to the app

There is exactly **one** bar on screen, and it is the **module's own**. The suite
does not draw a bar above it; it puts its window controls *into* it.

```
┌──────────────────────────────────────────────────────────────────────────┐
│ QUANTVIEW │ Dashboard QuantMetrics QuantTerminal …        ⚙   − □ ×     │  ← the module's own 51px header
└──────────────────────────────┬───────────────────────────────────────────┘
                          ╰────╯  ← the app switcher, at rest
```

A converted module does three things:

1. Keeps its original titlebar **unchanged** — logo, nav, search, status, all of
   it, at its original height.
2. Drops `<QWindowControls />` where its own minimize/maximize/close used to be.
   That component styles itself from `currentColor`, not suite tokens, so it
   looks native in eight differently-themed bars.
3. Carries `v-drag-window` on that bar, and a **Home** button in the one round
   action slot its own settings button used to occupy. Settings moves down to
   the sidebar footer as a full-width bar button.

Then it declares its chrome:

```json
"chrome": { "ownBar": true, "barHeight": 51, "homeInBar": true }
```

`ownBar: false` means the module has no titlebar at all — QuantControl was
natively decorated standalone — and the shell renders its own bar above it.
`homeInBar` retires the hanging switcher for that module.

The validator warns for every migrated module still missing it, which doubles as
the conversion checklist.

**The picker is the exception.** With no module open there is no module bar, so
the shell renders `QTitlebar` — a plain 44px bar with the brand, Ctrl+K and the
same `<QWindowControls />`. It exists only so the picker has something to drag
and close by.

**No emoji glyphs in chrome.** `&#9881;` has an emoji presentation on Windows:
the font stack renders it as a colour cog belonging to no theme. Chrome icons are
drawn SVGs inheriting `currentColor`. Where a module legitimately uses the
codepoint as a list glyph, append `︎` (VARIATION SELECTOR-15) to force the
text presentation.

### One shared control pattern

Eight apps meant eight ways to draw the same control. The suite defines the
shared ones **once**, in `apps/shell/app/assets/css/shell.css`, and the shape is
taken from the modules whose sidebars already read best — QuantMCP's
`.nav-item`, QuantSystems' `.qs-rail-btn`, QuantCode, QuantZen:

| Class | What it is |
|---|---|
| `.qss-foot` | sidebar footer: hairline above, `margin-top: auto` |
| `.qss-foot-action` | a row in it — icon + label, 9×12 padding, 8px radius, 10px gap |

Settings lives there in every module. The rows style themselves from
`currentColor` and `inherit` rather than suite tokens, so each takes on its own
module's palette — the same trick that makes `<QWindowControls />` look native
in eight differently-themed titlebars.

Do not write these as a `font:` shorthand. `inherit` is not a legal family in
the shorthand, so `font: 500 13px/1 inherit` is invalid and the browser drops the
whole declaration — which is how the rows silently kept each module's own size
and weight, 13px in one and 14px in the next.

### Theming

One theme for the suite, set by `apps/shell/app/plugins/theme.client.ts`.

The migration dropped `@nuxtjs/color-mode` from every layer on the grounds that
"theming is qs-core's job", and then nothing took the job. `useColorMode`
silently resolved to **VueUse's**, which defaults to `auto`; on a light system
nothing put `.dark` on `<html>` and QuantView sat on its `:root` light palette
permanently, no matter what its settings said.

Two rules follow:

- The plugin must use VueUse's **default storage key**. Modules call
  `useColorMode()` with no arguments and bind to that key; a custom one would
  create a second instance and the two would fight over the class on `<html>`.
- A theme class on `<html>` is an *ancestor* of `[data-module="…"]`, so the CSS
  scoper must leave `.dark` / `.light` global. Scoping them makes them
  unmatchable — which is exactly how the bug hid.

### Dragging: a directive, not `data-tauri-drag-region`

`v-drag-window` (`apps/shell/app/plugins/drag-window.client.ts`) is what moves
the window. The attribute does not work here: Tauri applies it only when the
element carrying it **is** the click target, and every module titlebar is fully
covered by child containers — logo section, nav section, actions section — so
the bar itself is never the target and the window never moves. The directive
walks up from the real target instead, skipping interactive descendants
(`button, a, input, select, textarea, label, [data-no-drag], [class*="no-drag"]`)
and treating a second click within 300ms as maximize.

Verified by driving the mouse: press on an empty stretch of QuantView's bar,
move 120×60px, release — the window rect moved by exactly 120×60.

Note that a **maximized** window cannot be moved at all; test on a restored one
before concluding dragging is broken.

### The app switcher

A rounded tab hanging from the bottom edge of the bar, centred. At rest 64×7px;
on hover it expands to 112×26 showing a grid icon and "Home", and clicking it
returns to the picker.

It hangs from `--qss-bar-h`, which the shell sets from the active module's
`chrome.barHeight` — the bar above it is the module's, and the shell cannot
guess its height.

Its collapsed height must include the hit padding: the reset sets
`box-sizing: border-box`, so `height: 7px` with `padding-bottom: 14px` leaves a
content box of zero and nothing renders.

### The shell layout is a flex column, not a grid

`.qss-app` is `display: flex; flex-direction: column`. It was a grid with
`grid-template-rows: 44px auto 1fr`; with the error banner absent, `<main>` fell
into the `auto` row and collapsed to content height — the module filled 545px of
a 720px window and the rest was dead space. Rows that can be mis-assigned are
not worth the tidiness.

### Four rules that fail silently if broken

**Window-creating commands must be `async`.** A synchronous `#[tauri::command]`
runs on the main thread; `WebviewWindowBuilder::build()` there hands the event
loop a task and waits for it, while the event loop waits for the command to
return. The app freezes — no error, no panic, no log entry. Every window command
in QuantHUD is async for this reason. `check-module-isolation.mjs` enforces it.

**Any command that blocks must be `async`, and high-rate events must be
batched.** Same root cause, wider blast radius. `#[tauri::command]` defaults to
`ExecutionContext::Blocking`, so a command that is not `async fn` runs **on the
main thread**, inline in the IPC handler — a `docker info`, a git spawn, an HTTP
call or a PTY write there is a frozen window, not a slow one. The same budget is
spent by `emit`: every event becomes an `EvaluateScript` message on the main
thread, *per webview*, so a per-line emit from a chatty child process saturates
the event loop just as effectively.

This is not theoretical. Release builds hung four times (Windows `AppHangB1` for
quantsuite.exe, "Keine Rückmeldung"); the trigger was QuantCanvas's PTY reader
emitting one event per 4 KB read. The fix pattern, in
`modules/canvas/crate/src/commands/terminal.rs` and `algo`'s `push_bot_log`: a
reader thread that only stages output, and a flusher that emits one coalesced
event per frame, addressed with `emit_to` rather than broadcast. All 128
blocking commands in the suite are now async (209 of 312 total; the other 103
read in-memory state and belong on the main thread); `check-module-isolation.mjs`
enforces the rule crate-wide and its ratchet is empty.

**Every window label must appear in `capabilities/default.json`.** A missing
label does not stop the window from opening; it denies every `invoke` inside it,
per call, at runtime. Guarded.

**Routes opened from Rust must exist.** Module pages are namespaced under
`/<id>`, so `WebviewUrl::App("/region-selector")` opens a window onto nothing —
transparent, empty, silent. Guarded.

### Tray residency

The suite lives in the notification area and closing a window does not quit it.

Both required pieces already exist in the source repositories and are lifted into
`qs-core` rather than written fresh:

- QuantMCP implements `WindowEvent::CloseRequested` → `api.prevent_close()` →
  hide, plus `TrayIconEvent::Click` with `MouseButton::Left` to restore.
- QuantHUD implements the tray menu (`MenuItem` "Show / Hide", "Quit") via
  `TrayIconBuilder`, and the `skip_taskbar(true)` / `visible: false` window
  pattern for its overlays.

### Behaviour

| Action | Result |
|---|---|
| Close the window (X or `Alt+F4`) | window hides; engines, bots, agents keep running |
| Left-click tray icon | toggle the window visible / hidden |
| Tray menu → Show / Hide | same as left-click |
| Tray menu → module name | show the window and route to that module |
| Tray menu → Quit | the only real exit: run module teardown (which stops their processes), flush DBs, exit |
| Global shortcut | toggle `hud`, independent of the suite window |

Closing the window while a bot is live must never kill the bot — that is the
entire reason for tray residency.

### Tray ownership

Exactly one tray icon for the suite, owned by `qs-core`. Today QuantHUD and
QuantMCP each register their own; both registrations are removed during their
phases. Modules contribute menu entries through a registry rather than building
their own tray:

```rust
qs_core::tray::register_entry(TrayEntry {
    module: "algo",
    label: "QuantAlgo",
    status: || bot_status_line(),   // rendered into the menu, e.g. "running · 3 positions"
    on_click: TrayAction::Route("/algo"),
});
```

### Quit path

`Quit` is the only exit. It must, in order: stop accepting new work, run every
module teardown hook — which is where a module kills the process it owns, since
nothing else holds its handle (§6) — flush `core.db` and all module databases,
persist window geometry, release global shortcuts, remove the tray icon. A
`qs-core::shutdown` sequence owns this; modules register teardown hooks and do
not call `std::process::exit` themselves.

QuantConsole's hook is the clearest case for why teardown belongs here and not on
window close: a shell survives a hidden window on purpose — a long build should
not die because someone closed the window — so the sessions are killed on Quit
instead. In a fused binary a leaked shell is much harder to notice than in a
single-purpose app, where it dies with the process it was the point of.

### How the shell decides what to render

`apps/shell/app/app.vue` branches on the **route**, not the window label:

- `/` → the app picker
- `/<module>/…` → the module's page, wrapped in `<NuxtLayout>` and a
  `data-module` attribute so its stylesheet scoping applies (§9)

Keying off the route rather than `getCurrentWindow().label` is deliberate: the
label is only readable inside Tauri, which would make the branch untestable in a
browser. The route carries the same information.

A consequence worth knowing: a module linking to `/` sends the window to the
picker. Six such links survived the migration from when `/` was each app's home;
they now point at `/<module>`.

### QuantHUD, the one module that builds its own window

It is a 340×900 always-on-top overlay plus three transparent full-screen helpers
(region selector, colour picker, screenshot preview) that Rust creates on demand.

**It is created lazily.** Standalone it was `main`, declared in
`tauri.conf.json`. Here it gets the label `hud` and is built on first use by
`plugin:hud|open_hud` — a suite opened to look at charts should not have an
always-on-top strip appear unasked.

**The Hub invokes it rather than opening a generic window.** `module.json`
declares `"ownWindow": { "command": "plugin:hud|open_hud" }`. A manifest field
rather than a special case in the shell, so the next own-window module needs no
shell change.

**Its frontend requires `withGlobalTauri`.** Every Tauri call in QuantHUD sits
behind `if (window.__TAURI__)`, with a localStorage fallback in the `else`. Its
own `tauri.conf.json` sets `"withGlobalTauri": true`; the suite's must too, or
the whole module silently runs in browser-fallback mode — config, notes,
clipboard history, calendar, shortcuts, colour picker and auto-hide all quietly
disabled, with nothing in the console.

Window-manipulating commands needed no edits: they take `window: WebviewWindow`,
and Tauri injects the *calling* window, which is already the HUD's.

### Windows note

On Windows 11 a newly registered tray icon lands in the **hidden icons overflow**
by default. The user drags it onto the taskbar to pin it. This is OS behaviour
and cannot be set programmatically — worth stating in the README so it does not
read as a bug on first launch.

### Autostart

Optional, off by default, toggled in suite settings. When enabled the app starts
minimised to tray, with `main` hidden — the same `visible: false` start QuantHUD
already uses.

---

## 11. Failure Isolation

One process means one crash surface. Mitigations, in order of effectiveness:

- **Heavy and risky work runs out-of-process.** Python engines, MCP servers, the
  Docker stack, the CCXT sidecar. A crash there is the owning module noticing an
  exit and reporting it (§6), not an application crash.
- **Rust module plugins are lazy.** `setup` registers state holders only. No
  database is opened, no engine started, no directory scanned until the module is
  first used. A broken module that is never opened cannot break the launch.
- **Per-module Vue error boundaries.** Each module route is wrapped in
  `<NuxtErrorBoundary>`; a render failure shows a module-scoped error panel with
  a reload action, leaving the rail and the other modules alive.
- **No shared mutable Rust state between modules.** Modules communicate by events
  and by their own databases.
- **Panic hook.** `qs-core` installs a hook that writes the panic to
  `~/.quantsuite/logs/panic.log` and emits `core.process.panicked` before
  unwinding, so a crash is diagnosable.
- **A log file.** `qs-core::diagnostics` installs a `log` sink writing to
  `~/.quantsuite/logs/quantsuite.log` (5 MB, one previous generation kept;
  level from `QS_LOG`, default `info`). Before it existed, every `log::info!`
  in the workspace went nowhere — the suite's four release hangs left no trace
  but the Windows event log.
- **A main-thread watchdog.** The same module pings the event loop every 2 s via
  `run_on_main_thread` and logs when the round trip exceeds 500 ms, or when it
  stops answering for 5 s — the threshold at which Windows itself declares the
  window hung. A hang now names its own start and end time in the log instead
  of having to be reconstructed from source.

Accepted residual risk: a panic in a module's Tauri command still aborts the
process. This is the real cost of full fusion, and it is why §6 pushes as much as
possible into sidecars.

---

## 12. Performance

Current duplication across the eight repos: Monaco three times, xterm twice,
lightweight-charts twice, TipTap once but heavy. CCXT is not deduplicated but
**removed** — see §8.

- **npm workspace** deduplicates node_modules to one copy each.
- **Route-level code splitting** is Nuxt's default. Monaco loads when `/canvas`,
  `/notes` or `/algo` is opened, not at boot. Enforced by a bundle-size check in
  CI with a per-route budget.
- **`optimizeDeps.include`** is merged from all layers; verify no layer forces a
  heavy dependency into the entry chunk.
- **Lazy Rust init** (§11) keeps cold start close to the current single-app times.
- **Cargo workspace** shares one `target/` directory. Note the existing repos
  carry large build artifacts (QuantCode's `target/release` alone holds dozens of
  cached asset bundles); these are not migrated.

Target: cold start to interactive shell under 1.5 s, module activation under
400 ms.

---

## 13. Build and Release

```bash
npm install                  # workspace-wide
npm run dev                      # shell on :1420, all layers hot-reloading
npm run tauri:dev                # full desktop app
npm run tauri:build              # single installer
npm run check                    # vue-tsc + cargo clippy + module.json validation
```

- One `tauri.conf.json`, identifier `com.quantable.quantsuite`,
  `frontendDist: ../shell/.output/public`, `devUrl: http://localhost:1420`.
- One application version in the root `package.json`, synced into
  `Cargo.toml` and `tauri.conf.json` by the existing `version:sync` script
  pattern.
- Modules keep their own `version` in `module.json` for changelog attribution
  only; it does not affect the build.
- The updater lives in `qs-core` — one channel, one signature key, replacing the
  three separate updater implementations in QuantHUD, QuantMCP and QuantCode.

---

## 14. Conventions

| Concern | Rule |
|---|---|
| Routes | `/<module>/...`, defined in `modules/<id>/app/pages/<id>/` |
| Pinia store id | `<module>/<store>` |
| Tauri command | `plugin:<module>\|<snake_case>` |
| Event topic | `<domain>.<entity>.<verb>` |
| Entity id | `<module>:<kind>:<ulid>` |
| CSS tokens | each module keeps its own prefix (`--qz-*`, `--qs-*`, …) unchanged |
| CSS selectors | no bare element selectors in a module stylesheet; scope under `[data-module="<id>"]` (§9) |
| Component names | `<Module><Name>` for module components; `packages/ui` is shell-only |
| Rust crate | `qs-mod-<id>` for modules, `qs-<name>` for infrastructure |
| Settings key | `settings(scope=<module>, key=<dot.path>)` |

---

## 15. Decisions

### Resolved

| # | Question | Decision |
|---|---|---|
| 1 | Nuxt version target | **Nuxt 4 for every module.** Latest across the board. Six of eight already use `srcDir: 'app'`, the Nuxt 4 default; QuantView is already on 4.1. Modules on 3.x are upgraded as part of their own phase, not in a separate sweep. |
| 2 | CCXT sidecar vs. Rust port for market data | **Rust, via `crates/qs-market`** — reusing QuantAlgo's existing `reqwest` exchange layer rather than writing a new one. Rationale and the counterargument in §8. `MarketSource` stays a trait so a CCXT sidecar remains addable later. |

### Open

| # | Question | Impact | Default if unresolved |
|---|---|---|---|
| 3 | Is Docker a hard dependency of the suite or only of `control`? | Installer, first-run experience | Only of `control`; degrade gracefully |
| 4 | Does `hud` stay a window of the main process, or a second binary? | Overlay responsiveness vs. simplicity | Same process, separate window |
| 5 | Bundle a Python runtime or require a system interpreter? | Installer size vs. setup friction | Require system Python; keep `detect_python`, prompt on first use |
| 6 | Do the eight source repos get archived after migration, or kept in sync? | Maintenance load | Archive per module once its phase is accepted |
| 7 | Does QuantSystems' Python ranking registry move onto `qs-market`'s `RankingSource`, or keep its own fetch path? | Removes a third market-data implementation, but touches the engine | Fold it in during Phase 5, once `qs-market` is proven |
