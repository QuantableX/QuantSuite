/**
 * Reading terminal output (card 13): the dev servers it announces, and
 * whether a shell sits at its prompt.
 */

const URL_RE = /https?:\/\/(?:localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\]):(\d{2,5})(?:\/[^\s"'<>]*)?/g
// eslint-disable-next-line no-control-regex
const ANSI_RE = /\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07]*(?:\x07|\x1b\\)/g

/** Servers a text mentions: `http://localhost:5173/` and friends. */
export function serverUrls(text: string): { port: number; url: string }[] {
  const out: { port: number; url: string }[] = []
  for (const m of text.replace(ANSI_RE, '').matchAll(URL_RE)) {
    const port = Number(m[1])
    if (port > 0 && port < 65536 && !out.some((s) => s.port === port)) out.push({ port, url: m[0].replace('0.0.0.0', 'localhost').replace(/[.,;)]+$/, '') })
  }
  return out
}

/** The last line of a terminal's output looks like a shell waiting at its prompt. */
export function atPrompt(tail: string): boolean {
  const lines = tail.replace(ANSI_RE, '').split(/\r?\n|\r/).map((l) => l.trimEnd()).filter(Boolean)
  const last = lines.at(-1) ?? ''
  return /(^PS .*>|[>$#%\u276f])$/.test(last)
}
