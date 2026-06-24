use std::fs;
use std::path::{Path, PathBuf};

use crate::skills::models::{AgentConfig, CustomAgentInput, LinkType};

pub struct AgentRegistry {
    builtin: Vec<AgentConfig>,
    custom: Vec<AgentConfig>,
    config_path: PathBuf,
}

impl AgentRegistry {
    pub fn new(config_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(config_dir).map_err(|e| format!("Failed to create config dir: {}", e))?;

        let config_path = config_dir.join("agents.json");
        let builtin = builtin_agents();
        let custom = load_custom_agents(&config_path).unwrap_or_default();

        Ok(Self { builtin, custom, config_path })
    }

    /// Returns all agents (builtin + custom)
    pub fn all(&self) -> Vec<AgentConfig> {
        let mut result: Vec<AgentConfig> = self.builtin.clone();
        result.extend(self.custom.clone());
        result
    }

    /// Returns only custom agents
    pub fn custom_agents(&self) -> &[AgentConfig] {
        &self.custom
    }

    /// Find an agent by ID (searches both builtin and custom)
    pub fn find(&self, id: &str) -> Option<&AgentConfig> {
        self.builtin.iter().chain(self.custom.iter()).find(|a| a.id == id)
    }

    /// Find an agent by ID (mutable, for updating linked_skills)
    pub fn find_mut(&mut self, id: &str) -> Option<&mut AgentConfig> {
        if let Some(agent) = self.builtin.iter_mut().find(|a| a.id == id) {
            Some(agent)
        } else {
            self.custom.iter_mut().find(|a| a.id == id)
        }
    }

    /// Add a custom agent
    pub fn add_custom(&mut self, input: CustomAgentInput) -> Result<AgentConfig, String> {
        // Validate path exists
        let expanded = expand_path(&input.skills_path)?;
        if !expanded.exists() {
            return Err(format!("Path does not exist: {}", expanded.display()));
        }

        let id = format!("custom-{}", uuid::Uuid::new_v4());
        let agent = AgentConfig::custom(&id, &input.name, &input.skills_path, input.link_type);

        self.custom.push(agent.clone());
        self.persist()?;
        Ok(agent)
    }

    /// Remove a custom agent
    pub fn remove_custom(&mut self, id: &str) -> Result<(), String> {
        let idx = self.custom.iter().position(|a| a.id == id)
            .ok_or_else(|| format!("Custom agent not found: {}", id))?;
        self.custom.remove(idx);
        self.persist()?;
        Ok(())
    }

    /// Link a skill to an agent
    pub fn link_skill(&mut self, agent_id: &str, skill_id: &str) -> Result<(), String> {
        let agent = self.find_mut(agent_id)
            .ok_or_else(|| format!("Agent not found: {}", agent_id))?;

        if !agent.linked_skills.contains(&skill_id.to_string()) {
            agent.linked_skills.push(skill_id.to_string());
        }

        // Only persist custom agents
        if !agent.is_builtin {
            self.persist()?;
        }
        Ok(())
    }

    /// Unlink a skill from an agent
    pub fn unlink_skill(&mut self, agent_id: &str, skill_id: &str) -> Result<(), String> {
        let agent = self.find_mut(agent_id)
            .ok_or_else(|| format!("Agent not found: {}", agent_id))?;

        agent.linked_skills.retain(|s| s != skill_id);

        if !agent.is_builtin {
            self.persist()?;
        }
        Ok(())
    }

    /// Check if a skill is linked to an agent
    pub fn is_skill_linked(&self, agent_id: &str, skill_id: &str) -> bool {
        self.find(agent_id)
            .map(|a| a.linked_skills.contains(&skill_id.to_string()))
            .unwrap_or(false)
    }

    /// Persist custom agents to agents.json
    fn persist(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.custom)
            .map_err(|e| format!("Failed to serialize agents: {}", e))?;
        fs::write(&self.config_path, json)
            .map_err(|e| format!("Failed to write agents.json: {}", e))?;
        Ok(())
    }
}

/// Load custom agents from agents.json
fn load_custom_agents(path: &Path) -> Result<Vec<AgentConfig>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read agents.json: {}", e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse agents.json: {}", e))
}

/// Expand paths starting with ~
pub fn expand_path(raw: &str) -> Result<PathBuf, String> {
    if raw.starts_with("~/") {
        let home = dirs::home_dir()
            .ok_or_else(|| "Cannot determine home directory".to_string())?;
        Ok(home.join(&raw[2..]))
    } else if raw == "~" {
        dirs::home_dir()
            .ok_or_else(|| "Cannot determine home directory".to_string())
    } else {
        Ok(PathBuf::from(raw))
    }
}

