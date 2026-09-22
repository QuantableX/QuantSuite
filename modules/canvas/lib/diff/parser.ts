import type { FileDiff, DiffHunk } from '../../shared/types'

let hunkIdCounter = 0

export function resetHunkIdCounter() {
  hunkIdCounter = 0
}

export function parseUnifiedDiff(diffText: string): FileDiff[] {
  if (!diffText.trim()) return []

  const files: FileDiff[] = []
  const lines = diffText.split('\n')

  let currentFile: FileDiff | null = null
  let currentHunk: DiffHunk | null = null
  let oldLines: string[] = []
  let newLines: string[] = []

  function flushHunk() {
    if (currentHunk && currentFile) {
      currentHunk.oldContent = oldLines.join('\n')
      currentHunk.newContent = newLines.join('\n')
      currentFile.hunks.push(currentHunk)
    }
    currentHunk = null
    oldLines = []
    newLines = []
  }

  function flushFile() {
    flushHunk()
    if (currentFile) {
      files.push(currentFile)
    }
    currentFile = null
  }

  for (const line of lines) {
    // File header: diff --git a/path b/path
    if (line.startsWith('diff --git ')) {
      flushFile()
      const match = line.match(/^diff --git a\/(.+) b\/(.+)$/)
      currentFile = {
        filePath: match ? match[2] : 'unknown',
        hunks: [],
        isNew: false,
        isDeleted: false,
      }
      continue
    }

    if (!currentFile) continue

    // New/deleted file markers
    if (line.startsWith('new file mode')) {
      currentFile.isNew = true
      continue
    }
    if (line.startsWith('deleted file mode')) {
      currentFile.isDeleted = true
      continue
    }

    // Skip metadata lines (index, file markers, binary notice)
    if (
      line.startsWith('index ')
      || line.startsWith('--- ')
      || line.startsWith('+++ ')
      || line.startsWith('Binary ')
      || line.startsWith('similarity index')
      || line.startsWith('rename from')
      || line.startsWith('rename to')
      || line.startsWith('old mode')
      || line.startsWith('new mode')
    ) {
      continue
    }

    // Hunk header: @@ -oldStart,oldLines +newStart,newLines @@
    const hunkMatch = line.match(/^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/)
    if (hunkMatch) {
      flushHunk()
      currentHunk = {
        id: `hunk-${++hunkIdCounter}`,
        filePath: currentFile.filePath,
        oldStart: parseInt(hunkMatch[1]),
        oldLines: parseInt(hunkMatch[2] ?? '1'),
        newStart: parseInt(hunkMatch[3]),
        newLines: parseInt(hunkMatch[4] ?? '1'),
        oldContent: '',
        newContent: '',
        accepted: undefined,
      }
      continue
    }

    // Content lines
    if (currentHunk) {
      if (line.startsWith('-')) {
        oldLines.push(line.slice(1))
      } else if (line.startsWith('+')) {
        newLines.push(line.slice(1))
      } else if (line.startsWith(' ')) {
        oldLines.push(line.slice(1))
        newLines.push(line.slice(1))
      }
      // Ignore "\ No newline at end of file"
    }
  }

  flushFile()
  return files
}
