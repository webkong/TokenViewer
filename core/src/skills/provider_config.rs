use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::skills::models::LinkType;

/// Canonical provider skills configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderSkillsConfig {
    pub source: String,
    pub display_name: String,
    pub skills_path: String,
    pub link_type: LinkType,
    #[serde(default)]
    pub is_linked: bool,
    #[serde(default)]
    pub linked_skills: Vec<String>,
    #[serde(default)]
    pub has_parser: bool,
    /// Has subscription/quota tracking (limits panel).
    #[serde(default)]
    pub has_limits: bool,
    /// CLI binary name for install detection (from Orca tui-agent-config.ts).
    /// None means the agent has no standalone CLI (IDE/plugin-only).
    #[serde(default)]
    pub detect_cmd: Option<String>,
    /// Whether the detect_cmd binary was found on PATH at last check.
    #[serde(default)]
    pub is_installed: bool,
}

impl ProviderSkillsConfig {
    /// Convenience constructor for tests.
    pub fn custom(source: &str, name: &str, skills_path: &str, link_type: LinkType) -> Self {
        Self {
            source: source.to_string(),
            display_name: name.to_string(),
            skills_path: skills_path.to_string(),
            link_type,
            is_linked: false,
            linked_skills: Vec::new(),
            has_parser: false,
            has_limits: false,
            detect_cmd: None,
            is_installed: false,
        }
    }
}

/// Registry of all providers with skills configuration.
/// Builtin providers are derived from parser sources + additional skill-only agents.
pub struct ProviderSkillsRegistry {
    builtin: Vec<ProviderSkillsConfig>,
    /// Custom overrides for provider skills paths.
    overrides: HashMap<String, ProviderSkillsOverrides>,
    /// Config directory for persistent linked_skills
    config_dir: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ProviderSkillsOverrides {
    skills_path: Option<String>,
    link_type: Option<LinkType>,
    linked_skills: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistedLinkedSkills {
    /// Map from source name to list of linked skill IDs.
    linked: HashMap<String, Vec<String>>,
}

impl ProviderSkillsRegistry {
    pub fn new(config_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(config_dir)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;

        let builtin = builtin_providers();
        let overrides = Self::load_overrides(config_dir).unwrap_or_default();

        Ok(Self {
            builtin,
            overrides,
            config_dir: config_dir.to_path_buf(),
        })
    }

    /// Returns all providers (builtin merged with overrides).
    pub fn all(&self) -> Vec<ProviderSkillsConfig> {
        let linked_skills_map = self.load_linked_skills();

        self.builtin
            .iter()
            .map(|b| {
                let mut config = b.clone();
                if let Some(ov) = self.overrides.get(&b.source) {
                    if let Some(ref sp) = ov.skills_path {
                        config.skills_path = sp.clone();
                    }
                    if let Some(ref lt) = ov.link_type {
                        config.link_type = lt.clone();
                    }
                    if !ov.linked_skills.is_empty() {
                        config.linked_skills = ov.linked_skills.clone();
                    }
                }
                // Merge linked skills from persistence
                if let Some(linked) = linked_skills_map.get(&b.source) {
                    for sid in linked {
                        if !config.linked_skills.contains(sid) {
                            config.linked_skills.push(sid.clone());
                        }
                    }
                }
                config.is_linked = !config.linked_skills.is_empty();
                config
            })
            .collect()
    }

    /// Find a provider config by source name. Uses canonical name.
    pub fn find(&self, source: &str) -> Option<ProviderSkillsConfig> {
        let canonical = canonical_source(source);
        self.builtin
            .iter()
            .find(|b| b.source == canonical)
            .map(|b| {
                let mut config = b.clone();
                if let Some(ov) = self.overrides.get(canonical) {
                    if let Some(ref sp) = ov.skills_path {
                        config.skills_path = sp.clone();
                    }
                    if let Some(ref lt) = ov.link_type {
                        config.link_type = lt.clone();
                    }
                }
                let linked_map = self.load_linked_skills();
                if let Some(linked) = linked_map.get(canonical) {
                    for sid in linked {
                        if !config.linked_skills.contains(sid) {
                            config.linked_skills.push(sid.clone());
                        }
                    }
                }
                config
            })
    }

    /// Set per-provider override (skills_path / link_type).
    pub fn set_override(
        &mut self,
        source: &str,
        skills_path: Option<String>,
        link_type: Option<LinkType>,
    ) -> Result<(), String> {
        let canonical = canonical_source(source);
        let entry = self
            .overrides
            .entry(canonical.to_string())
            .or_insert_with(|| ProviderSkillsOverrides {
                skills_path: None,
                link_type: None,
                linked_skills: Vec::new(),
            });
        if let Some(sp) = skills_path {
            entry.skills_path = Some(sp);
        }
        if let Some(lt) = link_type {
            entry.link_type = Some(lt);
        }
        self.persist_overrides()
    }

    /// Reset per-provider override to defaults.
    pub fn reset_override(&mut self, source: &str) -> Result<(), String> {
        let canonical = canonical_source(source);
        self.overrides.remove(canonical);
        self.persist_overrides()
    }

    /// Link a skill to a provider.
    pub fn link_skill(&mut self, source: &str, skill_id: &str) -> Result<(), String> {
        let canonical = canonical_source(source);
        let mut linked = self.load_linked_skills();
        let entry = linked.entry(canonical.to_string()).or_default();
        if !entry.contains(&skill_id.to_string()) {
            entry.push(skill_id.to_string());
        }
        self.persist_linked_skills(&linked)
    }

    /// Unlink a skill from a provider.
    pub fn unlink_skill(&mut self, source: &str, skill_id: &str) -> Result<(), String> {
        let canonical = canonical_source(source);
        let mut linked = self.load_linked_skills();
        if let Some(entry) = linked.get_mut(canonical) {
            entry.retain(|s| s != skill_id);
            if entry.is_empty() {
                linked.remove(canonical);
            }
        }
        self.persist_linked_skills(&linked)
    }

    /// Check if a skill is linked to a provider.
    pub fn is_skill_linked(&self, source: &str, skill_id: &str) -> bool {
        let canonical = canonical_source(source);
        self.load_linked_skills()
            .get(canonical)
            .map(|s| s.contains(&skill_id.to_string()))
            .unwrap_or(false)
    }

    // ── Persistence ──

    fn overrides_path(&self) -> PathBuf {
        self.config_dir.join("provider_overrides.json")
    }

    fn linked_skills_path(&self) -> PathBuf {
        self.config_dir.join("linked_skills.json")
    }

    fn load_overrides(
        config_dir: &Path,
    ) -> Result<HashMap<String, ProviderSkillsOverrides>, String> {
        let path = config_dir.join("provider_overrides.json");
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let content =
            fs::read_to_string(&path).map_err(|e| format!("Failed to read overrides: {}", e))?;
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse overrides: {}", e))
    }

    fn persist_overrides(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.overrides)
            .map_err(|e| format!("Failed to serialize overrides: {}", e))?;
        fs::write(self.overrides_path(), json)
            .map_err(|e| format!("Failed to write overrides: {}", e))
    }

    fn load_linked_skills(&self) -> HashMap<String, Vec<String>> {
        let path = self.linked_skills_path();
        if !path.exists() {
            return HashMap::new();
        }
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<PersistedLinkedSkills>(&s).ok())
            .map(|p| p.linked)
            .unwrap_or_default()
    }

