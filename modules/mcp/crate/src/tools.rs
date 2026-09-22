use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

fn default_true() -> bool {
    true
}

/// Wall-clock leash on a scripted tool whose definition sets none: a script
/// that blocks on input or a dead socket must not pin a JSON-RPC request.
pub const DEFAULT_TIMEOUT_SECS: u64 = 120;
/// No definition may hold a request open longer than this, whatever it asks
/// for — the suite's own capability calls are leashed the same way.
pub const MAX_TIMEOUT_SECS: u64 = 600;

/// Brings a timeout into `1..=MAX_TIMEOUT_SECS`. `None` stays `None` — it
/// means "the default", and storing the default would pin today's value.
pub fn clamp_timeout(secs: Option<u64>) -> Option<u64> {
    secs.map(|s| s.clamp(1, MAX_TIMEOUT_SECS))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub script_path: String,
    #[serde(default)]
    pub interpreter: Option<String>,
    /// Seconds a run may take before the script is killed; unset means
    /// `DEFAULT_TIMEOUT_SECS`, `MAX_TIMEOUT_SECS` is the hard cap.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub native: bool,
}

impl ToolDef {
    /// The leash `execute_tool` puts on this script. Clamped here as well as
    /// on save, so a hand-edited tools.json cannot exceed the cap either.
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(clamp_timeout(self.timeout_secs).unwrap_or(DEFAULT_TIMEOUT_SECS))
    }
}

pub struct ToolRegistry {
    tools: Vec<ToolDef>,
    path: Option<PathBuf>,
}

impl ToolRegistry {
    pub fn new(path: PathBuf) -> Self {
        let tools = if path.exists() {
            let data = fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };
        Self {
            tools,
            path: Some(path),
        }
    }

    fn save(&self) {
        if let Some(ref path) = self.path {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let data = serde_json::to_string_pretty(&self.tools).unwrap_or_default();
            let _ = fs::write(path, data);
        }
    }

    pub fn list(&self) -> Vec<ToolDef> {
        self.tools.clone()
    }

    pub fn list_enabled(&self) -> Vec<ToolDef> {
        self.tools.iter().filter(|t| t.enabled).cloned().collect()
    }

    pub fn get(&self, id: &str) -> Option<&ToolDef> {
        self.tools.iter().find(|t| t.id == id)
    }

    pub fn get_by_name(&self, name: &str) -> Option<&ToolDef> {
        self.tools.iter().find(|t| t.name == name && t.enabled)
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String> {
        let tool = self
            .tools
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("Tool '{}' not found", id))?;
        let previous = tool.enabled;
        tool.enabled = enabled;
        let saved = (|| -> Result<(), String> {
            if let Some(path) = &self.path {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                let data = serde_json::to_string_pretty(&self.tools).map_err(|e| e.to_string())?;
                fs::write(path, data).map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        if saved.is_err() {
            if let Some(tool) = self.tools.iter_mut().find(|tool| tool.id == id) {
                tool.enabled = previous;
            }
        }
        saved
    }

    pub fn add(&mut self, mut tool: ToolDef) -> ToolDef {
        tool.id = Uuid::new_v4().to_string();
        tool.enabled = true;
        tool.native = false;
        tool.timeout_secs = clamp_timeout(tool.timeout_secs);
        self.tools.push(tool.clone());
        self.save();
        tool
    }

    pub fn update(&mut self, id: &str, updated: ToolDef) -> Result<ToolDef, String> {
        let tool = self
            .tools
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("Tool '{}' not found", id))?;
        tool.name = updated.name;
        tool.description = updated.description;
        tool.input_schema = updated.input_schema;
        tool.script_path = updated.script_path;
        tool.interpreter = updated.interpreter;
        tool.timeout_secs = clamp_timeout(updated.timeout_secs);
        // preserve enabled state
        let result = tool.clone();
        self.save();
        Ok(result)
    }

    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        let idx = self
            .tools
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| format!("Tool '{}' not found", id))?;
        self.tools.remove(idx);
        self.save();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(timeout_secs: Option<u64>) -> ToolDef {
        ToolDef {
            id: String::new(),
            name: "t".into(),
            description: String::new(),
            input_schema: serde_json::json!({}),
            script_path: "t.py".into(),
            interpreter: None,
            timeout_secs,
            enabled: true,
            native: false,
        }
    }

    #[test]
    fn toggles_persist_and_failed_saves_keep_the_previous_choice() {
        let dir = std::env::temp_dir().join(format!("qs-tool-preferences-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tools.json");
        let mut registry = ToolRegistry::new(path.clone());
        let tool = registry.add(def(None));
        registry.set_enabled(&tool.id, false).unwrap();
        assert!(!ToolRegistry::new(path.clone()).get(&tool.id).unwrap().enabled);
        // A path below a file cannot be created; the in-memory choice must stay off.
        registry.path = Some(path.join("invalid.json"));
        assert!(registry.set_enabled(&tool.id, true).is_err());
        assert!(!registry.get(&tool.id).unwrap().enabled);
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }

    /// Unset falls back to the default; anything set is clamped into the cap.
    #[test]
    fn timeout_defaults_and_clamps() {
        assert_eq!(def(None).timeout(), Duration::from_secs(DEFAULT_TIMEOUT_SECS));
        assert_eq!(def(Some(30)).timeout(), Duration::from_secs(30));
        assert_eq!(def(Some(0)).timeout(), Duration::from_secs(1));
        assert_eq!(def(Some(9_999)).timeout(), Duration::from_secs(MAX_TIMEOUT_SECS));
    }

    /// A tools.json written before the field existed still loads.
    #[test]
    fn timeout_secs_is_optional_in_stored_json() {
        let t: ToolDef = serde_json::from_str(
            r#"{"id":"a","name":"n","description":"","input_schema":{},"script_path":"x.py"}"#,
        )
        .expect("parse");
        assert_eq!(t.timeout_secs, None);
    }
}
