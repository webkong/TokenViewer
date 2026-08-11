pub mod git_engine;
pub mod install;
pub mod models;
pub mod agent_config;
pub mod scanner;
pub mod storage;
pub mod symlink;

use std::collections::HashSet;
use std::path::PathBuf;

use crate::storage::Database;

use self::git_engine::GitEngine;
use self::install::SkillInstaller;
use self::models::{SkillInstallRequest, SkillInstallResponse};
use self::agent_config::AgentRegistry;
use self::scanner::Scanner;
use self::symlink::SymlinkManager;

pub struct SkillsCore {
    pub registry: AgentRegistry,
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
    /// Remote branch used by all Skill Git pull and push operations.
    pub git_branch: String,
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
        let mut registry = AgentRegistry::new(&config_dir).unwrap();
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
            git_branch: "main".to_string(),
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

    #[test]
    fn ensure_git_initialized_creates_repo_for_plain_source_root() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("shared-skills");
        let codex_skills = dir.path().join(".codex").join("skills");
        let config_dir = dir.path().join(".agents");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&config_dir).unwrap();

        let mut core = test_core(source_root.clone(), codex_skills, config_dir);
        assert!(core.git.is_none());

        core.ensure_git_initialized().unwrap();

        assert!(core.git.is_some());
        assert!(source_root.join(".git").exists());
    }

    #[test]
    fn delete_skill_cleans_links_and_install_metadata() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("shared-skills");
        let agent_skills = dir.path().join("agent-skills");
        let config_dir = dir.path().join(".agents");
        let install_source = dir.path().join("install-source");
        for skill_id in ["delete-me", "keep-me"] {
            let skill_dir = install_source.join(skill_id);
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(skill_dir.join("SKILL.md"), format!("# {}\n", skill_id)).unwrap();
        }

        let mut core = test_core_for_agent(
            source_root.clone(),
            "cursor",
            agent_skills.clone(),
            config_dir,
        );
        core.install_skills(SkillInstallRequest {
            source_type: "folder".to_string(),
            path: Some(install_source.to_string_lossy().to_string()),
            git_url: None,
            github_token: None,
            replace_existing: false,
            selected_skill_ids: vec!["delete-me".to_string(), "keep-me".to_string()],
        })
        .unwrap();

        let agent = core.registry.find("cursor").unwrap();
        core.symlink.create_skill_link(&agent, "delete-me").unwrap();
        core.registry.link_skill("cursor", "delete-me").unwrap();
        assert!(agent_skills.join("delete-me").is_symlink());

        core.delete_skill("delete-me").unwrap();

        assert!(!source_root.join("delete-me").exists());
        assert!(source_root.join("keep-me").exists());
        assert!(!agent_skills.join("delete-me").is_symlink());
        assert!(!core.registry.is_skill_linked("cursor", "delete-me"));
        let metadata = fs::read_to_string(dir.path().join(".tokenviewer/install.json")).unwrap();
        let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();
        assert!(metadata["skills"].get("delete-me").is_none());
        assert!(metadata["skills"].get("keep-me").is_some());

        let error = core.delete_skill("delete-me").unwrap_err();
        assert!(error.contains("Skill not found"));

        let agent = core.registry.find("cursor").unwrap();
        core.symlink.create_skill_link(&agent, "keep-me").unwrap();
        core.registry.link_skill("cursor", "keep-me").unwrap();
        fs::remove_dir_all(source_root.join("keep-me")).unwrap();
        core.cleanup_missing_skill_metadata().unwrap();
        assert!(agent_skills.join("keep-me").is_symlink());
        assert!(core.registry.is_skill_linked("cursor", "keep-me"));
        assert!(!dir.path().join(".tokenviewer/install.json").exists());
    }

    #[test]
    fn delete_skill_rebuilds_single_file_with_remaining_skills() {
        let dir = TempDir::new().unwrap();
        let source_root = dir.path().join("shared-skills");
        let merged_file = dir.path().join("AGENT.md");
        let config_dir = dir.path().join(".agents");
        for (skill_id, content) in [("delete-me", "Delete me"), ("keep-me", "Keep me")] {
            let skill_dir = source_root.join(skill_id);
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(skill_dir.join("SKILL.md"), content).unwrap();
        }

        let mut core = test_core_for_agent(
            source_root.clone(),
            "cursor",
            merged_file.clone(),
            config_dir,
        );
        core.registry
            .set_override("cursor", None, Some(self::models::LinkType::SingleFile))
            .unwrap();
        for skill_id in ["delete-me", "keep-me"] {
            core.registry.link_skill("cursor", skill_id).unwrap();
        }
        let agent = core.registry.find("cursor").unwrap();
        core.symlink
            .rebuild_single_file(&agent, &agent.linked_skills)
            .unwrap();

        core.delete_skill("delete-me").unwrap();

        let content = fs::read_to_string(&merged_file).unwrap();
        assert!(!content.contains("Delete me"));
        assert!(content.contains("Keep me"));
        assert!(core.registry.is_skill_linked("cursor", "keep-me"));
        assert!(!core.registry.is_skill_linked("cursor", "delete-me"));
    }
}