    fn persist_linked_skills(&self, linked: &HashMap<String, Vec<String>>) -> Result<(), String> {
        let data = PersistedLinkedSkills {
            linked: linked.clone(),
        };
        let json = serde_json::to_string_pretty(&data)
            .map_err(|e| format!("Failed to serialize linked skills: {}", e))?;
        fs::write(self.linked_skills_path(), json)
            .map_err(|e| format!("Failed to write linked skills: {}", e))
    }
}

/// Expand paths starting with ~
pub fn expand_path(raw: &str) -> Result<PathBuf, String> {
    if raw.starts_with("~/") {
        let home = dirs::home_dir().ok_or_else(|| "Cannot determine home directory".to_string())?;
        Ok(home.join(&raw[2..]))
    } else if raw == "~" {
        dirs::home_dir().ok_or_else(|| "Cannot determine home directory".to_string())
    } else {
        Ok(PathBuf::from(raw))
    }
}

/// Detect which agents are installed. Two strategies:
/// 1. CLI check: if detect_cmd is set, run `which <cmd>` (fast, ms-level)
/// 2. Dir check: for agents without CLI (IDE/plugin), check if ~/.{source}/ exists
pub fn detect_installed_agents(providers: &[ProviderSkillsConfig]) -> Vec<(String, bool)> {
    providers
        .par_iter()
        .map(|p| {
            let installed = match &p.detect_cmd {
                Some(cmd) => is_command_on_path(cmd),
                // No CLI — fall back to checking if the agent's config directory exists
                None => is_agent_dir_present(&p.skills_path),
            };
            (p.source.clone(), installed)
        })
        .collect()
}

