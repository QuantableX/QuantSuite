//! One-time, suite-level data migrations that run before any module plugin.
//!
//! V3 rename (2026-08): module ids `view` -> `terminal` and `code` -> `canvas`.
//! Frontend routes, plugin names and CSS scopes were renamed in lockstep; this
//! moves the on-disk half — `~/.quantsuite/modules/<id>/` and every row in
//! `core.db` that carries a module id, an entity id prefix or a `/view`//`/code`
//! route. Idempotent via a marker setting; the database is backed up first.
//!
//! QuantZen split (2026-08-25): `zen` -> `notes`. QuantZen moved one level up to
//! become an *app* (the rail's top entry) and the module it used to be is
//! QuantNotes. Same shape as the V3 rename, own marker so installs that already
//! ran the first one still get this one.

use rusqlite::Connection;
use std::fs;

const MARKER_KEY: &str = "migration.rename_2026_08";

/// `(old id, new id)` — order is irrelevant, neither new id collides with an
/// existing module directory or scope.
const RENAMES: &[(&str, &str)] = &[("view", "terminal"), ("code", "canvas")];

pub fn run(conn: &Connection) -> rusqlite::Result<()> {
    if super::db::get_setting(conn, "core", MARKER_KEY)?.is_some() {
        return Ok(());
    }

    let db_path = super::paths::core_db();
    if db_path.exists() {
        let backup = super::paths::root().join("core.db.bak-rename");
        match fs::copy(&db_path, &backup) {
            Ok(_) => log::info!("rename migration: core.db backed up to {}", backup.display()),
            Err(e) => log::warn!("rename migration: core.db backup failed: {e}"),
        }
    }

    for (old, new) in RENAMES {
        let old_dir = super::paths::module_dir(old);
        let new_dir = super::paths::module_dir(new);
        if old_dir.exists() && !new_dir.exists() {
            match fs::rename(&old_dir, &new_dir) {
                Ok(()) => log::info!("rename migration: moved {} -> {}", old_dir.display(), new_dir.display()),
                Err(e) => log::error!("rename migration: could not move {}: {e}", old_dir.display()),
            }
        }
    }

    // Entity ids are `<module>:<rest>` and are referenced by `links` (FK, no ON
    // UPDATE cascade) — defer FK checks to the end of the transaction and
    // rewrite both sides together. The FTS index follows via the existing
    // AFTER UPDATE trigger on `entities`.
    conn.execute_batch(
        r#"
        BEGIN;
        PRAGMA defer_foreign_keys = ON;

        -- code -> canvas
        UPDATE links    SET src = 'canvas:' || substr(src, 6)  WHERE src LIKE 'code:%';
        UPDATE links    SET dst = 'canvas:' || substr(dst, 6)  WHERE dst LIKE 'code:%';
        UPDATE entities SET id  = 'canvas:' || substr(id, 6)   WHERE id  LIKE 'code:%';
        UPDATE entities SET module = 'canvas'                  WHERE module = 'code';
        UPDATE entities SET route = '/canvas' || substr(route, 6)
                                                               WHERE route = '/code' OR route LIKE '/code/%';
        UPDATE settings SET scope = 'canvas'                   WHERE scope = 'code';

        -- view -> terminal (page moves first: the old dashboard index became
        -- /terminal/overview, the old /view/quantterminal became the index)
        UPDATE links    SET src = 'terminal:' || substr(src, 6) WHERE src LIKE 'view:%';
        UPDATE links    SET dst = 'terminal:' || substr(dst, 6) WHERE dst LIKE 'view:%';
        UPDATE entities SET id  = 'terminal:' || substr(id, 6)  WHERE id  LIKE 'view:%';
        UPDATE entities SET module = 'terminal'                 WHERE module = 'view';
        UPDATE entities SET route = '/terminal'                 WHERE route = '/view/quantterminal' OR route = '/view/terminal';
        UPDATE entities SET route = '/terminal'                 WHERE route = '/view/algo';
        UPDATE entities SET route = '/terminal/overview'        WHERE route = '/view';
        UPDATE entities SET route = '/terminal' || substr(route, 6)
                                                                WHERE route LIKE '/view/%';
        UPDATE settings SET scope = 'terminal'                  WHERE scope = 'view';

        COMMIT;
        "#,
    )?;

    super::db::set_setting(conn, "core", MARKER_KEY, &serde_json::json!(true))?;
    log::info!("rename migration: view->terminal, code->canvas complete");
    Ok(())
}

/// `zen` -> `notes` (2026-08-25). The old id became the *app* id `quantzen`,
/// which lives only in `modules/apps.json` — apps never appear in routes, in
/// `entities.module` or in a settings scope, so nothing here can collide with
/// it.
///
/// One caveat worth naming: `settings` scope `notes` is also where the shared
/// data plane keeps `notes/sections` (the suite quick notes, PLAN-V2 E3). The
/// module has never written a row into `core.db` settings — its own state
/// lives in `~/.quantsuite/modules/notes/` — so the scope rewrite below is a
/// no-op guard, and the keys could not clash even if it were not.
pub fn run_zen_to_notes(conn: &Connection) -> rusqlite::Result<()> {
    const MARKER: &str = "migration.rename_zen_notes_2026_08";
    if super::db::get_setting(conn, "core", MARKER)?.is_some() {
        return Ok(());
    }

    let db_path = super::paths::core_db();
    if db_path.exists() {
        let backup = super::paths::root().join("core.db.bak-zen-notes");
        match fs::copy(&db_path, &backup) {
            Ok(_) => log::info!("zen->notes: core.db backed up to {}", backup.display()),
            Err(e) => log::warn!("zen->notes: core.db backup failed: {e}"),
        }
    }

    let old_dir = super::paths::module_dir("zen");
    let new_dir = super::paths::module_dir("notes");
    if old_dir.exists() && !new_dir.exists() {
        match fs::rename(&old_dir, &new_dir) {
            Ok(()) => log::info!("zen->notes: moved {} -> {}", old_dir.display(), new_dir.display()),
            Err(e) => log::error!("zen->notes: could not move {}: {e}", old_dir.display()),
        }
    }

    conn.execute_batch(
        r#"
        BEGIN;
        PRAGMA defer_foreign_keys = ON;

        UPDATE links    SET src = 'notes:' || substr(src, 5)  WHERE src LIKE 'zen:%';
        UPDATE links    SET dst = 'notes:' || substr(dst, 5)  WHERE dst LIKE 'zen:%';
        UPDATE entities SET id  = 'notes:' || substr(id, 5)   WHERE id  LIKE 'zen:%';
        UPDATE entities SET module = 'notes'                  WHERE module = 'zen';
        UPDATE entities SET route = '/notes' || substr(route, 5)
                                                              WHERE route = '/zen' OR route LIKE '/zen/%';
        UPDATE settings SET scope = 'notes'                   WHERE scope = 'zen';

        COMMIT;
        "#,
    )?;

    super::db::set_setting(conn, "core", MARKER, &serde_json::json!(true))?;
    log::info!("zen->notes rename complete");
    Ok(())
}
