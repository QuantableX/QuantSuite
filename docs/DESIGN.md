# QuantSuite — Design System

> ## UN-PARKED with V3 (2026-08-15) — partially shipped
>
> The 2026-08-11 "no new design" decision was reversed. What shipped:
>
> - **§2 Motion** — the duration/easing tokens live in `shell.css` as
>   `--qss-dur-*` / `--qss-ease-*` (incl. the `linear()` spring) and drive the
>   module selector, settings modal and Dashboard animations.
> - **§3.2 The Hub** — shipped as the **Dashboard** (`/`, the start app): live
>   app cards with status dots, metrics, sparkline, entrance stagger and the
>   selection bloom, ported from `prototypes/hub.html`. It is a page, not an
>   overlay; the rail and palette remain the switching layers.
> - **§6 components** — `QStatusDot`, `QMetric`, `QSparkline` exist in
>   `packages/ui`, alongside the V3 module chrome (`QModuleHeader`,
>   `QModuleSelector`, `QSidebar`, `QRightPanel`, `QSettingsModal`).
>
> **Not adopted:** §1 per-module accent colours — the suite stays monochrome
> (decision 2026-08-14); accents remain semantic only. §4's surface ramp was
> superseded by the `--qss-*` palette. Where prototype and implementation
> disagree, the implementation wins; see `PLAN-V3.md`.

---

Visual and motion language for the fused application. Read `../project.md` for
scope, `ARCHITECTURE.md` for the technical contracts.

**Scope note.** `project.md` lists "modules move, they are not redesigned" as a
non-goal, and that stands for module internals — redesigning eight applications
during their migration guarantees neither finishes. This document governs:

1. The **shell** — titlebar, module rail, Hub, command palette. Entirely new
   surface, so it gets full design investment immediately.
2. **`packages/design` and `packages/ui`** — the shared token and component
   layer. Modules inherit polish as they adopt shared components, which happens
   naturally during their migration phase.
3. An **optional per-module polish pass**, scheduled after a module's phase is
   accepted. Never a blocker for that phase.

---

## 1. Module Identity

Eight modules need to be distinguishable at a glance. The application chrome
stays monochrome; **colour belongs to the modules**, used in the rail, the Hub,
focus states and status indicators — never as page background.

| Module | Accent | Hex | Rationale |
|---|---|---|---|
| `view` | cyan | `#22d3ee` | market data, charts |
| `algo` | orange | `#f97316` | already QuantCode's `--color-brand`; trading |
| `systems` | violet | `#a78bfa` | quantitative research |
| `zen` | emerald | `#34d399` | calm, notes and canvas |
| `code` | blue | `#60a5fa` | development |
| `mcp` | amber | `#fbbf24` | tooling, connectors |
| `control` | rose | `#fb7185` | mission control, alerting |
| `hud` | slate | `#94a3b8` | overlay, deliberately neutral |

Each is declared in `module.json` as `"accent"` and exposed to the module's
subtree as `--q-accent-module`. All eight clear 4.5:1 against `--q-bg` (`#18181e`)
for text use and 3:1 for UI elements.

Rules:

- Accent is for state and identity: active rail item, focus ring, live status,
  selection, the module's own primary action.
- Never tint a page background, a card fill, or body text with it.
- Two accents never appear as peers in the same view. Cross-module surfaces
  (Hub, palette, event log) show each row in its own accent — that is a list, not
  a conflict.

---

## 1.1 Wordmarks — the module logos

Module identity in the shipped suite is carried by the PNG wordmarks, not by
accent colour (§1 was not adopted). They live in
`packages/ui/src/assets/logos/` and are resolved *only* through `logoFor()`
(`packages/ui/src/logos.ts`) — nobody hand-builds an asset URL.

### Asset spec — what a new logo PNG must be

Every mark in the folder follows this, and a new one has to match or it will
render at a different size than its neighbours:

| Property | Value | Why |
|---|---|---|
| Width | **1280px** | one export width for all marks |
| Height | whatever the ink is | the crop, not a fixed canvas |
| Crop | **tight to the ink**, zero padding | the intrinsic height then *is* the cap height |
| Background | transparent | rows and caps have their own surfaces |
| Colour | white / light monochrome | the suite is monochrome; the chrome tints nothing |
| Name | `Quant<Module>.png`, PascalCase | matches the `logo` key in `modules/apps.json` |

