import { invoke } from '@tauri-apps/api/core'
import type { SpecFile, SpecFrontmatter, SpecStatus } from '../../shared/types'

const FRONTMATTER_DELIMITER = '---'

export function parseSpecFile(filePath: string, rawContent: string): SpecFile {
  const frontmatter = parseFrontmatter(rawContent)
  const content = extractMarkdownBody(rawContent)

  return {
    path: filePath,
    frontmatter,
    content,
    rawContent,
  }
}

function normalizeSpecStatus(value: unknown): SpecStatus {
  switch (value) {
    case 'done':
      return 'done'
    case 'verify':
      return 'verify'
    case 'open':
    case 'planned':
    case 'in-progress':
    case 'blocked':
    default:
      return 'open'
  }
}

function parseFrontmatter(raw: string): SpecFrontmatter {
  const lines = raw.split('\n')
  let inFrontmatter = false
  const frontmatterLines: string[] = []

  for (const line of lines) {
    const trimmed = line.trim()
    if (trimmed === FRONTMATTER_DELIMITER) {
      if (!inFrontmatter) {
        inFrontmatter = true
        continue
      } else {
        break
      }
    }
    if (inFrontmatter) {
      frontmatterLines.push(line)
    }
  }

  const parsed = parseYamlSimple(frontmatterLines.join('\n'))

  return {
    title: (parsed.title as string) || 'Untitled Spec',
    status: normalizeSpecStatus(parsed.status),
    priority: parsed.priority as SpecFrontmatter['priority'],
    assignedTo: parsed.assignedTo as string | undefined,
    verifyMode: parsed.verifyMode === true || parsed.verifyMode === 'true',
    linkedFiles: parseStringArray(parsed.linkedFiles),
    tags: parseStringArray(parsed.tags),
    createdAt: (parsed.createdAt as string) || new Date().toISOString(),
    updatedAt: (parsed.updatedAt as string) || new Date().toISOString(),
  }
}

function extractMarkdownBody(raw: string): string {
  const lines = raw.split('\n')
  let delimiterCount = 0
  let bodyStartIndex = 0

  for (let i = 0; i < lines.length; i++) {
    if (lines[i].trim() === FRONTMATTER_DELIMITER) {
      delimiterCount++
      if (delimiterCount === 2) {
        bodyStartIndex = i + 1
        break
      }
    }
  }

  if (delimiterCount < 2) {
    return raw
  }

  return lines.slice(bodyStartIndex).join('\n').trim()
}

