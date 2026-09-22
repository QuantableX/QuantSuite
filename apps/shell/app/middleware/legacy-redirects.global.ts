/**
 * V3 rename (2026-08): `view` -> `terminal`. (`code` -> `canvas` was here too;
 * it is gone — `/code` is QuantCode's own route since 2026-08-26.)
 * QuantZen split (2026-08-25): `zen` -> `notes` — QuantZen became the app one
 * level up, the module is QuantNotes. Sub-paths map 1:1.
 *
 * Persisted routes are rewritten by the one-time core.db migration in
 * `qs-core`, but anything that slips through (stale localStorage, old links,
 * muscle memory in the dev URL bar) lands here instead of on a 404.
 */
export default defineNuxtRouteMiddleware((to) => {
  if (to.path === '/systems' || to.path.startsWith('/systems/')) {
    return navigateTo({ path: to.path.replace(/^\/systems/, '/algo/manual'), query: to.query, hash: to.hash }, { redirectCode: 301 })
  }
  if (to.path === '/plan' || to.path.startsWith('/plan/')) {
    return navigateTo({ path: to.path.replace(/^\/plan/, '/flow'), query: to.query, hash: to.hash }, { redirectCode: 301 })
  }
  if (to.path === '/habit' || to.path.startsWith('/habit/')) {
    return navigateTo({ path: to.path.replace(/^\/habit/, '/flow/habits'), query: to.query, hash: to.hash }, { redirectCode: 301 })
  }
  // QuantControl (Hermes mission control) was removed on 2026-09-02;
  // QuantPilot took its slot in QuantAgent (docs/PLAN-QUANTPILOT.md).
  if (to.path === '/control' || to.path.startsWith('/control/')) {
    return navigateTo({ path: '/pilot', query: to.query }, { redirectCode: 301 })
  }

  if (to.path === '/zen' || to.path.startsWith('/zen/')) {
    return navigateTo({ path: to.path.replace(/^\/zen/, '/notes'), query: to.query }, { redirectCode: 301 })
  }

  // `/code` used to redirect to `/canvas` — the tail of the V3 rename. It is
  // GONE and must not come back: since 2026-08-26 `/code` is QuantCode, a real
  // module of its own (docs/PLAN-QUANTSPACE.md), and a redirect here would make
  // it unreachable. Persisted routes were rewritten once in core.db back then.

  // The Overview page was removed on 2026-08-16; persisted stage routes and
  // bookmarks still point at it.
  if (to.path === '/terminal/overview') {
    return navigateTo({ path: '/terminal', query: to.query }, { redirectCode: 301 })
  }
  if (to.path === '/view' || to.path.startsWith('/view/')) {
    // The old market dashboard lived at `/view`, then `/terminal/overview`.
    // That page is gone (2026-08-16) — its market cap ladder is the terminal's
    // left sidebar now — so both spellings land on `/terminal`.
    const rest = to.path.slice('/view'.length)
    const mapped =
      rest === '' || rest === '/' || rest === '/overview'
        ? '/terminal'
        : rest === '/quantterminal'
          ? '/terminal'
          : rest === '/algo' || rest === '/research'
            // The old in-view algo and research pages are gone entirely —
            // algo lives at /algo, sentiment moved into /terminal/metrics.
            ? '/terminal'
            : rest === '/terminal'
              ? '/terminal'
              : `/terminal${rest}`
    return navigateTo({ path: mapped, query: to.query }, { redirectCode: 301 })
  }
})
