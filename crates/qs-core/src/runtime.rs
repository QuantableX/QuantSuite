//! Erased entry points into the running app for sibling crates (E4).
//!
//! A module crate sometimes needs to write to the shared data plane from
//! plain Rust code that has no `AppHandle` — QuantMCP's kanban store mirrors
//! its cards onto the global board this way. These hooks capture the handle
//! once at setup and expose exactly two operations, both of which behave
//! like their command counterparts: they write core.db **and** emit the bus
//! event, so every open surface updates live. Nothing else is exposed —
//! this is a mirror-writer's door, not a general backdoor into the app.

use crate::db::Entity;
use std::sync::OnceLock;

type UpsertFn = Box<dyn Fn(Entity) -> Result<(), String> + Send + Sync>;
type DeleteFn = Box<dyn Fn(String) -> Result<(), String> + Send + Sync>;

struct Hooks {
    upsert: UpsertFn,
    delete: DeleteFn,
}

static HOOKS: OnceLock<Hooks> = OnceLock::new();

pub fn install<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    use tauri::Manager;

    let upsert_app = app.clone();
    let delete_app = app;

    let _ = HOOKS.set(Hooks {
        upsert: Box::new(move |entity| {
            let db = upsert_app
                .try_state::<crate::db::Db>()
                .ok_or("core.db not ready")?;
            {
                let conn = db.0.lock().map_err(|e| e.to_string())?;
                crate::db::upsert_entity(&conn, &entity).map_err(|e| e.to_string())?;
            }
            let _ = crate::bus::emit(
                &upsert_app,
                "core.entity.upserted",
                serde_json::json!({ "id": entity.id, "module": entity.module, "kind": entity.kind }),
            );
            Ok(())
        }),
        delete: Box::new(move |id| {
            let db = delete_app
                .try_state::<crate::db::Db>()
                .ok_or("core.db not ready")?;
            {
                let conn = db.0.lock().map_err(|e| e.to_string())?;
                crate::db::delete_entity(&conn, &id).map_err(|e| e.to_string())?;
            }
            let _ = crate::bus::emit(
                &delete_app,
                "core.entity.deleted",
                serde_json::json!({ "id": id }),
            );
            Ok(())
        }),
    });
}

/// Upsert an entity exactly as the `upsert_entity` command would.
/// Errs if the app is not up yet (or in unit tests) — callers mirror
/// best-effort and may ignore it.
pub fn upsert_entity(entity: Entity) -> Result<(), String> {
    (HOOKS.get().ok_or("runtime hooks not installed")?.upsert)(entity)
}

/// Delete an entity exactly as the `delete_entity` command would.
pub fn delete_entity(id: &str) -> Result<(), String> {
    (HOOKS.get().ok_or("runtime hooks not installed")?.delete)(id.to_string())
}
