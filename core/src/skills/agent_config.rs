use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::skills::models::LinkType;

/// Canonical coding-agent configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentConfig {
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
    /// Alternative binary names that identify the same agent (Orca's
    /// `detectCmdAliases`), e.g. `mistral-vibe` → `vibe`.
    #[serde(default)]
    pub detect_cmd_aliases: Vec<String>,
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

impl AgentConfig {
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
            detect_cmd_aliases: Vec::new(),
            is_installed: false,
            brand_color: "#059669".to_string(),
            logo_file: String::new(),
        }
    }
}

/// Registry of all coding agents with skills configuration.
/// Builtin agents are derived from parser sources + additional skill-only agents.
pub struct AgentRegistry {
    builtin: Vec<AgentConfig>,
    /// Custom overrides for agent skills paths.
    overrides: HashMap<String, AgentOverrides>,
    /// Config directory for persistent linked_skills
    config_dir: PathBuf,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct AgentOverrides {
    skills_path: Option<String>,
    link_type: Option<LinkType>,
    linked_skills: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistedLinkedSkills {
    /// Map from source name to list of linked skill IDs.
    linked: HashMap<String, Vec<String>>,
}

impl AgentRegistry {
    pub fn new(config_dir: &Path) -> Result<Self, String> {
        fs::create_dir_all(config_dir)
            .map_err(|e| format!("Failed to create config dir: {}", e))?;

        let builtin = builtin_agents();
        let overrides = Self::load_overrides(config_dir).unwrap_or_default();

        Ok(Self {
            builtin,
            overrides,
            config_dir: config_dir.to_path_buf(),
        })
    }

    /// Returns all agents (builtin merged with overrides).
    pub fn all(&self) -> Vec<AgentConfig> {
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

    /// Find an agent config by source name. Uses canonical name.
    pub fn find(&self, source: &str) -> Option<AgentConfig> {
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

    /// Set a per-agent override (skills_path / link_type).
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
            .or_insert_with(|| AgentOverrides {
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

    /// Reset a per-agent override to defaults.
    pub fn reset_override(&mut self, source: &str) -> Result<(), String> {
        let canonical = canonical_source(source);
        self.overrides.remove(canonical);
        self.persist_overrides()
    }

    /// Link a skill to an agent.
    pub fn link_skill(&mut self, source: &str, skill_id: &str) -> Result<(), String> {
        let canonical = canonical_source(source);
        let mut linked = self.load_linked_skills();
        let entry = linked.entry(canonical.to_string()).or_default();
        if !entry.contains(&skill_id.to_string()) {
            entry.push(skill_id.to_string());
        }
        self.persist_linked_skills(&linked)
    }

    /// Unlink a skill from an agent.
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

    /// Remove a deleted skill from every persisted agent association.
    pub fn unlink_skill_from_all(&mut self, skill_id: &str) -> Result<(), String> {
        self.unlink_skills_from_all(&HashSet::from([skill_id.to_string()]))
    }

    /// Remove deleted skills from every persisted agent association in one read/write pass.
    pub fn unlink_skills_from_all(&mut self, skill_ids: &HashSet<String>) -> Result<(), String> {
        if skill_ids.is_empty() {
            return Ok(());
        }
        let mut linked = self.load_linked_skills();
        let linked_changed = linked
            .values()
            .any(|linked_ids| linked_ids.iter().any(|id| skill_ids.contains(id)));
        if linked_changed {
            linked.retain(|_, linked_ids| {
                linked_ids.retain(|id| !skill_ids.contains(id));
                !linked_ids.is_empty()
            });
            self.persist_linked_skills(&linked)?;
        }

        let mut overrides_changed = false;
        for overrides in self.overrides.values_mut() {
            let previous_len = overrides.linked_skills.len();
            overrides.linked_skills.retain(|id| !skill_ids.contains(id));
            overrides_changed |= overrides.linked_skills.len() != previous_len;
        }
        if overrides_changed {
            self.persist_overrides()?;
        }
        Ok(())
    }

    /// Check if a skill is linked to an agent.
    pub fn is_skill_linked(&self, source: &str, skill_id: &str) -> bool {
        let canonical = canonical_source(source);
        self.load_linked_skills()
            .get(canonical)
            .map(|s| s.contains(&skill_id.to_string()))
            .unwrap_or(false)
    }

    // ── Persistence ──

    fn overrides_path(&self) -> PathBuf {
        self.config_dir.join("agent_overrides.json")
    }

    fn linked_skills_path(&self) -> PathBuf {
        self.config_dir.join("linked_skills.json")
    }

    fn load_overrides(config_dir: &Path) -> Result<HashMap<String, AgentOverrides>, String> {
        let current_path = config_dir.join("agent_overrides.json");
        // Compatibility with installations that already used the private config
        // directory while retaining the filename from Provider terminology.
        let legacy_path = config_dir.join("provider_overrides.json");
        let path = if current_path.exists() {
            current_path
        } else {
            legacy_path
        };
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
/// 2. Presence check: inspect product-specific runtime/config artifacts for
///    agents without a CLI, or when the CLI binary is not on PATH.
///
/// A generic `~/.<agent>/skills` directory is deliberately not evidence of an
/// installation: TokenViewer may create it while linking skills, and it often
/// survives after an agent is removed.
pub fn detect_installed_agents(agents: &[AgentConfig]) -> Vec<(String, bool)> {
    agents
        .par_iter()
        .map(|p| {
            let installed = is_agent_cli_on_path(p) || is_agent_present_on_disk(p);
            (p.source.clone(), installed)
        })
        .collect()
}

/// CLI strategy: the primary `detect_cmd` or any of its aliases on PATH
/// (Orca's `getTuiAgentDetectCommands`).
fn is_agent_cli_on_path(agent: &AgentConfig) -> bool {
    agent
        .detect_cmd
        .as_deref()
        .map(is_command_on_path)
        .unwrap_or(false)
        || agent
            .detect_cmd_aliases
            .iter()
            .any(|alias| is_command_on_path(alias))
}

/// Check whether agent-specific local data exists.
fn is_agent_present_on_disk(agent: &AgentConfig) -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    is_agent_present_in_home(agent, &home)
}

fn is_agent_present_in_home(agent: &AgentConfig, home: &Path) -> bool {
    agent_presence_paths(agent, home)
        .into_iter()
        .any(|path| path.exists())
}

/// Candidate files/directories that indicate an agent has been used/installed.
/// These must be product-owned artifacts, not the configured skills destination
/// (TokenViewer may create the skills directory itself while linking skills, so
/// a bare `~/.<agent>` home is not evidence of an installation).
fn agent_presence_paths(agent: &AgentConfig, home: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    match agent.source.as_str() {
        "codebuddy" => {
            if let Ok(custom_home) = std::env::var("CODEBUDDY_HOME") {
                paths.push(PathBuf::from(custom_home));
            }
            paths.push(home.join(".codebuddy/projects"));
            paths.push(home.join(".antigravity_cockpit/codebuddy_accounts"));
            paths.push(home.join(".antigravity_cockpit/codebuddy_cn_accounts"));
            paths.push(home.join(".antigravity_cockpit/codebuddy_accounts.json"));
            paths.push(home.join(".antigravity_cockpit/codebuddy_cn_accounts.json"));
        }
        "workbuddy" => {
            if let Ok(custom_home) = std::env::var("WORKBUDDY_HOME") {
                paths.push(PathBuf::from(custom_home));
            }
            paths.push(home.join(".workbuddy/projects"));
            paths.push(home.join(".antigravity_cockpit/workbuddy_accounts"));
            paths.push(home.join(".antigravity_cockpit/workbuddy_accounts.json"));
        }
        "zcode" => {
            paths.push(home.join(".zcode/cli/db/db.sqlite"));
            paths.push(home.join(".zcode/v2/config.json"));
        }
        "dsh" => {
            paths.push(home.join(".dsh/sessions"));
            paths.push(home.join(".dsh/settings.yaml"));
        }
        "craft" => {
            paths.push(home.join(".craft-agent"));
        }
        "zed" => {
            paths.push(home.join(".config/zed"));
            paths.push(home.join("Library/Application Support/Zed"));
        }
        "trae" => {
            paths.push(home.join(".antigravity_cockpit/trae_accounts"));
            paths.push(home.join("Library/Application Support/Trae"));
        }
        "windsurf" => {
            paths.push(home.join(".codeium/windsurf"));
            paths.push(home.join(".antigravity_cockpit/windsurf_accounts"));
            paths.push(home.join("Library/Application Support/Windsurf"));
        }
        "qoder" => {
            paths.push(home.join(".antigravity_cockpit/qoder_accounts"));
            paths.push(home.join("Library/Application Support/Qoder"));
        }
        // OhMyPi (ohmypi) and OMP write the same session tree; Pi uses its own.
        "omp" | "ohmypi" => paths.push(home.join(".omp/agent/sessions")),
        "pi" => paths.push(home.join(".pi/agent/sessions")),
        // RooCode is a VS Code extension: its globalStorage task history is the
        // product-owned artifact (the same tree the usage parser reads).
        "roocode" => paths.push(
            crate::parsers::utils::vscode_global_storage(home).join("rooveterinaryinc.roo-cline"),
        ),
        // Kilo CLI's local SQLite history (the parser's data source).
        "kilocli" => paths.push(home.join(".local/share/kilo/kilo.db")),
        // EveryCode sessions mirror Codex rollout format under ~/.code/sessions.
        "everycode" => paths.push(home.join(".code/sessions")),
        _ => {}
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
const INSTALL_CACHE_VERSION: u32 = 4;

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
    agents: &[AgentConfig],
    force: bool,
) -> Vec<(String, bool)> {
    if !force {
        if let Some(cached) = load_install_cache(config_dir) {
            // Merge cached status with current agent list.
            return agents
                .iter()
                .map(|p| {
                    let installed = cached.get(&p.source).copied().unwrap_or(false);
                    (p.source.clone(), installed)
                })
                .collect();
        }
    }

    let results = detect_installed_agents(agents);
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

/// Built-in agents derived from parser sources + additional skill-only agents.
fn builtin_agents() -> Vec<AgentConfig> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let home_str = home.to_string_lossy().to_string();

    // Source names with has_parser (the parser sources list)
    let parser_sources: std::collections::HashSet<&str> = std::collections::HashSet::from([
        "claude",
        "codex",
        "cursor",
        "dsh",
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

    // Agents with subscription/quota tracking (limits panel).
    let limits_sources: std::collections::HashSet<&str> = std::collections::HashSet::from([
        "claude",
        "codex",
        "gemini",
        "kimi",
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
            // ── Canonical agents with parsers ──
            (
                "claude",
                "Claude Code",
                "claude-code",
                Some("claude"),
                "#d97757",
            ),
            ("codex", "ChatGPT", "chatgpt", Some("codex"), "#3b82f6"),
            (
                "cursor",
                "Cursor",
                "cursor",
                Some("cursor-agent"),
                "#8c5cf5",
            ),
            ("dsh", "DeepSeek Harness", "dsh", Some("dsh"), "#4d6bfe"),
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
            ("everycode", "EveryCode", "chatgpt", None, "#3b82f6"),
            ("mimocode", "MimoCode", "mimo", Some("mimo"), "#2563eb"),
            ("zcode", "ZCode", "zcode", None, "#4f5cf5"),
            ("codebuddy", "CodeBuddy", "codebuddy", None, "#d97757"),
            ("workbuddy", "WorkBuddy", "workbuddy", None, "#1d4ed8"),
            // ── Agent-only (limits card, no parser) ──
            ("trae", "Trae", "trae", Some("traecli"), "#2563eb"),
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
                // Why: the package is qwen-code but the installed CLI binary is
                // `qwen` (Orca tui-agent-config.ts).
                Some("qwen"),
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

                // Orca's detectCmdAliases: alternative binary names for the
                // same agent. Mistral Vibe installs `vibe`; `mistral-vibe`
                // remains an alias for wrapped installs.
                let detect_cmd_aliases: Vec<String> = match source {
                    "mistral-vibe" => vec!["mistral-vibe".to_string()],
                    _ => Vec::new(),
                };

                AgentConfig {
                    source: source.to_string(),
                    display_name: display_name.to_string(),
                    skills_path,
                    link_type: LinkType::Directory,
                    is_linked: false,
                    linked_skills: Vec::new(),
                    has_parser,
                    has_limits,
                    detect_cmd: detect_cmd.map(|s| s.to_string()),
                    detect_cmd_aliases,
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
    fn test_builtin_agents() {
        let dir = TempDir::new().unwrap();
        let registry = AgentRegistry::new(dir.path()).unwrap();
        let agents = registry.all();
        // Should have all parser sources + agent-only
        assert!(agents.len() >= 42);
        assert!(agents.iter().any(|a| a.source == "claude"));
        assert!(agents.iter().any(|a| a.source == "cursor"));
        // Claude should have display name "Claude Code"
        let claude = agents.iter().find(|a| a.source == "claude").unwrap();
        assert_eq!(claude.display_name, "Claude Code");
        assert!(claude.has_parser);
        // Trae should exist but have has_parser = false
        let trae = agents.iter().find(|a| a.source == "trae").unwrap();
        assert!(!trae.has_parser);
    }

    #[test]
    fn test_dsh_agent() {
        let dir = TempDir::new().unwrap();
        let registry = AgentRegistry::new(dir.path()).unwrap();
        let agents = registry.all();
        let dsh = agents.iter().find(|a| a.source == "dsh").unwrap();
        assert_eq!(dsh.display_name, "DeepSeek Harness");
        assert_eq!(dsh.logo_file, "dsh");
        assert_eq!(dsh.brand_color, "#4d6bfe");
        assert_eq!(dsh.detect_cmd.as_deref(), Some("dsh"));
        assert!(dsh.has_parser);
        assert!(!dsh.has_limits);
        assert!(dsh.skills_path.ends_with("/.dsh/skills"));
    }

    #[test]
    fn test_detect_cmds_align_with_orca() {
        let dir = TempDir::new().unwrap();
        let registry = AgentRegistry::new(dir.path()).unwrap();
        let agents = registry.all();
        let find = |s: &str| {
            agents
                .iter()
                .find(|a| a.source == s)
                .unwrap_or_else(|| panic!("missing agent {s}"))
        };

        // qwen-code installs the `qwen` binary (package name ≠ binary name).
        assert_eq!(find("qwen-code").detect_cmd.as_deref(), Some("qwen"));
        // trae's CLI is `traecli` (not bytedance/trae-agent's `trae-cli`).
        assert_eq!(find("trae").detect_cmd.as_deref(), Some("traecli"));
        // mistral-vibe's binary is `vibe`, with the package name as alias.
        assert_eq!(find("mistral-vibe").detect_cmd.as_deref(), Some("vibe"));
        assert_eq!(
            find("mistral-vibe").detect_cmd_aliases,
            vec!["mistral-vibe".to_string()]
        );
        // KiloCode's CLI is `kilo`.
        assert_eq!(find("kilocode").detect_cmd.as_deref(), Some("kilo"));
        // Kiro's installer ships `kiro-cli`, not `kiro`.
        assert_eq!(find("kiro").detect_cmd.as_deref(), Some("kiro-cli"));
        // Augment's binary is `auggie`.
        assert_eq!(find("aug").detect_cmd.as_deref(), Some("auggie"));
        // Continue's CLI is `cn`.
        assert_eq!(find("continue").detect_cmd.as_deref(), Some("cn"));
        // Cursor's agent CLI is `cursor-agent`.
        assert_eq!(find("cursor").detect_cmd.as_deref(), Some("cursor-agent"));
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
    fn test_skills_and_extensions_do_not_count_as_agent_installation() {
        let home = TempDir::new().unwrap();
        let omp = AgentConfig::custom("omp", "OMP", "~/.omp/skills", LinkType::Directory);
        let pi = AgentConfig::custom("pi", "Pi", "~/.pi/skills", LinkType::Directory);

        // These directories can be created by skill linking or extension setup
        // without the corresponding CLI ever having been installed.
        fs::create_dir_all(home.path().join(".omp/agent/extensions")).unwrap();
        fs::create_dir_all(home.path().join(".omp/skills")).unwrap();
        fs::create_dir_all(home.path().join(".pi/agent/extensions")).unwrap();
        fs::create_dir_all(home.path().join(".pi/agent/skills")).unwrap();
        assert!(!is_agent_present_in_home(&omp, home.path()));
        assert!(!is_agent_present_in_home(&pi, home.path()));

        fs::create_dir_all(home.path().join(".omp/agent/sessions")).unwrap();
        fs::create_dir_all(home.path().join(".pi/agent/sessions")).unwrap();
        assert!(is_agent_present_in_home(&omp, home.path()));
        assert!(is_agent_present_in_home(&pi, home.path()));
    }

    #[test]
    fn test_old_install_status_cache_is_invalidated() {
        let dir = TempDir::new().unwrap();
        let old_cache = serde_json::json!({
            "version": INSTALL_CACHE_VERSION - 1,
            "updated_at": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            "statuses": { "omp": true, "pi": true }
        });
        fs::write(
            dir.path().join("install_status.json"),
            serde_json::to_string(&old_cache).unwrap(),
        )
        .unwrap();

        assert!(load_install_cache(dir.path()).is_none());
    }

    #[test]
    fn test_parser_data_paths_are_installation_evidence() {
        let home = TempDir::new().unwrap();
        let h = home.path();

        // roocode: the VS Code extension globalStorage tree its parser reads.
        let roocode = AgentConfig::custom(
            "roocode",
            "RooCode",
            "~/.roocode/skills",
            LinkType::Directory,
        );
        fs::create_dir_all(h.join(".roocode")).unwrap();
        assert!(!is_agent_present_in_home(&roocode, h));
        let roo_dir =
            crate::parsers::utils::vscode_global_storage(h).join("rooveterinaryinc.roo-cline");
        fs::create_dir_all(&roo_dir).unwrap();
        assert!(is_agent_present_in_home(&roocode, h));

        // ohmypi: shares OMP's session tree; the bare ~/.ohmypi is not evidence.
        let ohmypi =
            AgentConfig::custom("ohmypi", "OhMyPi", "~/.ohmypi/skills", LinkType::Directory);
        fs::create_dir_all(h.join(".ohmypi")).unwrap();
        assert!(!is_agent_present_in_home(&ohmypi, h));
        fs::create_dir_all(h.join(".omp/agent/sessions")).unwrap();
        assert!(is_agent_present_in_home(&ohmypi, h));

        // kilocli: its local SQLite history.
        let kilocli = AgentConfig::custom(
            "kilocli",
            "Kilo CLI",
            "~/.kilocli/skills",
            LinkType::Directory,
        );
        fs::create_dir_all(h.join(".kilocli")).unwrap();
        assert!(!is_agent_present_in_home(&kilocli, h));
        fs::create_dir_all(h.join(".local/share/kilo")).unwrap();
        fs::write(h.join(".local/share/kilo/kilo.db"), b"").unwrap();
        assert!(is_agent_present_in_home(&kilocli, h));

        // everycode: rollout-format sessions under ~/.code/sessions.
        let everycode = AgentConfig::custom(
            "everycode",
            "EveryCode",
            "~/.everycode/skills",
            LinkType::Directory,
        );
        fs::create_dir_all(h.join(".everycode")).unwrap();
        assert!(!is_agent_present_in_home(&everycode, h));
        fs::create_dir_all(h.join(".code/sessions")).unwrap();
        assert!(is_agent_present_in_home(&everycode, h));
    }

    #[test]
    fn test_bare_home_dirs_are_not_installation_evidence() {
        let home = TempDir::new().unwrap();
        let h = home.path();

        // Bare ~/.<source> (the skills parent) alone must never count — skill
        // linking can create it — only product-owned artifacts below it do.
        for source in ["codebuddy", "workbuddy", "zcode", "trae", "qoder", "dsh"] {
            let agent = AgentConfig::custom(
                source,
                source,
                &format!("~/.{source}/skills"),
                LinkType::Directory,
            );
            fs::create_dir_all(h.join(format!(".{source}"))).unwrap();
            assert!(
                !is_agent_present_in_home(&agent, h),
                "bare ~/.{source} must not be installation evidence"
            );
        }

        // The product-owned children of the same trees do count.
        let codebuddy = AgentConfig::custom(
            "codebuddy",
            "CodeBuddy",
            "~/.codebuddy/skills",
            LinkType::Directory,
        );
        fs::create_dir_all(h.join(".codebuddy/projects")).unwrap();
        assert!(is_agent_present_in_home(&codebuddy, h));

        let dsh = AgentConfig::custom("dsh", "dsh", "~/.dsh/skills", LinkType::Directory);
        fs::create_dir_all(h.join(".dsh/sessions")).unwrap();
        assert!(is_agent_present_in_home(&dsh, h));
    }

    #[test]
    fn test_link_and_unlink_skill() {
        let dir = TempDir::new().unwrap();
        let mut registry = AgentRegistry::new(dir.path()).unwrap();

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

        let mut registry = AgentRegistry::new(dir.path()).unwrap();

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
        let mut registry = AgentRegistry::new(dir.path()).unwrap();
        registry.link_skill("claude", "code-review").unwrap();
        registry.link_skill("cursor", "commit-msg").unwrap();
        drop(registry);

        // Reload and verify
        let registry2 = AgentRegistry::new(dir.path()).unwrap();
        assert!(registry2.is_skill_linked("claude", "code-review"));
        assert!(registry2.is_skill_linked("cursor", "commit-msg"));
        assert!(!registry2.is_skill_linked("codex", "code-review"));
    }
}
