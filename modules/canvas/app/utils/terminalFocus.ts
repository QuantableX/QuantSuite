/**
 * Moved to `@quantsuite/core` on 2026-08-26 — three modules host terminals now,
 * so the "is the keyboard in a shell" boundary is suite-wide. Re-exported here
 * so this module's existing imports keep working.
 */
export { terminalHasFocus } from '@quantsuite/core'
