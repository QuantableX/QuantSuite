/**
 * The one logo lookup for suite chrome (V3). Rail, module header, module
 * selector and dashboard all resolve PNGs through here — nobody hand-builds
 * asset URLs.
 *
 * Keys are both the `logo` values from modules/apps.json (file basenames) and
 * module ids, so `logoFor(module.id)` works everywhere a module is rendered.
 * `suite-q` (the dashboard) has no PNG — it is drawn as the serif "Q" mark in
 * `--qss-font-brand`; callers get `undefined` and render the glyph.
 *
 * ── Adding a new module wordmark (full spec: docs/DESIGN.md §1.1) ──────────
 *
 * 1. The PNG must be 1280px wide, cropped TIGHT to the ink (no padding),
 *    transparent, light monochrome, named `Quant<Module>.png`. Check it:
 *      python -c "from PIL import Image;im=Image.open('QuantX.png').convert('RGBA');print(im.size, im.split()[3].getbbox())"
 *    The alpha bbox must equal the full image — padding makes the mark render
 *    smaller than its neighbours.
 * 2. Drop it in ./assets/logos/, add it to BY_NAME, and add the module id to
 *    BY_MODULE_ID.
 * 3. Do NOT write CSS for it. Consumers size wordmarks by HEIGHT only
 *    (--qss-wordmark-h / --qss-wordmark-h-menu); a width or a biting
 *    max-width makes the letter size depend on how long the name is.
 */

const asset = (name: string) => new URL(`./assets/logos/${name}.png`, import.meta.url).href

const BY_NAME: Record<string, string> = {
  QuantSuite: asset('QuantSuite'),
  QuantView: asset('QuantView'),
  QuantAgent: asset('QuantAgent'),
  QuantCode: asset('QuantCode'),
  QuantTerminal: asset('QuantTerminal'),
  QuantSystems: asset('QuantSystems'),
  QuantAlgo: asset('QuantAlgo'),
  QuantZen: asset('QuantZen'),
  QuantNotes: asset('QuantNotes'),
  QuantPlan: asset('QuantPlan'),
  QuantHabit: asset('QuantHabit'),
  QuantFinance: asset('QuantFinance'),
  QuantMemory: asset('QuantMemory'),
  QuantScript: asset('QuantScript'),
  QuantPilot: asset('QuantPilot'),
  QuantMCP: asset('QuantMCP'),
  QuantCanvas: asset('QuantCanvas'),
  QuantConsole: asset('QuantConsole'),
  QuantFlow: asset('QuantFlow'),
  QuantHUD: asset('QuantHUD'),
}

const BY_MODULE_ID: Record<string, string> = {
  terminal: BY_NAME.QuantTerminal!,
  systems: BY_NAME.QuantAlgo!,
  algo: BY_NAME.QuantAlgo!,
  notes: BY_NAME.QuantNotes!,
  plan: BY_NAME.QuantPlan!,
  habit: BY_NAME.QuantHabit!,
  finance: BY_NAME.QuantFinance!,
  memory: BY_NAME.QuantMemory!,
  script: BY_NAME.QuantScript!,
  pilot: BY_NAME.QuantPilot!,
  mcp: BY_NAME.QuantMCP!,
  canvas: BY_NAME.QuantCanvas!,
  code: BY_NAME.QuantCode!,
  console: BY_NAME.QuantConsole!,
  flow: BY_NAME.QuantFlow!,
  hud: BY_NAME.QuantHUD!,
}

/** Resolve a logo URL by apps.json `logo` key, module id, or module title. */
export function logoFor(key: string): string | undefined {
  return BY_NAME[key] ?? BY_MODULE_ID[key]
}