function parseYamlSimple(yaml: string): Record<string, unknown> {
  const result: Record<string, unknown> = {}
  const lines = yaml.split('\n')
  let currentKey: string | null = null
  let arrayValues: string[] = []
  let collectingArray = false

  for (const line of lines) {
    // Skip empty lines and comments
    if (!line.trim() || line.trim().startsWith('#')) continue

    // Check for array item (starts with "  - ")
    const arrayMatch = line.match(/^\s+-\s+(.+)/)
    if (arrayMatch && currentKey) {
      collectingArray = true
      arrayValues.push(arrayMatch[1].trim().replace(/^['"]|['"]$/g, ''))
      continue
    }

    // If we were collecting array values, flush them
    if (collectingArray && currentKey) {
      result[currentKey] = arrayValues
      arrayValues = []
      collectingArray = false
    }

    // Check for key: value pair
    const kvMatch = line.match(/^(\w[\w\s]*?):\s*(.*)/)
    if (kvMatch) {
      currentKey = kvMatch[1].trim()
      const value = kvMatch[2].trim()

      if (!value) {
        // Value is on subsequent lines (array or block)
        continue
      }

      // Handle inline arrays: [val1, val2]
      if (value.startsWith('[') && value.endsWith(']')) {
        const inner = value.slice(1, -1)
        result[currentKey] = inner
          .split(',')
          .map(s => s.trim().replace(/^['"]|['"]$/g, ''))
          .filter(Boolean)
        continue
      }

      // Handle quoted strings
      if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) {
        result[currentKey] = value.slice(1, -1)
        continue
      }

      // Handle booleans
      if (value === 'true') { result[currentKey] = true; continue }
      if (value === 'false') { result[currentKey] = false; continue }

      // Handle numbers
      const num = Number(value)
      if (!isNaN(num) && value !== '') {
        result[currentKey] = num
        continue
      }

      result[currentKey] = value
    }
  }

  // Flush any remaining array
  if (collectingArray && currentKey) {
    result[currentKey] = arrayValues
  }

  return result
}

function parseStringArray(value: unknown): string[] | undefined {
  if (Array.isArray(value)) {
    return value.map(String)
  }
  if (typeof value === 'string') {
    return [value]
  }
  return undefined
}

export function serializeSpec(spec: SpecFile): string {
  const fm = spec.frontmatter
  const lines: string[] = [FRONTMATTER_DELIMITER]

  lines.push(`title: ${fm.title}`)
  lines.push(`status: ${fm.status}`)

  if (fm.priority) {
    lines.push(`priority: ${fm.priority}`)
  }
  if (fm.assignedTo) {
    lines.push(`assignedTo: ${fm.assignedTo}`)
  }
  if (fm.verifyMode) {
    lines.push(`verifyMode: true`)
  }
  lines.push('linkedFiles:')
  if (fm.linkedFiles && fm.linkedFiles.length > 0) {
    for (const file of fm.linkedFiles) {
      lines.push(`  - ${file}`)
    }
  }

  lines.push(`createdAt: ${fm.createdAt}`)
  lines.push(`updatedAt: ${fm.updatedAt}`)
  lines.push(FRONTMATTER_DELIMITER)
  lines.push('')

  if (spec.content) {
    lines.push(spec.content)
  }

  return lines.join('\n') + '\n'
}

export function updateSpecField(
  rawContent: string,
  field: string,
  value: string,
): string {
  const lines = rawContent.split('\n')
  let inFrontmatter = false
  let found = false

  for (let i = 0; i < lines.length; i++) {
    const trimmed = lines[i].trim()
    if (trimmed === FRONTMATTER_DELIMITER) {
      if (!inFrontmatter) {
        inFrontmatter = true
        continue
      } else {
        // End of frontmatter - if field not found, insert it before closing delimiter
        if (!found) {
          lines.splice(i, 0, `${field}: ${value}`)
        }
        break
      }
    }
    if (inFrontmatter) {
      const kvMatch = lines[i].match(/^(\w[\w\s]*?):\s*(.*)/)
      if (kvMatch && kvMatch[1].trim() === field) {
        lines[i] = `${field}: ${value}`
        found = true
      }
    }
  }

  return lines.join('\n')
}

interface FileEntry {
  name: string
  path: string
  isDirectory: boolean
  children?: FileEntry[]
}

function collectSpecFiles(entries: FileEntry[]): FileEntry[] {
  const result: FileEntry[] = []
  for (const entry of entries) {
    if (!entry.isDirectory && entry.name.endsWith('.spec.md')) {
      result.push(entry)
    }
    if (entry.children) {
      result.push(...collectSpecFiles(entry.children))
    }
  }
  return result
}

/** Private storage is resolved by Rust so all consumers use the same migration. */
export async function workspaceStorageDir(workspacePath: string): Promise<string> {
  return invoke<string>('plugin:canvas|workspace_storage_dir', { folderPath: workspacePath })
}

export async function findSpecFiles(workspacePath: string): Promise<SpecFile[]> {
  const storage = await workspaceStorageDir(workspacePath)
  const specDirPath = `${storage}/specs`
  let entries: FileEntry[]
  try {
    entries = await invoke<FileEntry[]>('plugin:canvas|read_dir_tree', {
      path: specDirPath,
      gitignore: false,
    })
  } catch (error) {
    if (String(error).startsWith('Path does not exist:')) return []
    throw error
  }
  return Promise.all(collectSpecFiles(entries).map(async (entry) => {
    const rawContent = await invoke<string>('plugin:canvas|read_file', { path: entry.path })
    // Absolute paths also work when a spec is opened in the normal editor.
    return parseSpecFile(entry.path, rawContent)
  }))
}
