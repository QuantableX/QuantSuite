use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase", tag = "status")]
pub enum McpProcessStatus {
    Running,
    Stopped,
    Error { message: String },
}

struct ProcessEntry {
    child: Option<Child>,
    last_status: McpProcessStatus,
}

pub struct ProcessManager {
    processes: Arc<Mutex<HashMap<String, ProcessEntry>>>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start(
        &self,
        id: &str,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
        working_dir: Option<&str>,
    ) -> Result<(), String> {
        let mut processes = self.processes.lock().await;
        if let Some(entry) = processes.get_mut(id) {
            // Reap first — a process that died since the last status poll still
            // carries its Child handle and would look like it is running.
            if Self::check_status(entry) == McpProcessStatus::Running {
                return Err("Process already running".into());
            }
        }

        let mut cmd = Command::new(command);
        cmd.args(args);
        cmd.envs(env);
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }
        cmd.kill_on_drop(true);
        cmd.stdout(std::process::Stdio::null());
        cmd.stderr(std::process::Stdio::null());
        #[cfg(windows)]
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW

        let child = cmd
            .spawn()
            .map_err(|e| format!("Failed to start: {}", e))?;
        processes.insert(
            id.to_string(),
            ProcessEntry {
                child: Some(child),
                last_status: McpProcessStatus::Running,
            },
        );
        Ok(())
    }

    pub async fn stop(&self, id: &str) -> Result<(), String> {
        let mut processes = self.processes.lock().await;
        if let Some(entry) = processes.get_mut(id) {
            if let Some(mut child) = entry.child.take() {
                child
                    .kill()
                    .await
                    .map_err(|e| format!("Failed to kill: {}", e))?;
                entry.last_status = McpProcessStatus::Stopped;
                Ok(())
            } else {
                Err("Process not running".into())
            }
        } else {
            Err("Process not running".into())
        }
    }

    fn check_status(entry: &mut ProcessEntry) -> McpProcessStatus {
        if let Some(ref mut child) = entry.child {
            match child.try_wait() {
                Ok(Some(exit)) => {
                    entry.child = None;
                    let status = if exit.success() {
                        McpProcessStatus::Stopped
                    } else {
                        McpProcessStatus::Error {
                            message: format!("Exited with code: {:?}", exit.code()),
                        }
                    };
                    entry.last_status = status.clone();
                    status
                }
                Ok(None) => {
                    entry.last_status = McpProcessStatus::Running;
                    McpProcessStatus::Running
                }
                Err(e) => {
                    entry.child = None;
                    let status = McpProcessStatus::Error {
                        message: e.to_string(),
                    };
                    entry.last_status = status.clone();
                    status
                }
            }
        } else {
            entry.last_status.clone()
        }
    }

    pub async fn status(&self, id: &str) -> McpProcessStatus {
        let mut processes = self.processes.lock().await;
        match processes.get_mut(id) {
            None => McpProcessStatus::Stopped,
            Some(entry) => Self::check_status(entry),
        }
    }

    pub async fn status_all(&self) -> HashMap<String, McpProcessStatus> {
        let mut processes = self.processes.lock().await;
        let mut result = HashMap::new();
        for (id, entry) in processes.iter_mut() {
            result.insert(id.clone(), Self::check_status(entry));
        }
        result
    }

    pub async fn stop_all(&self) {
        let mut processes = self.processes.lock().await;
        for (_, entry) in processes.iter_mut() {
            if let Some(ref mut child) = entry.child {
                let _ = child.kill().await;
            }
            entry.child = None;
            entry.last_status = McpProcessStatus::Stopped;
        }
    }
}