/// Built-in agents (hardcoded)
fn builtin_agents() -> Vec<AgentConfig> {
    vec![
        // ── TokenViewer canonical agents (with parsers) ──
        AgentConfig::builtin(
            "claude-code",
            "Claude Code",
            "~/.claude/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "codex",
            "Codex",
            "~/.codex/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "cursor",
            "Cursor",
            "~/.cursor/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "kiro",
            "Kiro",
            "~/.kiro/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "copilot",
            "GitHub Copilot",
            "~/.github/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "kimi",
            "Kimi",
            "~/.kimi/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "antigravity",
            "Antigravity",
            "~/.antigravity/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "zed",
            "Zed",
            "~/.zed/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "trae",
            "Trae",
            "~/.trae/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "windsurf",
            "Windsurf",
            "~/.windsurf/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "qoder",
            "Qoder",
            "~/.qoder/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "codebuddy",
            "CodeBuddy",
            "~/.codebuddy/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "workbuddy",
            "WorkBuddy",
            "~/.workbuddy/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "gemini",
            "Gemini",
            "~/.gemini/skills",
            LinkType::Directory,
        ),
        // ── Additional TokenViewer agents (parser-only, no limits) ──
        AgentConfig::builtin(
            "opencode",
            "OpenCode",
            "~/.opencode/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "openclaw",
            "OpenClaw",
            "~/.openclaw/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "hermes",
            "Hermes",
            "~/.hermes/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "grok",
            "Grok",
            "~/.grok/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "roocode",
            "RooCode",
            "~/.roocode/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "kilocode",
            "KiloCode",
            "~/.kilocode/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "kilocli",
            "Kilo CLI",
            "~/.kilocli/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "goose",
            "Goose",
            "~/.goose/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "ohmypi",
            "OhMyPi",
            "~/.ohmypi/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "pi",
            "Pi",
            "~/.pi/skills",
            LinkType::Directory,
        ),
        AgentConfig::builtin(
            "craft",
            "Craft Agent",
            "~/.craft-agent/skills",
            LinkType::Directory,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use tempfile::TempDir;

    #[test]
    fn test_builtin_agents() {
        let dir = TempDir::new().unwrap();
        let registry = AgentRegistry::new(dir.path()).unwrap();
        let agents = registry.all();
        assert_eq!(agents.len(), 25);
        assert!(agents.iter().any(|a| a.id == "claude-code"));
        assert!(agents.iter().any(|a| a.id == "cursor"));
        assert!(agents.iter().all(|a| a.is_builtin));
    }

    #[test]
    fn test_add_custom_agent() {
        let dir = TempDir::new().unwrap();

        // Create a dummy directory to use as skills path
        let skills_dir = dir.path().join("test-skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let mut registry = AgentRegistry::new(dir.path()).unwrap();

        let input = CustomAgentInput {
            name: "Test Agent".into(),
            skills_path: skills_dir.to_string_lossy().to_string(),
            link_type: LinkType::Directory,
        };

        let agent = registry.add_custom(input).unwrap();
        assert_eq!(agent.name, "Test Agent");
        assert!(agent.id.starts_with("custom-"));
        assert!(!agent.is_builtin);

        // Verify persistence
        let agents = registry.all();
        assert_eq!(agents.len(), 26); // 25 builtin + 1 custom
        assert!(agents.iter().any(|a| a.id == agent.id));
    }

    #[test]
    fn test_remove_custom_agent() {
        let dir = TempDir::new().unwrap();
        let skills_dir = dir.path().join("test-skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let mut registry = AgentRegistry::new(dir.path()).unwrap();

        let input = CustomAgentInput {
            name: "To Remove".into(),
            skills_path: skills_dir.to_string_lossy().to_string(),
            link_type: LinkType::Directory,
        };
        let agent = registry.add_custom(input).unwrap();
        assert_eq!(registry.all().len(), 26);

        registry.remove_custom(&agent.id).unwrap();
        assert_eq!(registry.all().len(), 25);
    }

    #[test]
    fn test_link_and_unlink_skill() {
        let dir = TempDir::new().unwrap();
        let skills_dir = dir.path().join("test-skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let mut registry = AgentRegistry::new(dir.path()).unwrap();

        let input = CustomAgentInput {
            name: "Test Agent".into(),
            skills_path: skills_dir.to_string_lossy().to_string(),
            link_type: LinkType::Directory,
        };
        let agent = registry.add_custom(input).unwrap();

        // Link a skill
        registry.link_skill(&agent.id, "code-review").unwrap();
        assert!(registry.is_skill_linked(&agent.id, "code-review"));

        // Link another
        registry.link_skill(&agent.id, "commit-msg").unwrap();
        assert!(registry.is_skill_linked(&agent.id, "commit-msg"));

        // Unlink
        registry.unlink_skill(&agent.id, "code-review").unwrap();
        assert!(!registry.is_skill_linked(&agent.id, "code-review"));
        assert!(registry.is_skill_linked(&agent.id, "commit-msg"));
    }

    #[test]
    fn test_expand_path() {
        let home = env::var("HOME").unwrap();

        assert_eq!(
            expand_path("~/test").unwrap(),
            PathBuf::from(format!("{}/test", home))
        );
        assert_eq!(
            expand_path("/absolute/path").unwrap(),
            PathBuf::from("/absolute/path")
        );
        assert_eq!(
            expand_path("relative/path").unwrap(),
            PathBuf::from("relative/path")
        );
    }

    #[test]
    fn test_add_custom_invalid_path() {
        let dir = TempDir::new().unwrap();
        let mut registry = AgentRegistry::new(dir.path()).unwrap();

        let input = CustomAgentInput {
            name: "Bad Agent".into(),
            skills_path: "/nonexistent/path/12345".into(),
            link_type: LinkType::Directory,
        };

        let result = registry.add_custom(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_find_agent() {
        let dir = TempDir::new().unwrap();
        let registry = AgentRegistry::new(dir.path()).unwrap();

        let found = registry.find("claude-code");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Claude Code");

        let not_found = registry.find("nonexistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_persistence() {
        let dir = TempDir::new().unwrap();
        let skills_dir = dir.path().join("test-skills");
        fs::create_dir_all(&skills_dir).unwrap();

        // Create and add agent
        let mut registry = AgentRegistry::new(dir.path()).unwrap();
        let input = CustomAgentInput {
            name: "Persistent Agent".into(),
            skills_path: skills_dir.to_string_lossy().to_string(),
            link_type: LinkType::Directory,
        };
        let agent = registry.add_custom(input).unwrap();
        drop(registry);

        // Reload and verify
        let registry2 = AgentRegistry::new(dir.path()).unwrap();
        let reloaded = registry2.find(&agent.id);
        assert!(reloaded.is_some());
        assert_eq!(reloaded.unwrap().name, "Persistent Agent");
    }
}
