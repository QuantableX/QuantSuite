/**
 * Turning a module's raw log chunks into text a page can render.
 *
 * Shared because there are two readers of the same bytes: QuantConsole's suite
 * session (`ConsoleSuiteSession`) and the shell's process page. The escape
 * handling below is subtle enough that two copies of it would drift, and the
 * drift would be invisible — one view rendering mojibake the other strips.
 *
 * Note what this is *not*: a VT parser. Nothing here renders colour. A log tail
 * shown as plain text must not paint escape codes as characters, so they are
 * removed; a view that wants styled output needs a terminal, not this.
 */

/** Rendered lines per tail. A module that ignores `limit` and hands back its
 * whole ring buffer must not be able to put 200 000 nodes in the DOM. */
export const LOG_TAIL_CAP = 2000

/**
 * ANSI/VT escapes, in the four shapes that actually turn up in a log tail: CSI
 * (`ESC [` … a final byte — colours, cursor moves, line erases), OSC terminated
 * by BEL or ST (window titles, OSC 8 links), the string sequences DCS/PM/APC,
 * and the two-character escapes (`ESC ( B`, charset selects).
 *
 * Every shape has an unterminated twin, and those are not paranoia: a tail is
 * cut mid-stream, so its first line routinely begins inside a sequence. Without
 * them the parameters leak as text, and a half-written CSI puts a stray digit in
 * the middle of a log line. The text is split into lines before this runs, so
 * `$` is the end of one line.
 */
const ANSI_ESCAPE =
  /\u001B(?:\[[0-?]*[ -/]*[@-~]|\[[0-?]*[ -/]*$|[\]P^_][\s\S]*?(?:\u0007|\u001B\\)|[\]P^_][\s\S]*$|[ -/]*[0-~])/g

/** Everything else unprintable. Tab survives — it is layout the process meant. */
const CONTROL_CHARS = /[\u0000-\u0008\u000B-\u001F\u007F]/g

/**
 * One log line as a terminal would be showing it, without being one.
 *
 * The carriage returns are the interesting half: `docker pull` and `pip` draw
 * progress by returning to the start of the line, so the raw text of one logical
 * line holds every intermediate state. Keeping the segment after the last `\r`
 * is the honest approximation — the alternative pastes forty copies of a
 * progress bar into a single unreadable line.
 */
export function plainLogLine(line: string): string {
  const stripped = line.replace(ANSI_ESCAPE, '')
  const rewrite = stripped.lastIndexOf('\r')
  const visible = rewrite === -1 ? stripped : stripped.slice(rewrite + 1)
  return visible.replace(CONTROL_CHARS, '')
}

/**
 * A module's answer as renderable lines.
 *
 * `string[]` by contract, but the entries are not reliably single lines — a
 * module that forwards read chunks (docker does) puts twenty in one — so they
 * are split again here. O(lines) per poll.
 */
export function toLogLines(
  raw: string[],
  cap: number = LOG_TAIL_CAP
): { lines: string[]; truncated: boolean } {
  const text: string[] = []
  for (const chunk of raw) for (const piece of chunk.split(/\r?\n/)) text.push(piece)
  // A trailing newline in the last chunk is punctuation, not an empty line.
  while (text.length && text[text.length - 1] === '') text.pop()

  const truncated = text.length > cap
  const kept = truncated ? text.slice(text.length - cap) : text
  return { lines: kept.map((line) => plainLogLine(line)), truncated }
}
