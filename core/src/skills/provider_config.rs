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
    /// Brand color hex string, e.g. "#059669".
    #[serde(default)]
    pub brand_color: String,
    /// Logo filename without extension, e.g. "claude-code".
    #[serde(default)]
    pub logo_file: String,
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
            brand_color: "#059669".to_string(),
            logo_file: String::new(),
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

/// Detect which agents are installed. Two strategies are OR'ed:
/// 1. CLI check: if detect_cmd is set, run `which <cmd>` (fast, ms-level)
/// 2. Presence check: inspect known config/data directories for agents without
///    a CLI, or when the CLI binary is not on PATH.
pub fn detect_installed_agents(providers: &[ProviderSkillsConfig]) -> Vec<(String, bool)> {
    providers
        .par_iter()
        .map(|p| {
            let installed = p
                .detect_cmd
                .as_deref()
                .map(is_command_on_path)
                .unwrap_or(false)
                || is_agent_present_on_disk(p);
            (p.source.clone(), installed)
        })
        .collect()
}

/// Check whether provider-specific local data exists.
fn is_agent_present_on_disk(provider: &ProviderSkillsConfig) -> bool {
    agent_presence_paths(provider)
        .into_iter()
        .any(|path| path.exists())
}

/// Candidate files/directories that indicate an agent has been used/installed.
fn agent_presence_paths(provider: &ProviderSkillsConfig) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if let Ok(skills_path) = expand_path(&provider.skills_path) {
        if let Some(parent) = skills_path.parent() {
            paths.push(parent.to_path_buf());
        }
    }

    if let Some(home) = dirs::home_dir() {
        match provider.source.as_str() {
            "codebuddy" => {
                if let Ok(custom_home) = std::env::var("CODEBUDDY_HOME") {
                    paths.push(PathBuf::from(custom_home));
                }
                paths.push(home.join(".codebuddy"));
                paths.push(home.join(".antigravity_cockpit/codebuddy_accounts"));
                paths.push(home.join(".antigravity_cockpit/codebuddy_cn_accounts"));
                paths.push(home.join(".antigravity_cockpit/codebuddy_accounts.json"));
                paths.push(home.join(".antigravity_cockpit/codebuddy_cn_accounts.json"));
            }
            "workbuddy" => {
                if let Ok(custom_home) = std::env::var("WORKBUDDY_HOME") {
                    paths.push(PathBuf::from(custom_home));
                }
                paths.push(home.join(".antigravity_cockpit/workbuddy_accounts"));
                paths.push(home.join(".antigravity_cockpit/workbuddy_accounts.json"));
            }
            "zcode" => {
                paths.push(home.join(".zcode"));
                paths.push(home.join(".zcode/cli/db/db.sqlite"));
                paths.push(home.join(".zcode/v2/config.json"));
            }
            "craft" => {
                paths.push(home.join(".craft-agent"));
            }
            "zed" => {
                paths.push(home.join(".config/zed"));
                paths.push(home.join("Library/Application Support/Zed"));
            }
            "trae" => {
                paths.push(home.join(".trae"));
                paths.push(home.join(".antigravity_cockpit/trae_accounts"));
                paths.push(home.join("Library/Application Support/Trae"));
            }
            "windsurf" => {
                paths.push(home.join(".codeium/windsurf"));
                paths.push(home.join(".antigravity_cockpit/windsurf_accounts"));
                paths.push(home.join("Library/Application Support/Windsurf"));
            }
            "qoder" => {
                paths.push(home.join(".qoder"));
                paths.push(home.join(".antigravity_cockpit/qoder_accounts"));
                paths.push(home.join("Library/Application Support/Qoder"));
            }
            _ => {}
        }
    }

    paths.sort();
    paths.dedup();
    paths
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
const INSTALL_CACHE_VERSION: u32 = 2;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct InstallStatusCache {
    #[serde(default)]
    version: u32,
    updated_at: u64,
    statuses: HashMap<String, bool>,
}

