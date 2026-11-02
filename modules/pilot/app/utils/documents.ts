/**
 * Document attachments (parity 16), shared by the composer and question
 * answers (parity 17): which files are read into text, reading one, and how
 * its state reads on the shelf. Keep DOCUMENT in step with KINDS in
 * documents.rs.
 */
import { pilot } from '#pilot/utils/invoke'
import type { DocInfo } from '#pilot/types'

export const DOCUMENT = /\.(pdf|docx|pptx|odt|doc|xlsx|xls|ppt|odp|ods|epub|pages|key|numbers)$/i

/** A saved attachment's text (cached by content); never throws. */
export async function readDocument(path: string, name: string): Promise<DocInfo> {
  try {
    return (await pilot<DocInfo>('plugin:pilot|pilot_document_extract', { path, name }))!
  } catch (e) {
    return { status: 'failed', kind: '', format: 'Document', title: null, pages: null, pagesRead: 0, unit: '', chars: 0, textPath: null, detail: String(e), emptyPages: [] }
  }
}

/** Stops reading a document that left the shelf. */
export function cancelDocument(path: string) {
  void pilot('plugin:pilot|pilot_document_cancel', { path }).catch(() => undefined)
}

export function docLabel(d: DocInfo): string {
  const n = d.pages ?? 0
  const units = d.unit && d.pages != null ? `${n.toLocaleString('en-US')} ${d.unit}${n === 1 ? '' : 's'}` : ''
  switch (d.status) {
    case 'ok':
      return units ? `Text · ${units}` : 'Text read'
    case 'truncated':
      return units ? `Partly read · ${d.pagesRead.toLocaleString('en-US')} of ${units}` : 'Partly read'
    case 'needs_ocr':
      return 'Scanned · needs OCR'
    case 'encrypted':
      return 'Password-protected'
    case 'unsupported':
      return 'Text not extracted'
    case 'cancelled':
      return 'Not read'
    default:
      return 'Could not read'
  }
}
