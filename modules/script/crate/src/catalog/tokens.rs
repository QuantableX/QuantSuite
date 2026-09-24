//! Tokens of Collection sources — in the OS credential store (Windows Credential
//! Manager, macOS Keychain, Secret Service), one entry per source id. A
//! token goes in through `collection_token_set` and out only to GitHub: never
//! into settings, script.db, logs, events or command results — the UI gets
//! `has_token`.

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

const SERVICE: &str = "QuantSuite QuantScript Collection";

pub struct KeyringTokens;

impl KeyringTokens {
    fn entry(source_id: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, source_id).map_err(|e| format!("The credential store is not available: {e}"))
    }
}

impl TokenStore for KeyringTokens {
    fn get(&self, source_id: &str) -> Result<Option<String>, String> {
        match Self::entry(source_id)?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("Could not read the token from the credential store: {e}")),
        }
    }

    fn set(&self, source_id: &str, token: &str) -> Result<(), String> {
        Self::entry(source_id)?
            .set_password(token)
            .map_err(|e| format!("Could not store the token in the credential store: {e}"))
    }

    fn clear(&self, source_id: &str) -> Result<(), String> {
        match Self::entry(source_id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("Could not remove the token from the credential store: {e}")),
        }
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
