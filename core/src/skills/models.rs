use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct SkillCommandResult {
    pub ok: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillInstallRequest {
    pub source_type: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub git_url: Option<String>,
    #[serde(default)]
    pub replace_existing: bool,
    #[serde(default, alias = "selected_skill_i_ds")]
    pub selected_skill_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillInstallCandidate {
    pub id: String,
    pub source_dir: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillInstallResponse {
    pub ok: bool,
    pub status: String,
    #[serde(default)]
    pub installed_skill_ids: Vec<String>,
    #[serde(default)]
    pub candidates: Vec<SkillInstallCandidate>,
    #[serde(default)]
    pub error: Option<String>,
}

impl SkillInstallResponse {
    pub fn installed(ids: Vec<String>) -> Self {
        Self {
            ok: true,
            status: "installed".to_string(),
            installed_skill_ids: ids,
            candidates: Vec::new(),
            error: None,
        }
    }

    pub fn selection_required(candidates: Vec<SkillInstallCandidate>) -> Self {
        Self {
            ok: true,
            status: "selection_required".to_string(),
            installed_skill_ids: Vec::new(),
            candidates,
            error: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            status: "error".to_string(),
            installed_skill_ids: Vec::new(),
            candidates: Vec::new(),
            error: Some(message.into()),
        }
    }
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
    #[serde(default)]
    pub is_built_in: bool,
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

    /// Simulates the ffi.rs scan ordering: a manifest-less global skill is
    /// inserted first with ["*"] (source_root scan), then re-encountered via an
    /// agent's post-organize symlink. With the global-skill guard in place, the
    /// wildcard must survive — otherwise the skill falsely appears single-agent.
    #[test]
    fn global_skill_not_narrowed_by_agent_symlink() {
        use std::collections::HashSet;

        let global_id = "global-review";
        let source_root_ids: HashSet<String> = [global_id.to_string()].into();

        // Simulate source_root scan insert.
        let mut global_manifest = SkillManifest {
            name: "global-review".into(),
            description: "review skill".into(),
            tags: vec![],
            compatible_agents: vec!["*".into()],
            version: "unknown".into(),
            has_manifest: false,
        };

        // Simulate an agent-scan encounter of the same skill via symlink.
        // The guard `!source_root_ids.contains(...)` means merge is skipped.
        let is_global = source_root_ids.contains(global_id);
        if !is_global {
            global_manifest.merge_compatible_agent("codex");
        }

        assert_eq!(global_manifest.compatible_agents, vec!["*".to_string()]);
    }

    /// The inverse: a skill discovered only via agent dirs (never source_root)
    /// must accumulate agents normally.
    #[test]
    fn agent_only_skill_accumulates_agents() {
        use std::collections::HashSet;

        let agent_only_id = "codex-review";
        let source_root_ids: HashSet<String> = HashSet::new();

        let mut m = SkillManifest {
            name: "codex-review".into(),
            description: "review skill".into(),
            tags: vec![],
            compatible_agents: vec!["*".into()],
            version: "unknown".into(),
            has_manifest: false,
        };

        let is_global = source_root_ids.contains(agent_only_id);
        if !is_global {
            m.merge_compatible_agent("codex");
        }

        assert_eq!(m.compatible_agents, vec!["codex".to_string()]);
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
