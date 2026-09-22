use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub transport: McpTransport,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub working_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    Stdio,
    #[serde(alias = "sse")]
    Http { port: u16 },
}

pub struct McpRegistry {
    entries: Vec<McpEntry>,
    path: Option<PathBuf>,
}

impl McpRegistry {
    pub fn new(path: PathBuf) -> Self {
        let entries = if path.exists() {
            let data = fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };
        Self {
            entries,
            path: Some(path),
        }
    }

    fn save(&self) {
        if let Some(ref path) = self.path {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let data = serde_json::to_string_pretty(&self.entries).unwrap_or_default();
            let _ = fs::write(path, data);
        }
    }

    pub fn list(&self) -> Vec<McpEntry> {
        self.entries.clone()
    }

    pub fn get(&self, id: &str) -> Option<&McpEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn add(&mut self, mut entry: McpEntry) -> McpEntry {
        entry.id = Uuid::new_v4().to_string();
        self.entries.push(entry.clone());
        self.save();
        entry
    }

    pub fn update(&mut self, id: &str, updated: McpEntry) -> Result<McpEntry, String> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| format!("MCP '{}' not found", id))?;
        entry.name = updated.name;
        entry.description = updated.description;
        entry.transport = updated.transport;
        entry.command = updated.command;
        entry.args = updated.args;
        entry.env = updated.env;
        entry.working_dir = updated.working_dir;
        let result = entry.clone();
        self.save();
        Ok(result)
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), String> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| format!("MCP '{}' not found", id))?;
        entry.enabled = enabled;
        self.save();
        Ok(())
    }

    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        let idx = self
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| format!("MCP '{}' not found", id))?;
        self.entries.remove(idx);
        self.save();
        Ok(())
    }
}
