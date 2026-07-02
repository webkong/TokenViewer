use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct SkillCommandResult {
    pub ok: bool,
    pub error: Option<String>,
}

impl SkillCommandResult {
    pub fn ok() -> Self {
        Self {
            ok: true,
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(message.into()),
        }
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
    #[serde(default)]
    pub agent_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub compatible_agents: Vec<String>,
    pub version: String,
    /// false (default) when synthesized because no manifest.json was found;
    /// true when loaded from a user-authored manifest.json on disk.
    #[serde(default)]
    pub has_manifest: bool,
}

impl SkillManifest {
    /// Source-aware compatible_agents merge. When a skill has no user-authored
    /// manifest, replace the default `["*"]` wildcard with the concrete agent
    /// whose on-disk `skills_path` is known to contain the skill. Subsequent
    /// agents accumulate into the list. Skills with a manifest keep their
    /// author-declared list untouched (this function is a no-op for them).
    pub fn merge_compatible_agent(&mut self, agent_id: &str) {
        if self.has_manifest {
            return;
        }
        if self.compatible_agents.len() == 1 && self.compatible_agents[0] == "*" {
            self.compatible_agents = vec![agent_id.to_string()];
        } else if !self.compatible_agents.contains(&agent_id.to_string()) {
            self.compatible_agents.push(agent_id.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_compat_replaces_wildcard_with_first_agent() {
        let mut m = SkillManifest {
            name: "review".into(),
            description: "review skill".into(),
            tags: vec![],
            compatible_agents: vec!["*".into()],
            version: "unknown".into(),
            has_manifest: false,
        };
        m.merge_compatible_agent("codex");
        assert_eq!(m.compatible_agents, vec!["codex".to_string()]);
    }

    #[test]
    fn merge_compat_accumulates_multiple_agents() {
        let mut m = SkillManifest {
            name: "review".into(),
            description: "review skill".into(),
            tags: vec![],
            compatible_agents: vec!["*".into()],
            version: "unknown".into(),
            has_manifest: false,
        };
        m.merge_compatible_agent("codex");
        m.merge_compatible_agent("claude");
        assert_eq!(m.compatible_agents, vec!["codex".to_string(), "claude".to_string()]);
    }

    #[test]
    fn merge_compat_does_not_duplicate() {
        let mut m = SkillManifest {
            name: "review".into(),
            description: "review skill".into(),
            tags: vec![],
            compatible_agents: vec!["*".into()],
            version: "unknown".into(),
            has_manifest: false,
        };
        m.merge_compatible_agent("codex");
        m.merge_compatible_agent("codex");
        assert_eq!(m.compatible_agents, vec!["codex".to_string()]);
    }

    #[test]
    fn merge_compat_respects_user_manifest() {
        let mut m = SkillManifest {
            name: "review".into(),
            description: "review skill".into(),
            tags: vec![],
            compatible_agents: vec!["codex".into(), "cursor".into()],
            version: "1.0".into(),
            has_manifest: true,
        };
        // Even if called for another agent, user-authored values win.
        m.merge_compatible_agent("claude");
        assert_eq!(m.compatible_agents, vec!["codex".to_string(), "cursor".to_string()]);
    }

    #[test]
    fn merge_compat_noop_on_non_wildcard_without_manifest() {
        // Edge: a synthesized manifest shouldn't normally have a non-wildcard
        // list, but if it does, we still accumulate without clobbering.
        let mut m = SkillManifest {
            name: "review".into(),
            description: "review skill".into(),
            tags: vec![],
            compatible_agents: vec!["opencode".into()],
            version: "unknown".into(),
            has_manifest: false,
        };
        m.merge_compatible_agent("codex");
        assert_eq!(m.compatible_agents, vec!["opencode".to_string(), "codex".to_string()]);
    }
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
            status: "idle".into(),
            message: None,
            branch: None,
            ahead: 0,
            behind: 0,
            has_changes: false,
            changes: vec![],
        }
    }
    pub fn modified(message: &str) -> Self {
        Self {
            status: "modified".into(),
            message: Some(message.into()),
            branch: None,
            ahead: 0,
            behind: 0,
            has_changes: true,
            changes: vec![],
        }
    }
    pub fn conflicted(message: &str) -> Self {
        Self {
            status: "conflicted".into(),
            message: Some(message.into()),
            branch: None,
            ahead: 0,
            behind: 0,
            has_changes: true,
            changes: vec![],
        }
    }
    pub fn pushing() -> Self {
        Self {
            status: "pushing".into(),
            message: None,
            branch: None,
            ahead: 0,
            behind: 0,
            has_changes: false,
            changes: vec![],
        }
    }
    pub fn synced() -> Self {
        Self {
            status: "synced".into(),
            message: None,
            branch: None,
            ahead: 0,
            behind: 0,
            has_changes: false,
            changes: vec![],
        }
    }
    pub fn error(message: &str) -> Self {
        Self {
            status: "error".into(),
            message: Some(message.into()),
            branch: None,
            ahead: 0,
            behind: 0,
            has_changes: false,
            changes: vec![],
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