Verify a new file before registering it — the alpha bounding box must equal the
full image:

```bash
python -c "from PIL import Image;im=Image.open('QuantX.png').convert('RGBA');print(im.size, im.split()[3].getbbox())"
```

`(1280, 137) (0, 0, 1280, 137)` is correct. A bbox smaller than the image means
the export carries padding — re-crop it, or that mark will sit visibly smaller
than every other one.

### The sizing rule — height only, never width

Because the crops are tight, the intrinsic height already is the cap height.
So:

> **Size a wordmark by `height`. Never give it a `width`, and never a
> `max-width` that can bite.**

Height sizing → every module reads at the same letter size regardless of name
length. Width sizing → the name length decides the size: at the same box,
`QUANTTERMINAL` (10.24:1) comes out ~30% smaller than `QUANTZEN` (6.56:1).
That was the QModuleHeader bug fixed on 2026-08-25. A `max-width` that clamps
is the same bug in disguise: `object-fit: contain` then shrinks the *height*
too, so the long marks lose again.

The two heights are tokens in `shell.css`, and consumers read them with a
fallback so a module works outside the shell document:

| Token | Value | Used by |
|---|---|---|
| `--qss-wordmark-h` | `18px` | `QModuleHeader` left cap (`.qmh-logo`) |
| `--qss-wordmark-h-menu` | `14px` | `QRail` flyout rows, `QModuleSelector` rows |

`18px` is a ceiling, not a taste call: the widest mark (`QuantTerminal`,
10.24:1) renders 184px wide there and the 220px cap has a 188px content box.
Raising the token to 19px overflows it. If a future mark is wider than
10.24:1, either lower the token or widen the cap — do not reintroduce a width
clamp.

Differing *widths* between rows are correct and expected: a 13-letter wordmark
is simply wider than a 9-letter one. Equal cap height is the normalisation;
equal width is not.

### Registering a new module logo

1. Drop `QuantX.png` into `packages/ui/src/assets/logos/` (spec above).
2. Add it to `BY_NAME` in `packages/ui/src/logos.ts`, and to `BY_MODULE_ID`
   under the module id — `logoFor('x')` and `logoFor('QuantX')` must both hit.
3. Set `"logo": "QuantX"` in `modules/apps.json` if the mark is an *app*
   wordmark; module marks resolve by id and need nothing there.
4. Nothing else. `QModuleHeader`, `QRail` and `QModuleSelector` pick it up
   automatically — if you find yourself writing CSS for the new logo, the
   sizing rule above is being broken.

---

## 2. Motion

Motion exists to explain what happened, not to decorate. Every animation answers
"where did this come from and where did it go".

### Duration scale

| Token | Value | Use |
|---|---|---|
| `--q-dur-instant` | 90ms | state flips: toggle, checkbox, tab underline |
| `--q-dur-fast` | 140ms | hover, focus, tooltip |
| `--q-dur-base` | 220ms | dropdown, popover, panel slide |
| `--q-dur-entrance` | 320ms | modal, Hub open, sheet |
| `--q-dur-slow` | 460ms | route transition, shared-element move |

### Easing

```css
--q-ease-out:   cubic-bezier(0.32, 0.72, 0, 1);      /* default — decelerate */
--q-ease-in:    cubic-bezier(0.6, 0, 0.9, 0.3);      /* exits only */
--q-ease-inout: cubic-bezier(0.65, 0, 0.35, 1);      /* moves that start+end onscreen */
--q-ease-spring: linear(0, 0.006, 0.086, 0.235, 0.44, 0.65, 0.822, 0.937, 0.993, 1.017, 1.019, 1.01, 1.003, 0.999, 0.998, 1);
```

`--q-ease-spring` is a `linear()` easing approximating a critically-damped
spring. Use for anything that should feel physical: Hub cards, rail indicator,
drag release. Supported in all Tauri-relevant WebView versions.

### Hard rules

- **Animate `transform` and `opacity` only.** Never `width`, `height`, `top`,
  `left`, `margin` — they force layout on every frame. Use `scale` and
  `translate`; for size changes that must be real, use a wrapper with
  `grid-template-rows: 0fr → 1fr`.
- **Stagger is 30–40ms per item, capped at 8 items.** Beyond that the last item
  arrives late enough to feel broken; items 9+ share the final delay.
