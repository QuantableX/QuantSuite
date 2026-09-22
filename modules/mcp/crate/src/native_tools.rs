use crate::tools::ToolDef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NativeToolManifestEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub script_path: String,
    #[serde(default)]
    pub interpreter: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_switches_survive_restart_and_report_failed_saves() {
        let dir = std::env::temp_dir().join(format!("qs-native-toggle-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let manifest = dir.join("native_tools.json");
        let state = dir.join("state.json");
        fs::write(&manifest, r#"[{"id":"native","name":"native_tool","description":"test","input_schema":{},"script_path":"test.py"}]"#).unwrap();
        let mut registry = NativeToolRegistry::new(dir.clone(), state.clone());
        registry.set_enabled("native", false).unwrap();
        assert!(!NativeToolRegistry::new(dir.clone(), state.clone()).get("native").unwrap().enabled);
        registry.state_path = manifest.join("invalid.json");
        assert!(registry.set_enabled("native", true).is_err());
        assert!(!registry.get("native").unwrap().enabled);
        assert!(!registry.state.enabled["native"]);
        fs::remove_file(manifest).unwrap();
        fs::remove_file(state).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct NativeToolsState {
    enabled: HashMap<String, bool>,
}

pub struct NativeToolRegistry {
    tools: Vec<ToolDef>,
    state_path: PathBuf,
    state: NativeToolsState,
}

impl NativeToolRegistry {
    pub fn new(resource_dir: PathBuf, state_path: PathBuf) -> Self {
        let manifest_path = resource_dir.join("native_tools.json");
        let entries: Vec<NativeToolManifestEntry> = if manifest_path.exists() {
            let data = fs::read_to_string(&manifest_path).unwrap_or_default();
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };

        let state: NativeToolsState = if state_path.exists() {
            let data = fs::read_to_string(&state_path).unwrap_or_default();
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            NativeToolsState::default()
        };

        let tools = entries
            .into_iter()
            .map(|e| {
                let absolute_script_path = resource_dir
                    .join(&e.script_path)
                    .to_string_lossy()
                    .to_string();
                let enabled = state.enabled.get(&e.id).copied().unwrap_or(true);
                ToolDef {
                    id: e.id,
                    name: e.name,
                    description: e.description,
                    input_schema: e.input_schema,
                    script_path: absolute_script_path,
                    interpreter: e.interpreter,
                    timeout_secs: None,
                    enabled,
                    native: true,
                }
            })
            .collect();

        Self {
            tools,
            state_path,
            state,
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

    pub fn has_name(&self, name: &str) -> bool {
        self.tools.iter().any(|t| t.name == name)
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String> {
        let tool = self
            .tools
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("Native tool '{}' not found", id))?;
        let mut next = self.state.clone();
        next.enabled.insert(id.to_string(), enabled);
        if let Some(parent) = self.state_path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let data = serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?;
        fs::write(&self.state_path, data).map_err(|e| e.to_string())?;
        tool.enabled = enabled;
        self.state = next;
        Ok(())
    }
}
