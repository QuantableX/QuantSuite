// File-type icon definitions with per-extension colors
// Inspired by collab-public's rich icon system

export interface FileIconDef {
  svg: string
  color: string
}

// SVG path templates (14x14 viewBox assumed in component)
const FILE_CODE = '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="10" y1="14" x2="8" y2="16"/><line x1="10" y1="14" x2="8" y2="12"/><line x1="14" y1="14" x2="16" y2="16"/><line x1="14" y1="14" x2="16" y2="12"/>'
const FILE_TEXT = '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/>'
const FILE_PLAIN = '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/>'
const FILE_IMAGE = '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><circle cx="10" cy="14" r="2"/><path d="M20 17l-3-3-7 7"/>'
const FILE_CONFIG = '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="8" y1="13" x2="16" y2="13"/><line x1="8" y1="17" x2="12" y2="17"/>'

const EXT_MAP: Record<string, FileIconDef> = {
  // TypeScript
  '.ts': { svg: FILE_CODE, color: '#5c9bcf' },
  '.tsx': { svg: FILE_CODE, color: '#5c9bcf' },
  '.mts': { svg: FILE_CODE, color: '#5c9bcf' },
  '.cts': { svg: FILE_CODE, color: '#5c9bcf' },
  '.d.ts': { svg: FILE_CODE, color: '#5c9bcf' },

  // JavaScript
  '.js': { svg: FILE_CODE, color: '#c8a35a' },
  '.jsx': { svg: FILE_CODE, color: '#c8a35a' },
  '.mjs': { svg: FILE_CODE, color: '#c8a35a' },
  '.cjs': { svg: FILE_CODE, color: '#c8a35a' },

  // Python
  '.py': { svg: FILE_CODE, color: '#7aab6e' },

  // Rust
  '.rs': { svg: FILE_CODE, color: '#c07a53' },

  // C family
  '.c': { svg: FILE_CODE, color: '#7a8aab' },
  '.h': { svg: FILE_CODE, color: '#7a8aab' },
  '.cpp': { svg: FILE_CODE, color: '#7a8aab' },
  '.hpp': { svg: FILE_CODE, color: '#7a8aab' },
  '.cc': { svg: FILE_CODE, color: '#7a8aab' },
  '.cs': { svg: FILE_CODE, color: '#8a7aab' },

  // Web
  '.html': { svg: FILE_CODE, color: '#c07a6e' },
  '.htm': { svg: FILE_CODE, color: '#c07a6e' },
  '.css': { svg: FILE_CODE, color: '#8a7aab' },
  '.scss': { svg: FILE_CODE, color: '#9a6e8a' },
  '.less': { svg: FILE_CODE, color: '#9a6e8a' },
  '.vue': { svg: FILE_CODE, color: '#7aab7a' },
  '.svelte': { svg: FILE_CODE, color: '#c07a53' },
  '.svg': { svg: FILE_IMAGE, color: '#c8a35a' },

  // Data / config
  '.json': { svg: FILE_CONFIG, color: '#8a8a7a' },
  '.yaml': { svg: FILE_CONFIG, color: '#8a8a7a' },
  '.yml': { svg: FILE_CONFIG, color: '#8a8a7a' },
  '.toml': { svg: FILE_CONFIG, color: '#8a8a7a' },
  '.ini': { svg: FILE_CONFIG, color: '#8a8a7a' },
  '.env': { svg: FILE_CONFIG, color: '#8a8a7a' },
  '.sql': { svg: FILE_CODE, color: '#7a8aab' },
  '.csv': { svg: FILE_TEXT, color: '#7aab6e' },
  '.xml': { svg: FILE_CODE, color: '#c07a6e' },

  // Images
  '.png': { svg: FILE_IMAGE, color: '#8a7aab' },
  '.jpg': { svg: FILE_IMAGE, color: '#8a7aab' },
  '.jpeg': { svg: FILE_IMAGE, color: '#8a7aab' },
  '.gif': { svg: FILE_IMAGE, color: '#8a7aab' },
  '.webp': { svg: FILE_IMAGE, color: '#8a7aab' },
  '.ico': { svg: FILE_IMAGE, color: '#8a7aab' },
  '.bmp': { svg: FILE_IMAGE, color: '#8a7aab' },
  '.avif': { svg: FILE_IMAGE, color: '#8a7aab' },

  // Documents
  '.pdf': { svg: FILE_TEXT, color: '#c07a6e' },
  '.doc': { svg: FILE_TEXT, color: '#5c9bcf' },
  '.docx': { svg: FILE_TEXT, color: '#5c9bcf' },
  '.xls': { svg: FILE_TEXT, color: '#7aab6e' },
  '.xlsx': { svg: FILE_TEXT, color: '#7aab6e' },

  // Archives
  '.zip': { svg: FILE_PLAIN, color: '#8a8a7a' },
  '.tar': { svg: FILE_PLAIN, color: '#8a8a7a' },
  '.gz': { svg: FILE_PLAIN, color: '#8a8a7a' },
  '.7z': { svg: FILE_PLAIN, color: '#8a8a7a' },

  // Markdown
  '.md': { svg: FILE_TEXT, color: '#6a9fcf' },
  '.mdx': { svg: FILE_TEXT, color: '#5c9bcf' },

  // Shell
  '.sh': { svg: FILE_CODE, color: '#7aab6e' },
  '.bash': { svg: FILE_CODE, color: '#7aab6e' },
  '.zsh': { svg: FILE_CODE, color: '#7aab6e' },
  '.fish': { svg: FILE_CODE, color: '#7aab6e' },
  '.ps1': { svg: FILE_CODE, color: '#5c9bcf' },
  '.bat': { svg: FILE_CODE, color: '#8a8a7a' },
  '.cmd': { svg: FILE_CODE, color: '#8a8a7a' },

  // Go
  '.go': { svg: FILE_CODE, color: '#5c9bcf' },

  // Java / Kotlin
  '.java': { svg: FILE_CODE, color: '#c07a53' },
  '.kt': { svg: FILE_CODE, color: '#8a7aab' },

  // Ruby
  '.rb': { svg: FILE_CODE, color: '#c07a6e' },

  // Swift
  '.swift': { svg: FILE_CODE, color: '#c07a53' },

  // Lock files
  '.lock': { svg: FILE_CONFIG, color: '#6a6a6a' },
}