- **Exits are faster than entrances**, typically 60% of the duration. Waiting for
  something to leave is the most common source of "sluggish".
- **Nothing animates on first paint.** The shell adds `data-ready` after mount;
  entrance animations are gated on it. Otherwise every cold start looks like a
  loading screen.
- **`prefers-reduced-motion: reduce` collapses all durations to 1ms** and
  disables transforms, keeping opacity changes. This is enforced globally in
  `base.css`, not per component.

```css
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after {
    animation-duration: 1ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 1ms !important;
    scroll-behavior: auto !important;
  }
}
```

### Performance budget

Every animated surface holds 60fps on integrated graphics. `will-change` is set
only for the duration of an interaction and removed afterwards — permanent
`will-change` on many elements exhausts compositor memory and makes things
slower, not faster.

---

## 3. The Three Switching Layers

Different intents deserve different mechanisms. All three are shell-owned.

| Layer | Trigger | Intent | Cost |
|---|---|---|---|
| **Module Rail** | click, `Ctrl+1…8` | "go there now", muscle memory | zero — always visible |
| **The Hub** | `Ctrl+Space`, logo click | "what's going on, then decide" | full-screen overlay |
| **Command Palette** | `Ctrl+K` | "find this specific thing" | overlay, keyboard-only |

### 3.1 Module Rail

56px, left edge, always visible. Icon per module, no labels — a tooltip appears
after 400ms hover.

- **Active indicator**: a 2px accent bar on the left edge of the item. It does not
  fade in and out between items — it *slides*, using `--q-ease-spring` over
  `--q-dur-base`. That single detail communicates "same rail, different position"
  better than any crossfade.
- **Live status dot**: bottom-right of the icon, in the module's accent. Solid for
  running, slow pulse for working, rose for error, absent for idle. Driven by bus
  events, so it is real state rather than decoration.
- **Hover**: icon lifts 1px, tint moves from `--q-text-muted` toward the module
  accent over `--q-dur-fast`.

### 3.2 The Hub

The centrepiece. Not a launcher grid — a **mission-control view that doubles as a
switcher**. Opening it should answer "what is my suite doing right now" before
you have decided where to go.

Each card carries live state pulled from the bus and `core.db`:

| Module | Live content on its card |
|---|---|
| `view` | tracked symbol, price, 24h change, sparkline |
| `algo` | bot running/stopped, open positions, today's P&L, equity sparkline |
| `systems` | engine status, last backtest metric, forced-rotation count |
| `zen` | active workspace, recently edited page |
| `code` | open workspaces, live agent sessions |
| `mcp` | running servers of total, last tool invocation |
| `control` | active agents, running tasks, queue depth |
| `hud` | attached/detached, last capture |

**Open sequence** (total ≈360ms, feels instant):

1. Backdrop: `opacity 0→1`, `backdrop-filter: blur(0→24px)`, 260ms `--q-ease-out`.
2. Grid: `scale(0.96→1)`, 320ms `--q-ease-spring`.
3. Cards: stagger 35ms; each `translateY(16px→0)`, `scale(0.97→1)`,
   `opacity 0→1`, 320ms `--q-ease-spring`.

**Hover**: card lifts 4px, border goes to the module accent, a soft accent glow
appears, and the card's sparkline redraws. 180ms.

**Select**: the chosen card scales up slightly and its accent floods the border
while every other card drops to `opacity 0` and `scale(0.97)`. The backdrop
unblurs. The module route mounts behind it. ~280ms, and it reads as *entering*
the module rather than dismissing a dialog.

**Keyboard**: arrow keys move focus across the grid, `Enter` selects, `Esc`
closes, `Ctrl+1…8` jumps directly and skips the overlay entirely. Focus is
trapped while open and restored on close.

**Reduced motion**: cards appear with opacity only, no stagger, no transform.

### 3.3 Command Palette

`Ctrl+K`. Searches `entities_fts` across every module — symbols, strategies,
systems, pages, boards, canvases, agents, backtests, workspaces, files.

- Each result shows its module's accent as a 2px leading bar, so the source is
  readable without a label.
- Grouped by module, ordered by recency within group.
- Results do not animate on keystroke — list reordering during typing is
  distracting and costs frames. Only the panel itself animates, on open and
  close.

---

## 4. Surfaces and Depth

