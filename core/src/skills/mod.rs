pub mod git_engine;
pub mod models;
pub mod provider_config;
pub mod scanner;
pub mod storage;
pub mod symlink;

use std::collections::HashSet;
use std::path::PathBuf;

use crate::storage::Database;

use self::git_engine::GitEngine;
use self::provider_config::ProviderSkillsRegistry;
use self::scanner::Scanner;
use self::symlink::SymlinkManager;

pub struct SkillsCore {
    pub registry: ProviderSkillsRegistry,
    pub scanner: Scanner,
    pub symlink: SymlinkManager,
    pub git: Option<GitEngine>,
    pub config_dir: PathBuf,
    pub source_root: PathBuf,
    pub source_root_display: String,
    pub known_skill_ids: HashSet<String>,
    /// Git auth token (set by FFI, stored in memory for the session).
    pub git_token: Option<String>,
    /// Git remote URL (set by FFI).
    pub git_remote_url: Option<String>,
    /// Git platform: "github", "gitlab", or "custom".
    pub git_platform: Option<String>,
    /// Optional commit author/committer name for sync commits.
    pub git_user_name: Option<String>,
    /// Optional commit author/committer email for sync commits.
    pub git_user_email: Option<String>,
}

impl SkillsCore {
    pub fn new(db: &Database, source_root: PathBuf) -> Result<Self, String> {
        db.migrate_skills_schema().map_err(|e| e.to_string())?;

        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let config_dir = home.join(".agents");
        let registry = ProviderSkillsRegistry::new(&config_dir).map_err(|e| e.to_string())?;
        let scanner = Scanner::new(source_root.clone());
        let symlink = SymlinkManager::new(source_root.clone());
        let git = GitEngine::open(&source_root).ok();

        let known_skill_ids = scanner
            .scan_all()
            .unwrap_or_default()
            .into_iter()
            .map(|skill| skill.id)
            .collect();

        Ok(Self {
            registry,
            scanner,
            symlink,
            git,
            config_dir,
            source_root,
            source_root_display: String::new(),
            known_skill_ids,
            git_token: None,
            git_remote_url: None,
            git_platform: None,
            git_user_name: None,
            git_user_email: None,
        })
    }

    pub fn delete_skill(&self, skill_id: &str) -> Result<(), String> {
        let path = self.source_root.join(skill_id);
        if !path.exists() {
            return Err(format!("Skill not found: {}", path.display()));
        }
        std::fs::remove_dir_all(&path)
            .map_err(|e| format!("Failed to delete skill {}: {}", path.display(), e))?;
        Ok(())
    }

    pub fn organize_skill(&mut self, skill_id: &str, agent_id: &str) -> Result<(), String> {
        let agent = self
            .registry
            .find(agent_id)
            .ok_or_else(|| format!("Agent not found: {}", agent_id))?;
        self.symlink.organize_skill(&agent, skill_id)?;
        self.registry.link_skill(agent_id, skill_id)?;
        Ok(())
    }

    pub fn restore_skill(&mut self, skill_id: &str, agent_id: &str) -> Result<(), String> {
        let agent = self
            .registry
            .find(agent_id)
            .ok_or_else(|| format!("Agent not found: {}", agent_id))?;
        let other_linked: Vec<String> = self
            .registry
            .all()
            .iter()
            .filter(|a| a.source != agent_id && a.linked_skills.contains(&skill_id.to_string()))
            .map(|a| a.source.clone())
            .collect();
        self.symlink
            .restore_skill(skill_id, &agent, &other_linked)?;
        self.registry.unlink_skill(agent_id, skill_id)?;
        Ok(())
    }
}
