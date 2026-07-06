pub mod git_engine;
pub mod install;
pub mod models;
pub mod provider_config;
pub mod scanner;
pub mod storage;
pub mod symlink;

use std::collections::HashSet;
use std::path::PathBuf;

use crate::storage::Database;

use self::git_engine::GitEngine;
use self::install::SkillInstaller;
use self::models::{SkillInstallRequest, SkillInstallResponse};
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn test_core(source_root: PathBuf, codex_skills: PathBuf, config_dir: PathBuf) -> SkillsCore {
        test_core_for_agent(source_root, "codex", codex_skills, config_dir)
    }

    fn test_core_for_agent(
        source_root: PathBuf,
        agent_id: &str,
        skills_path: PathBuf,
        config_dir: PathBuf,
    ) -> SkillsCore {
        let mut registry = ProviderSkillsRegistry::new(&config_dir).unwrap();
        registry
            .set_override(
                agent_id,
                Some(skills_path.to_string_lossy().to_string()),
                None,
            )
            .unwrap();

        SkillsCore {
            registry,
            scanner: Scanner::new(source_root.clone()),
            symlink: SymlinkManager::new(source_root.clone()),
            git: None,
            config_dir,
            source_root,
            source_root_display: String::new(),
            known_skill_ids: HashSet::new(),
            git_token: None,
            git_remote_url: None,
            git_platform: None,
            git_user_name: None,
            git_user_email: None,
        }
    }

    #[test]
    fn organize_codex_nested_system_skill() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("shared-skills");
        let codex_skills = dir.path().join(".codex").join("skills");
        let config_dir = dir.path().join(".agents");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&config_dir).unwrap();

        let codex_skill = codex_skills.join(".system").join("imagegen");
        fs::create_dir_all(&codex_skill).unwrap();
        fs::write(codex_skill.join("SKILL.md"), "# Imagegen\n").unwrap();

        let mut core = test_core(source_root.clone(), codex_skills, config_dir);
        core.organize_skill("imagegen", "codex").unwrap();

        let shared_skill = source_root.join("imagegen");
        assert!(shared_skill.exists());
        assert!(codex_skill.is_symlink());
        assert_eq!(fs::read_link(&codex_skill).unwrap(), shared_skill);
        assert!(core.registry.is_skill_linked("codex", "imagegen"));
    }

    /// The nested-layout fallback must work for any agent, not just Codex.
    #[test]
    fn organize_nested_skill_for_generic_agent() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("shared-skills");
        let agent_skills = dir.path().join(".cursor").join("skills");
        let config_dir = dir.path().join(".agents");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&config_dir).unwrap();

        let nested_skill = agent_skills.join("bundled").join("formatter");
        fs::create_dir_all(&nested_skill).unwrap();
        fs::write(nested_skill.join("SKILL.md"), "# Formatter\n").unwrap();

        // Use "cursor" (a builtin agent) with an overridden skills_path, to prove
        // the fallback generalizes across agents rather than being Codex-specific.
        let mut core = test_core_for_agent(source_root.clone(), "cursor", agent_skills, config_dir);

        core.organize_skill("formatter", "cursor").unwrap();

        let shared_skill = source_root.join("formatter");
        assert!(shared_skill.exists());
        assert!(nested_skill.is_symlink());
        assert_eq!(fs::read_link(&nested_skill).unwrap(), shared_skill);
        assert!(core.registry.is_skill_linked("cursor", "formatter"));
    }
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

    pub fn install_skills(&self, req: SkillInstallRequest) -> Result<SkillInstallResponse, String> {
        SkillInstaller::new(self.source_root.clone(), self.config_dir.clone()).install(req)
    }

    pub fn organize_skill(&mut self, skill_id: &str, agent_id: &str) -> Result<(), String> {
        let agent = self
            .registry
            .find(agent_id)
            .ok_or_else(|| format!("Agent not found: {}", agent_id))?;
        let source_dir = self
            .symlink
            .resolve_agent_skill_dir(&agent, skill_id, &self.scanner)?;
        self.symlink
            .organize_skill_from_source(skill_id, &source_dir)?;
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
