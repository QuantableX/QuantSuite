/**
 * Real terminal bells in a stream of PTY bytes.
 *
 * `\x07` is the bell — but it is also the classic terminator of OSC, DCS,
 * APC, PM and SOS control strings, and CLIs emit those all the time: the
 * window title (`ESC ] 0 ; … BEL`), shell-integration marks (OSC 133),
 * hyperlinks (OSC 8), clipboard (OSC 52). A `data.includes('\x07')` check
 * therefore "rings" on every title refresh, including while idle. This
 * scanner counts only bells outside such strings, and remembers whether a
 * chunk ended inside one so a terminator that lands in the next chunk is
 * still recognised.
 */

/** Where the scanner stopped inside the last chunk. */
export type BellScanState = 'text' | 'esc' | 'string' | 'string-esc'

const ESC = '\x1b'
const BEL = '\x07'
/** ESC introducers that open a control string terminated by BEL or ST. */
const STRING_OPENERS = new Set([']', 'P', '_', '^', 'X'])
/** The same introducers as C1 controls (OSC, DCS, APC, PM, SOS). */
const C1_OPENERS = new Set(['\u009d', '\u0090', '\u009f', '\u009e', '\u0098'])

/** Bells in `data` that are not control-string terminators, plus the state to carry. */
export function scanBells(data: string, state: BellScanState = 'text'): { bells: number; state: BellScanState } {
  let bells = 0
  for (const ch of data) {
    switch (state) {
      case 'text':
        if (ch === ESC) state = 'esc'
        else if (C1_OPENERS.has(ch)) state = 'string'
        else if (ch === BEL) bells += 1
        break
      case 'esc':
        state = STRING_OPENERS.has(ch) ? 'string' : 'text'
        break
      case 'string':
        if (ch === BEL) state = 'text'
        else if (ch === ESC) state = 'string-esc'
        break
      case 'string-esc':
        // `ESC \` is ST and ends the string; any other ESC sequence aborts it.
        if (ch === '\\') state = 'text'
        else state = STRING_OPENERS.has(ch) ? 'string' : 'text'
        break
    }
  }
  return { bells, state }
}
