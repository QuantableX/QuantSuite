//! Tokens of Collection sources — in the suite's secret vault
//! (`qs_core::vault`, id `script/collection/<source id>`), so they move with
//! an export (PLAN-MIGRATION §1.1). They used to sit in the OS credential
//! store directly, which is bound to this machine; an entry still there is
//! moved into the vault the first time it is read. A token goes in through
//! `collection_token_set` and out only to GitHub: never into settings,
//! script.db, logs, events or command results — the UI gets `has_token`.

#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::Mutex;

/// Where tokens live. The app uses [`KeyringTokens`]; tests use [`MemoryTokens`].
pub trait TokenStore: Send + Sync {
    fn get(&self, source_id: &str) -> Result<Option<String>, String>;
    fn set(&self, source_id: &str, token: &str) -> Result<(), String>;
    fn clear(&self, source_id: &str) -> Result<(), String>;
}

/// The OS credential store service the tokens used before the vault.
const LEGACY_SERVICE: &str = "QuantSuite QuantScript Collection";

fn vault_id(source_id: &str) -> String {
    format!("script/collection/{source_id}")
}

/// The app's token store: the vault, with a one-time move of a token still
/// in the OS credential store.
pub struct KeyringTokens;

impl KeyringTokens {
    fn legacy(source_id: &str) -> Option<keyring::Entry> {
        keyring::Entry::new(LEGACY_SERVICE, source_id).ok()
    }

    fn legacy_get(source_id: &str) -> Option<String> {
        Self::legacy(source_id).and_then(|e| e.get_password().ok())
    }

    fn legacy_clear(source_id: &str) {
        if let Some(entry) = Self::legacy(source_id) {
            let _ = entry.delete_credential();
        }
    }
}

impl TokenStore for KeyringTokens {
    fn get(&self, source_id: &str) -> Result<Option<String>, String> {
        let id = vault_id(source_id);
        if let Some(token) = qs_core::vault::with_vault(|v| v.get_string(&id))? {
            return Ok(Some(token.to_string()));
        }
        // Still in the OS credential store: move it into the vault.
        match Self::legacy_get(source_id) {
            Some(token) => {
                qs_core::vault::with_vault(|v| v.set(&id, token.as_bytes()).map(|_| ()))?;
                Self::legacy_clear(source_id);
                Ok(Some(token))
            }
            None => Ok(None),
        }
    }

    fn set(&self, source_id: &str, token: &str) -> Result<(), String> {
        qs_core::vault::with_vault(|v| v.set(&vault_id(source_id), token.as_bytes()).map(|_| ()))
            .map_err(|e| format!("Could not store the token in the vault: {e}"))?;
        Self::legacy_clear(source_id);
        Ok(())
    }

    fn clear(&self, source_id: &str) -> Result<(), String> {
        qs_core::vault::with_vault(|v| v.delete(&vault_id(source_id)).map(|_| ()))
            .map_err(|e| format!("Could not remove the token from the vault: {e}"))?;
        Self::legacy_clear(source_id);
        Ok(())
    }
}

/// In memory, for tests.
#[cfg(test)]
#[derive(Default)]
pub struct MemoryTokens(Mutex<HashMap<String, String>>);

#[cfg(test)]
impl TokenStore for MemoryTokens {
    fn get(&self, source_id: &str) -> Result<Option<String>, String> {
        Ok(self.0.lock().map_err(|e| e.to_string())?.get(source_id).cloned())
    }

    fn set(&self, source_id: &str, token: &str) -> Result<(), String> {
        self.0.lock().map_err(|e| e.to_string())?.insert(source_id.to_string(), token.to_string());
        Ok(())
    }

    fn clear(&self, source_id: &str) -> Result<(), String> {
        self.0.lock().map_err(|e| e.to_string())?.remove(source_id);
        Ok(())
    }
}
