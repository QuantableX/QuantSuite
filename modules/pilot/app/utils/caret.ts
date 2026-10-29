/**
 * Which visual line of a textarea the caret is on — wrapped lines count —
 * measured with an off-screen copy of the field (the classic mirror-div
 * technique). Prompt recall uses it: ArrowUp only recalls from the first
 * visual line, ArrowDown only from the last.
 */
const COPIED = [
  'boxSizing',
  'width',
  'borderTopWidth',
  'borderRightWidth',
  'borderBottomWidth',
  'borderLeftWidth',
  'paddingTop',
  'paddingRight',
  'paddingBottom',
  'paddingLeft',
  'fontStyle',
  'fontVariant',
  'fontWeight',
  'fontStretch',
  'fontSize',
  'lineHeight',
  'fontFamily',
  'textAlign',
  'textIndent',
  'letterSpacing',
  'wordSpacing',
  'tabSize',
] as const

function markerTop(ta: HTMLTextAreaElement, pos: number): number {
  const style = getComputedStyle(ta)
  const div = document.createElement('div')
  for (const p of COPIED) div.style[p] = style[p]
  div.style.position = 'absolute'
  div.style.visibility = 'hidden'
  div.style.top = '0'
  div.style.left = '-9999px'
  div.style.whiteSpace = 'pre-wrap'
  div.style.overflowWrap = 'break-word'
  div.style.height = 'auto'
  div.textContent = ta.value.slice(0, pos)
  const marker = document.createElement('span')
  marker.textContent = '​'
  div.appendChild(marker)
  document.body.appendChild(div)
  const top = marker.offsetTop
  div.remove()
  return top
}

export function caretLine(ta: HTMLTextAreaElement): { first: boolean; last: boolean } {
  const pos = ta.selectionStart
  const lh = parseFloat(getComputedStyle(ta).lineHeight) || 20
  const here = markerTop(ta, pos)
  const start = markerTop(ta, 0)
  const end = markerTop(ta, ta.value.length)
  return { first: here - start < lh / 2, last: end - here < lh / 2 }
}