impl SkillsCore {
    pub fn new(db: &Database, source_root: PathBuf) -> Result<Self, String> {
        db.migrate_skills_schema().map_err(|e| e.to_string())?;

        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let config_dir = home.join(".agents");
        let registry = AgentRegistry::new(&config_dir).map_err(|e| e.to_string())?;
        let scanner = Scanner::new(source_root.clone());
        let symlink = SymlinkManager::new(source_root.clone());
        let git = GitEngine::open(&source_root).ok();

        let known_skill_ids = scanner
            .scan_all()
            .unwrap_or_default()
            .into_iter()
            .map(|skill| skill.id)
            .collect();

        let mut core = Self {
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
            git_branch: "main".to_string(),
            git_user_name: None,
            git_user_email: None,
        };
        if let Err(error) = core.cleanup_missing_skill_metadata() {
            eprintln!("Failed to clean stale Skill metadata: {}", error);
        }
        Ok(core)
    }

    pub fn ensure_git_initialized(&mut self) -> Result<(), String> {
        self.ensure_git_initialized_with_identity(None, None)
    }

    pub fn ensure_git_initialized_with_identity(
        &mut self,
        user_name: Option<&str>,
        user_email: Option<&str>,
    ) -> Result<(), String> {
        if self.git.is_none() {
            self.git = Some(GitEngine::open_or_init_with_identity(
                &self.source_root,
                user_name,
                user_email,
            )?);
        }
        Ok(())
    }

    pub fn delete_skill(&mut self, skill_id: &str) -> Result<(), String> {
        let path = self.source_root.join(skill_id);
        if !path.exists() && !path.is_symlink() {
            return Err(format!("Skill not found: {}", path.display()));
        }
        let linked_agents = self
            .registry
            .all()
            .into_iter()
            .filter(|agent| agent.linked_skills.iter().any(|id| id == skill_id))
            .collect::<Vec<_>>();
        for agent in &linked_agents {
            if agent.link_type == self::models::LinkType::SingleFile {
                let remaining = agent
                    .linked_skills
                    .iter()
                    .filter(|id| id.as_str() != skill_id)
                    .cloned()
                    .collect::<Vec<_>>();
                self.symlink.rebuild_single_file(agent, &remaining)?;
            } else {
                self.symlink.remove_skill_link(agent, skill_id)?;
            }
        }

        if path.is_symlink() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("Failed to delete skill {}: {}", path.display(), e))?;
        } else if path.exists() {
            std::fs::remove_dir_all(&path)
                .map_err(|e| format!("Failed to delete skill {}: {}", path.display(), e))?;
        }

        self.registry.unlink_skill_from_all(skill_id)?;
        SkillInstaller::new(self.source_root.clone(), self.config_dir.clone())
            .remove_install_record(skill_id)?;
        self.known_skill_ids.remove(skill_id);
        Ok(())
    }

    fn cleanup_missing_skill_metadata(&mut self) -> Result<(), String> {
        if !self.source_root.is_dir() {
            eprintln!(
                "Skipped stale Skill metadata cleanup because source root is unavailable: {}",
                self.source_root.display()
            );
            return Ok(());
        }

        // A missing source directory can be transient (for example during migration or when the
        // library lives on a temporarily unavailable volume). Keep agent associations and
        // generated SingleFile content intact; only install records whose recorded destination is
        // definitely absent are safe to prune here.
        for agent in self.registry.all() {
            for skill_id in &agent.linked_skills {
                let source = self.source_root.join(skill_id);
                if !source.exists() {
                    eprintln!(
                        "Preserved linked Skill metadata for missing source {} (agent {})",
                        source.display(),
                        agent.source
                    );
                }
            }
        }

        let installer = SkillInstaller::new(self.source_root.clone(), self.config_dir.clone());
        for skill_id in installer.prune_missing_install_records()? {
            self.known_skill_ids.remove(&skill_id);
            eprintln!(
                "Removed stale Skill install record for missing destination: {}",
                skill_id
            );
        }
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