/// Load cached install status from disk. Returns None if expired or missing.
fn load_install_cache(config_dir: &Path) -> Option<HashMap<String, bool>> {
    let cache_path = config_dir.join("install_status.json");
    let data = fs::read_to_string(&cache_path).ok()?;
    let cache: InstallStatusCache = serde_json::from_str(&data).ok()?;
    if cache.version != INSTALL_CACHE_VERSION {
        return None;
    }
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
        version: INSTALL_CACHE_VERSION,
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

    // Providers with subscription/quota tracking (limits panel).
    let limits_sources: std::collections::HashSet<&str> = std::collections::HashSet::from([
        "claude",
        "codex",
        "gemini",
        "kimi",
        "opencode",
        "copilot",
        "kiro",
        "cursor",
        "antigravity",
        "zed",
        "trae",
        "windsurf",
        "qoder",
        "codebuddy",
        "workbuddy",
        "zcode",
    ]);

    // All sources (parser + agent-only)
    let all_sources: Vec<(&str, &str, &str, Option<&str>, &str)> = {
        let mut sources: Vec<(&str, &str, &str, Option<&str>, &str)> = vec![
            // (source, display_name, logo_file, detect_cmd, brand_color)
            // ── Canonical providers with parsers ──
            (
                "claude",
                "Claude Code",
                "claude-code",
                Some("claude"),
                "#d97757",
            ),
            ("codex", "Codex", "codex", Some("codex"), "#3b82f6"),
            (
                "cursor",
                "Cursor",
                "cursor",
                Some("cursor-agent"),
                "#8c5cf5",
            ),
            ("kiro", "Kiro", "kiro", Some("kiro-cli"), "#059669"),
            (
                "copilot",
                "GitHub Copilot",
                "copilot",
                Some("copilot"),
                "#4078c0",
            ),
            ("kimi", "Kimi", "kimi", Some("kimi"), "#a38cfa"),
            (
                "antigravity",
                "Antigravity",
                "antigravity",
                Some("agy"),
                "#2196f3",
            ),
            ("zed", "Zed", "zed", None, "#c4841e"),
            ("gemini", "Gemini", "gemini", Some("gemini"), "#2196f3"),
            (
                "opencode",
                "OpenCode",
                "opencode",
                Some("opencode"),
                "#f59e0b",
            ),
            (
                "openclaw",
                "OpenClaw",
                "openclaw",
                Some("openclaw"),
                "#f59e0b",
            ),
            ("hermes", "Hermes", "hermes", Some("hermes"), "#ca8a04"),
            ("grok", "Grok", "grok", Some("grok"), "#73737f"),
            ("roocode", "RooCode", "roocode", None, "#ea580c"),
            ("kilocode", "KiloCode", "kilo", Some("kilo"), "#dc2626"),
            ("kilocli", "Kilo CLI", "kilo", None, "#dc2626"),
            ("goose", "Goose", "goose", Some("goose"), "#16a34a"),
            ("ohmypi", "OhMyPi", "ohmypi", None, "#db2777"),
            ("pi", "Pi", "pi", Some("pi"), "#9333ea"),
            ("craft", "Craft Agent", "craft", None, "#0284c7"),
            ("everycode", "EveryCode", "codex", None, "#3b82f6"),
            ("mimocode", "MimoCode", "mimo", Some("mimo"), "#2563eb"),
            ("zcode", "ZCode", "zcode", None, "#4f5cf5"),
            ("codebuddy", "CodeBuddy", "codebuddy", None, "#d97757"),
            ("workbuddy", "WorkBuddy", "workbuddy", None, "#1d4ed8"),
            // ── Agent-only (limits card, no parser) ──
            ("trae", "Trae", "trae", None, "#2563eb"),
            ("windsurf", "Windsurf", "windsurf", None, "#0d9488"),
            ("qoder", "Qoder", "qoder", None, "#7c3aed"),
            // ── Orca-sourced agents (skill-only, no parser, no limits) ──
            (
                "openclaude",
                "OpenClaude",
                "openclaude-logo",
                Some("openclaude"),
                "#e06b4d",
            ),
            ("devin", "Devin", "devin", Some("devin"), "#4f82f5"),
            ("ante", "Ante", "ante", Some("ante"), "#3b82f6"),
            (
                "autohand",
                "Autohand Code",
                "autohand",
                Some("autohand"),
                "#f43f5e",
            ),
            ("aider", "Aider", "aider", Some("aider"), "#45cc82"),
            ("amp", "Amp", "amp", Some("amp"), "#8b5cf6"),
            ("crush", "Charm", "crush", Some("crush"), "#ec4899"),
            ("aug", "Auggie", "aug", Some("auggie"), "#14b8a6"),
            ("cline", "Cline", "cline", Some("cline"), "#f5a624"),
            (
                "codebuff",
                "Codebuff",
                "codebuff",
                Some("codebuff"),
                "#22c55e",
            ),
            (
                "command-code",
                "Command Code",
                "codex",
                Some("command-code"),
                "#3b82f6",
            ),
            ("continue", "Continue", "continue", Some("cn"), "#6366f1"),
            ("droid", "Droid", "droid", Some("droid"), "#22c55e"),
            (
                "mistral-vibe",
                "Mistral Vibe",
                "mistral-vibe",
                Some("vibe"),
                "#3b82f6",
            ),
            (
                "qwen-code",
                "Qwen Code",
                "qwen",
                Some("qwen-code"),
                "#1e90ff",
            ),
            ("rovo", "Rovo Dev", "rovo", Some("rovo"), "#a855f7"),
            ("omp", "OMP", "omp", Some("omp"), "#e04d8c"),
        ];
        // Ensure parser sources that aren't in the list above are still included
        // via all_parser_sources(). This catches any future additions.
        for ps in crate::parsers::all_parser_sources() {
            if !sources.iter().any(|(s, _, _, _, _)| *s == ps) {
                sources.push((ps, ps, ps, None, "#059669"));
            }
        }
        sources
    };

    all_sources
        .into_iter()
        .map(
            |(source, display_name, logo_file, detect_cmd, brand_color)| {
                let has_parser = parser_sources.contains(source);
                let has_limits = limits_sources.contains(source);
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
                    display_name: display_name.to_string(),
                    skills_path,
                    link_type: LinkType::Directory,
                    is_linked: false,
                    linked_skills: Vec::new(),
                    has_parser,
                    has_limits,
                    detect_cmd: detect_cmd.map(|s| s.to_string()),
                    is_installed: false,
                    brand_color: brand_color.to_string(),
                    logo_file: logo_file.to_string(),
                }
            },
        )
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