const FILENAME_MAP: Record<string, FileIconDef> = {
  'Dockerfile': { svg: FILE_CODE, color: '#5c9bcf' },
  'Makefile': { svg: FILE_CODE, color: '#8a8a7a' },
  'LICENSE': { svg: FILE_TEXT, color: '#8a8a7a' },
  '.gitignore': { svg: FILE_CONFIG, color: '#6a6a6a' },
  '.eslintrc': { svg: FILE_CONFIG, color: '#8a7aab' },
  '.prettierrc': { svg: FILE_CONFIG, color: '#8a7aab' },
}

const DEFAULT_ICON: FileIconDef = {
  svg: FILE_PLAIN,
  color: '#5a5a65',
}

export function getFileIcon(filename: string): FileIconDef {
  const match = FILENAME_MAP[filename]
  if (match) return match

  const dotIdx = filename.lastIndexOf('.')
  if (dotIdx >= 0) {
    const ext = filename.slice(dotIdx).toLowerCase()
    const extMatch = EXT_MAP[ext]
    if (extMatch) return extMatch
  }

  return DEFAULT_ICON
}

export function getFileStemAndExt(filename: string): { stem: string; ext: string } {
  const dotIdx = filename.lastIndexOf('.')
  if (dotIdx <= 0) return { stem: filename, ext: '' }
  return {
    stem: filename.slice(0, dotIdx),
    ext: filename.slice(dotIdx),
  }
}