/// Check if an agent's config directory exists by looking at the parent of skills_path.
/// skills_path is typically ~/.{source}/skills, so ~/.{source} is the config dir.
fn is_agent_dir_present(skills_path: &str) -> bool {
    let path = std::path::Path::new(skills_path);
    // Go up one level from .../skills to check the agent's config dir
    path.parent()
        .map(|p| p.exists())
        .unwrap_or(false)
}

/// Check if a command exists on PATH with a 3-second timeout.
fn is_command_on_path(cmd: &str) -> bool {
    let mut child = match std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    // Wait with timeout — some `which` calls can hang on slow NFS/home mounts
    let pid = child.id();
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) => {
                if start.elapsed().as_secs() > 3 {
                    // Kill stale process
                    let _ = std::process::Command::new("kill")
                        .arg("-9")
                        .arg(pid.to_string())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status();
                    return false;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(_) => return false,
        }
    }
}

// ── Install status cache ──

/// Cache TTL in seconds (1 hour).
const INSTALL_CACHE_TTL_SECS: u64 = 3600;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct InstallStatusCache {
    updated_at: u64,
    statuses: HashMap<String, bool>,
}

/// Load cached install status from disk. Returns None if expired or missing.
fn load_install_cache(config_dir: &Path) -> Option<HashMap<String, bool>> {
    let cache_path = config_dir.join("install_status.json");
    let data = fs::read_to_string(&cache_path).ok()?;
    let cache: InstallStatusCache = serde_json::from_str(&data).ok()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    if now.saturating_sub(cache.updated_at) < INSTALL_CACHE_TTL_SECS {
        Some(cache.statuses)
    } else {
        None
    }
}

/// Save install status cache to disk.
fn save_install_cache(config_dir: &Path, statuses: &HashMap<String, bool>) -> Result<(), String> {
    fs::create_dir_all(config_dir).map_err(|e| format!("Failed to create config dir: {}", e))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cache = InstallStatusCache {
        updated_at: now,
        statuses: statuses.clone(),
    };
    let json = serde_json::to_string(&cache)
        .map_err(|e| format!("Failed to serialize install cache: {}", e))?;
    fs::write(config_dir.join("install_status.json"), json)
        .map_err(|e| format!("Failed to write install cache: {}", e))
}

/// Detect installed agents with caching. Returns cached results if fresh (≤1h),
/// otherwise runs parallel detection and saves to disk.
/// Pass `force: true` to skip cache and re-detect.
pub fn detect_installed_agents_cached(
    config_dir: &Path,
    providers: &[ProviderSkillsConfig],
    force: bool,
) -> Vec<(String, bool)> {
    if !force {
        if let Some(cached) = load_install_cache(config_dir) {
            // Merge cached status with current provider list
            return providers
                .iter()
                .map(|p| {
                    let installed = cached.get(&p.source).copied().unwrap_or(false);
                    (p.source.clone(), installed)
                })
                .collect();
        }
    }

    let results = detect_installed_agents(providers);
    let statuses: HashMap<String, bool> = results.iter().cloned().collect();
    let _ = save_install_cache(config_dir, &statuses);
    results
}

/// Map aliases to canonical names. Pass through all others.
pub fn canonical_source(name: &str) -> &str {
    match name {
        "claude-code" => "claude",
        "kilo" => "kilocode",
        "mimo-code" => "mimocode",
        other => other,
    }
}

/// Reverse: "claude" → "claude-code" for display, pass through others.
pub fn agent_name_for(source: &str) -> &str {
    match source {
        "claude" => "claude-code",
        other => other,
    }
}