Modernising the current flat monochrome means adding legible depth, not shadows
everywhere.

```css
--q-surface-0: #18181e;   /* app background */
--q-surface-1: #1f1f25;   /* rail, titlebar, sidebar */
--q-surface-2: #24242c;   /* cards, panels */
--q-surface-3: #2b2b34;   /* raised: popover, dropdown, hovered card */
--q-surface-4: #33333d;   /* modal, Hub card */

--q-border:        #2e2e38;   /* default hairline */
--q-border-strong: #3d3d49;   /* emphasised division */

--q-shadow-sm: 0 1px 2px rgb(0 0 0 / 0.24);
--q-shadow-md: 0 4px 12px rgb(0 0 0 / 0.32);
--q-shadow-lg: 0 16px 48px rgb(0 0 0 / 0.44);
--q-glow: 0 0 0 1px var(--q-accent-module), 0 8px 32px -8px var(--q-accent-module);
```

Note these borders are darker than the current `#47474f`, which reads as heavy at
this surface contrast. Hairlines separate; they should not draw attention.

Overlays use `backdrop-filter: blur(24px) saturate(140%)` over
`rgb(11 11 15 / 0.72)`. Blur is expensive — it is used on the Hub, the palette
and modals only, never on a scrolling surface.

Light theme mirrors the ramp inverted; both are defined in `tokens.css` and both
must be checked before any component ships.

---

## 5. Typography

```css
--q-font-sans: "Inter var", -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
--q-font-mono: "JetBrains Mono", "Cascadia Code", ui-monospace, monospace;

--q-text-xs:   11px / 16px;   /* status, badges, table meta */
--q-text-sm:   12px / 18px;   /* dense UI, sidebars, labels */
--q-text-base: 13px / 20px;   /* body — desktop-dense, not web-default 16px */
--q-text-lg:   15px / 22px;   /* section headings */
--q-text-xl:   19px / 26px;   /* page titles */
--q-text-2xl:  26px / 32px;   /* Hub headings */
```

Numbers in tables, prices, metrics and P&L use
`font-variant-numeric: tabular-nums` without exception — proportional digits make
a live-updating price column jitter on every tick.

---

## 6. Component Additions to `packages/ui`

Beyond the Phase 0 inventory:

| Component | Notes |
|---|---|
| `QHub` | the overlay described in §3.2 |
| `QHubCard` | slot-based; each module supplies its own live content |
| `QRailItem` | icon, tooltip, status dot, shared sliding indicator |
| `QSparkline` | SVG, animated draw via `stroke-dasharray`, tabular value label |
| `QStatusDot` | idle / working / running / error, accent-aware, pulse on working |
| `QMetric` | label + tabular value + delta, used across every module's Hub card |
| `QSkeleton` | shimmer placeholder; replaces spinners for known-shape content |
| `QToast` | bus-driven notifications, module accent as leading bar |
| `QTransitionRoute` | wraps `<NuxtPage>`, handles the Hub → module handoff |

Every component ships with both themes verified and a reduced-motion path.

---

## 7. Accessibility

Not optional, and cheap if done from the start.

- Focus is always visible: 2px accent ring, 2px offset. Never `outline: none`
  without a replacement.
- Hub and palette trap focus while open and restore it to the trigger on close.
- Status is never communicated by colour alone — the status dot carries a shape
  or an adjacent label, so a red and a green dot are distinguishable without
  colour vision.
- All eight module accents verified at 4.5:1 for text and 3:1 for UI against both
  themes.
- `Esc` closes any overlay. `Tab` order follows visual order.

---

## 8. Prototype

`docs/prototypes/hub.html` is a working, self-contained prototype of the rail,
the Hub and the palette — real animations, real keyboard handling, fake data.
Open it in a browser.

It is the reference for the Vue implementation, not throwaway. When the Hub is
built in Phase 0, behaviour is ported from it directly; where implementation and
prototype disagree, the prototype is updated rather than left to rot.

---

## 9. Implementation Order

| When | Work |
|---|---|
| Phase 0 | tokens, motion system, rail, Hub, palette, `QSparkline`, `QStatusDot`, `QMetric` |
| Each module phase | that module's Hub card with real live data; adopt shared `packages/ui` components where it already had an equivalent |
| After a module's acceptance | optional polish pass on its internals — explicitly not a blocker |
