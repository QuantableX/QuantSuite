/**
 * Monaco's web workers.
 *
 * Monaco runs its language services — TypeScript, JSON, CSS, HTML — in workers
 * and asks `MonacoEnvironment.getWorker` for them. Without this, it warns that
 * it could not create a worker, falls back to loading the worker code on the
 * main thread, and the TypeScript fallback then throws on `require.toUrl`
 * (an AMD-loader API the ESM build does not have). The editor renders NOTHING
 * after that — a correctly sized, completely empty box.
 *
 * That failure only appears once models carry `file:` URIs, because that is
 * what makes Monaco recognise a `.ts` file and start the service at all. It is
 * worth having: it is where the problems panel's diagnostics come from.
 *
 * Vite's `?worker` suffix compiles each one into its own chunk, fetched the
 * first time a matching file is opened — not at page load.
 *
 * Client-only: this module touches `self` at import time, so it is imported
 * dynamically from the editor's `init()`, never at the top level.
 */
import EditorWorker from 'monaco-editor/esm/vs/editor/editor.worker?worker'
import JsonWorker from 'monaco-editor/esm/vs/language/json/json.worker?worker'
import CssWorker from 'monaco-editor/esm/vs/language/css/css.worker?worker'
import HtmlWorker from 'monaco-editor/esm/vs/language/html/html.worker?worker'
import TsWorker from 'monaco-editor/esm/vs/language/typescript/ts.worker?worker'

let installed = false

/** Register the worker factory once per page. Safe to call repeatedly. */
export function installMonacoEnvironment(): void {
  if (installed || typeof self === 'undefined') return
  installed = true

  ;(self as unknown as { MonacoEnvironment: unknown }).MonacoEnvironment = {
    getWorker(_workerId: string, label: string) {
      switch (label) {
        case 'json':
          return new JsonWorker()
        case 'css':
        case 'scss':
        case 'less':
          return new CssWorker()
        case 'html':
        case 'handlebars':
        case 'razor':
          return new HtmlWorker()
        case 'typescript':
        case 'javascript':
          return new TsWorker()
        default:
          return new EditorWorker()
      }
    },
  }
}
