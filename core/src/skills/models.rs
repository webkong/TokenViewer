use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct SkillCommandResult {
    pub ok: bool,
    pub error: Option<String>,
}

impl SkillCommandResult {
    pub fn ok() -> Self {
        Self { ok: true, error: None }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self { ok: false, error: Some(message.into()) }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LinkType {
    Directory,
    SingleFile,
    Overlay,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEntry {
    pub id: String,
    pub manifest: SkillManifest,
    pub source_dir: String,
    pub installed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub compatible_agents: Vec<String>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusInfo {
    pub status: String,
    pub message: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub ahead: i32,
    #[serde(default)]
    pub behind: i32,
    #[serde(default)]
    pub has_changes: bool,
    #[serde(default)]
    pub changes: Vec<PendingChange>,
}

impl GitStatusInfo {
    pub fn idle() -> Self {
        Self {
            status: "idle".into(), message: None,
            branch: None, ahead: 0, behind: 0, has_changes: false, changes: vec![],
        }
    }
    pub fn modified(message: &str) -> Self {
        Self {
            status: "modified".into(), message: Some(message.into()),
            branch: None, ahead: 0, behind: 0, has_changes: true, changes: vec![],
        }
    }
    pub fn conflicted(message: &str) -> Self {
        Self {
            status: "conflicted".into(), message: Some(message.into()),
            branch: None, ahead: 0, behind: 0, has_changes: true, changes: vec![],
        }
    }
    pub fn pushing() -> Self {
        Self {
            status: "pushing".into(), message: None,
            branch: None, ahead: 0, behind: 0, has_changes: false, changes: vec![],
        }
    }
    pub fn synced() -> Self {
        Self {
            status: "synced".into(), message: None,
            branch: None, ahead: 0, behind: 0, has_changes: false, changes: vec![],
        }
    }
    pub fn error(message: &str) -> Self {
        Self {
            status: "error".into(), message: Some(message.into()),
            branch: None, ahead: 0, behind: 0, has_changes: false, changes: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingChange {
    pub file_path: String,
    pub change_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitConnectivity {
    pub status: String,
    pub message: Option<String>,
}

