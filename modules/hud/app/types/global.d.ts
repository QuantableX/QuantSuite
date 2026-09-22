declare global {
  interface Window {
    __TAURI__?: {
      [key: string]: unknown
    }
  }
}

export {}

