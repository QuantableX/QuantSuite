export interface RemoteManifest {
  commands: Record<string, string>
  plugins?: Record<string, string>
  data?: { modules: string[]; scopes: string[] }
}

export function route(manifest: RemoteManifest, cmd: string, args: Record<string, unknown> | undefined): 'server' | 'local' | 'deny' | 'both'

export function mergeSearch(manifest: RemoteManifest, local: unknown, remote: unknown, limit?: number): unknown[]