/// Built-in providers derived from parser sources + additional skill-only agents.
fn builtin_providers() -> Vec<ProviderSkillsConfig> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let home_str = home.to_string_lossy().to_string();

    // Name → display_name mapping (same as original agent_registry)
    let display_names: HashMap<&str, &str> = HashMap::from([
        ("claude", "Claude Code"),
        ("codex", "Codex"),
        ("cursor", "Cursor"),
        ("kiro", "Kiro"),
        ("copilot", "GitHub Copilot"),
        ("kimi", "Kimi"),
        ("antigravity", "Antigravity"),
        ("zed", "Zed"),
        ("trae", "Trae"),
        ("windsurf", "Windsurf"),
        ("qoder", "Qoder"),
        ("codebuddy", "CodeBuddy"),
        ("workbuddy", "WorkBuddy"),
        ("gemini", "Gemini"),
        ("opencode", "OpenCode"),
        ("openclaw", "OpenClaw"),
        ("hermes", "Hermes"),
        ("grok", "Grok"),
        ("roocode", "RooCode"),
        ("kilocode", "KiloCode"),
        ("kilocli", "Kilo CLI"),
        ("goose", "Goose"),
        ("ohmypi", "OhMyPi"),
        ("pi", "Pi"),
        ("craft", "Craft Agent"),
        ("everycode", "EveryCode"),
        ("mimocode", "MimoCode"),
        ("zcode", "ZCode"),
        ("openclaude", "OpenClaude"),
        ("devin", "Devin"),
        ("ante", "Ante"),
        ("autohand", "Autohand Code"),
        ("aider", "Aider"),
        ("amp", "Amp"),
        ("crush", "Charm"),
        ("aug", "Auggie"),
        ("cline", "Cline"),
        ("codebuff", "Codebuff"),
        ("command-code", "Command Code"),
        ("continue", "Continue"),
        ("droid", "Droid"),
        ("mistral-vibe", "Mistral Vibe"),
        ("qwen-code", "Qwen Code"),
        ("rovo", "Rovo Dev"),
        ("omp", "OMP"),
    ]);

    // CLI binary names for install detection (from Orca tui-agent-config.ts).
    // Only agents with standalone CLI binaries have entries.
    let detect_cmds: HashMap<&str, &str> = HashMap::from([
        ("claude", "claude"),
        ("codex", "codex"),
        ("cursor", "cursor-agent"),
        ("kiro", "kiro-cli"),
        ("copilot", "copilot"),
        ("kimi", "kimi"),
        ("antigravity", "agy"),
        ("gemini", "gemini"),
        ("opencode", "opencode"),
        ("openclaw", "openclaw"),
        ("hermes", "hermes"),
        ("grok", "grok"),
        ("kilocode", "kilo"),
        ("goose", "goose"),
        ("pi", "pi"),
        ("mimocode", "mimo"),
        ("openclaude", "openclaude"),
        ("devin", "devin"),
        ("ante", "ante"),
        ("autohand", "autohand"),
        ("aider", "aider"),
        ("amp", "amp"),
        ("crush", "crush"),
        ("aug", "auggie"),
        ("cline", "cline"),
        ("codebuff", "codebuff"),
        ("command-code", "command-code"),
        ("continue", "cn"),
        ("droid", "droid"),
        ("mistral-vibe", "vibe"),
        ("qwen-code", "qwen-code"),
        ("rovo", "rovo"),
        ("omp", "omp"),
    ]);

    // Source names with has_parser (the parser sources list)
    let parser_sources: std::collections::HashSet<&str> = std::collections::HashSet::from([
        "claude",
        "codex",
        "cursor",
        "gemini",
        "kiro",
        "opencode",
        "openclaw",
        "everycode",
        "hermes",
        "copilot",
        "kimi",
        "grok",
        "antigravity",
        "roocode",
        "kilocode",
        "kilocli",
        "zed",
        "goose",
        "ohmypi",
        "pi",
        "craft",
        "codebuddy",
        "workbuddy",
        "mimocode",
        "zcode",
    ]);

    // All sources (parser + agent-only)
    let all_sources: Vec<&str> = {
        let mut sources = crate::parsers::all_parser_sources();
        // Add agent-only sources not in parsers
        for extra in [
            "trae",
            "windsurf",
            "qoder",
            // Orca-sourced agents (not in TokenViewer parsers)
            "openclaude",
            "devin",
            "ante",
            "autohand",
            "aider",
            "amp",
            "crush",
            "aug",
            "cline",
            "codebuff",
            "command-code",
            "continue",
            "droid",
            "mistral-vibe",
            "qwen-code",
            "rovo",
            "omp",
        ] {
            if !sources.contains(&extra) {
                sources.push(extra);
            }
        }
        sources
    };

    all_sources
        .into_iter()
        .map(|source| {
            let display_name = display_names.get(source).unwrap_or(&source).to_string();
            let has_parser = parser_sources.contains(source);
            let detect_cmd = detect_cmds.get(source).map(|s| s.to_string());
            // Skills path: ~/.{source}/skills
            // For claude, the agent id is "claude-code" but we use "claude" as canonical
            let skills_dir = if source == "claude" {
                ".claude".to_string()
            } else {
                format!(".{}", source)
            };
            let skills_path = format!("{}/{}/skills", home_str, skills_dir);

            ProviderSkillsConfig {
                source: source.to_string(),
                display_name,
                skills_path,
                link_type: LinkType::Directory,
                is_linked: false,
                linked_skills: Vec::new(),
                has_parser,
                detect_cmd,
                is_installed: false,
                // Providers with subscription/quota tracking for the limits panel.
                // Core rate-limit API fetchers (from Orca): claude, codex, gemini, opencode, kimi.
                // TokenViewer additionally tracks: copilot, kiro, cursor, antigravity, zed,
                // trae, windsurf, qoder, codebuddy, workbuddy, zcode.
                has_limits: matches!(
                    source,
                    "claude"
                        | "codex"
                        | "gemini"
                        | "kimi"
                        | "opencode"
                        | "copilot"
                        | "kiro"
                        | "cursor"
                        | "antigravity"
                        | "zed"
                        | "trae"
                        | "windsurf"
                        | "qoder"
                        | "codebuddy"
                        | "workbuddy"
                        | "zcode"
                ),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_builtin_providers() {
        let dir = TempDir::new().unwrap();
        let registry = ProviderSkillsRegistry::new(dir.path()).unwrap();
        let providers = registry.all();
        // Should have all parser sources + agent-only
        assert!(providers.len() >= 42);
        assert!(providers.iter().any(|a| a.source == "claude"));
        assert!(providers.iter().any(|a| a.source == "cursor"));
        // Claude should have display name "Claude Code"
        let claude = providers.iter().find(|a| a.source == "claude").unwrap();
        assert_eq!(claude.display_name, "Claude Code");
        assert!(claude.has_parser);
        // Trae should exist but have has_parser = false
        let trae = providers.iter().find(|a| a.source == "trae").unwrap();
        assert!(!trae.has_parser);
    }

    #[test]
    fn test_canonical_source() {
        assert_eq!(canonical_source("claude-code"), "claude");
        assert_eq!(canonical_source("claude"), "claude");
        assert_eq!(canonical_source("cursor"), "cursor");
        assert_eq!(canonical_source("codex"), "codex");
        assert_eq!(canonical_source("kilo"), "kilocode");
        assert_eq!(canonical_source("mimo-code"), "mimocode");
    }

    #[test]
    fn test_link_and_unlink_skill() {
        let dir = TempDir::new().unwrap();
        let mut registry = ProviderSkillsRegistry::new(dir.path()).unwrap();

        registry.link_skill("claude", "code-review").unwrap();
        assert!(registry.is_skill_linked("claude", "code-review"));
        // Also works with "claude-code"
        assert!(registry.is_skill_linked("claude-code", "code-review"));

        registry.link_skill("cursor", "commit-msg").unwrap();
        assert!(registry.is_skill_linked("cursor", "commit-msg"));

        registry.unlink_skill("claude", "code-review").unwrap();
        assert!(!registry.is_skill_linked("claude", "code-review"));
        assert!(registry.is_skill_linked("cursor", "commit-msg"));
    }

    #[test]
    fn test_set_and_reset_override() {
        let dir = TempDir::new().unwrap();
        let skills_dir = dir.path().join("custom-skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let mut registry = ProviderSkillsRegistry::new(dir.path()).unwrap();

        // Set override
        registry
            .set_override(
                "claude",
                Some(skills_dir.to_string_lossy().to_string()),
                None,
            )
            .unwrap();

        let config = registry.find("claude").unwrap();
        assert_eq!(config.skills_path, skills_dir.to_string_lossy().to_string());

        // Reset
        registry.reset_override("claude").unwrap();
        let config2 = registry.find("claude").unwrap();
        // Should be back to default ~/.claude/skills
        assert!(config2.skills_path.contains(".claude/skills"));
        assert!(!config2.skills_path.contains("..claude"));
    }

    #[test]
    fn test_persistence() {
        let dir = TempDir::new().unwrap();

        // Create and link
        let mut registry = ProviderSkillsRegistry::new(dir.path()).unwrap();
        registry.link_skill("claude", "code-review").unwrap();
        registry.link_skill("cursor", "commit-msg").unwrap();
        drop(registry);

        // Reload and verify
        let registry2 = ProviderSkillsRegistry::new(dir.path()).unwrap();
        assert!(registry2.is_skill_linked("claude", "code-review"));
        assert!(registry2.is_skill_linked("cursor", "commit-msg"));
        assert!(!registry2.is_skill_linked("codex", "code-review"));
    }
}
