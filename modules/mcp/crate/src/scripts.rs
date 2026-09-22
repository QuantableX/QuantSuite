use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScriptLanguage {
    Python,
    Javascript,
    Bash,
    Powershell,
}

impl ScriptLanguage {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Python => "py",
            Self::Javascript => "js",
            Self::Bash => "sh",
            Self::Powershell => "ps1",
        }
    }

    pub fn interpreter(&self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::Javascript => "node",
            Self::Bash => "bash",
            Self::Powershell => "powershell",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptEntry {
    pub id: String,
    pub name: String,
    pub language: ScriptLanguage,
    pub content: String,
    pub created_at: u64,
}

pub struct ScriptRegistry {
    scripts: Vec<ScriptEntry>,
    meta_path: PathBuf,
    scripts_dir: PathBuf,
}

impl ScriptRegistry {
    pub fn new(meta_path: PathBuf, scripts_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&scripts_dir);
        let scripts = if meta_path.exists() {
            let data = fs::read_to_string(&meta_path).unwrap_or_default();
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };
        Self {
            scripts,
            meta_path,
            scripts_dir,
        }
    }

    fn save_meta(&self) {
        if let Some(parent) = self.meta_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let data = serde_json::to_string_pretty(&self.scripts).unwrap_or_default();
        let _ = fs::write(&self.meta_path, data);
    }

    fn write_script_file(&self, entry: &ScriptEntry) {
        let file_path = self
            .scripts_dir
            .join(format!("{}.{}", entry.id, entry.language.extension()));
        let _ = fs::write(file_path, &entry.content);
    }

    fn delete_script_file(&self, entry: &ScriptEntry) {
        let file_path = self
            .scripts_dir
            .join(format!("{}.{}", entry.id, entry.language.extension()));
        let _ = fs::remove_file(file_path);
    }

    pub fn get_script_path(&self, id: &str) -> Option<String> {
        self.scripts.iter().find(|s| s.id == id).map(|s| {
            self.scripts_dir
                .join(format!("{}.{}", s.id, s.language.extension()))
                .to_string_lossy()
                .to_string()
        })
    }

    pub fn list(&self) -> Vec<ScriptEntry> {
        self.scripts.clone()
    }

    pub fn add(
        &mut self,
        name: String,
        language: ScriptLanguage,
        content: String,
    ) -> ScriptEntry {
        let entry = ScriptEntry {
            id: Uuid::new_v4().to_string(),
            name,
            language,
            content,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        self.write_script_file(&entry);
        self.scripts.push(entry.clone());
        self.save_meta();
        entry
    }

    pub fn update(
        &mut self,
        id: &str,
        name: String,
        language: ScriptLanguage,
        content: String,
    ) -> Result<ScriptEntry, String> {
        let script = self
            .scripts
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| format!("Script '{}' not found", id))?;
        // Remove old file (extension may change if language changed)
        let old_path = self
            .scripts_dir
            .join(format!("{}.{}", script.id, script.language.extension()));
        let _ = fs::remove_file(&old_path);
        script.name = name;
        script.language = language;
        script.content = content;
        let result = script.clone();
        self.write_script_file(&result);
        self.save_meta();
        Ok(result)
    }

    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        let idx = self
            .scripts
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| format!("Script '{}' not found", id))?;
        let entry = self.scripts.remove(idx);
        self.delete_script_file(&entry);
        self.save_meta();
        Ok(())
    }
}
