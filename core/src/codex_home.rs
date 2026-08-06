use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::storage::Database;

const ADDITIONAL_HOMES_KEY: &str = "codex.additional_homes";
const DISCOVERED_HOMES_KEY: &str = "codex.discovered_homes";
const DISCOVERY_TIME_KEY: &str = "codex.discovery_last_run";
const DISCOVERY_INTERVAL_SECONDS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexHomeSource {
    UserConfigured,
    Environment,
    Default,
    KnownHost,
    Discovered,
    Cached,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexHome {
    pub path: String,
    pub source: CodexHomeSource,
    pub exists: bool,
    pub has_sessions: bool,
    pub has_auth: bool,
    pub has_config: bool,
    pub is_user_configured: bool,
}

impl CodexHome {
    pub fn path_buf(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }
}

pub fn discover_with_database(db: &Database, home: &Path, force: bool) -> Vec<CodexHome> {
    let additional = setting_paths(db, ADDITIONAL_HOMES_KEY);
    let cached = setting_paths(db, DISCOVERED_HOMES_KEY);
    let last_scan = db
        .get_setting(DISCOVERY_TIME_KEY)
        .ok()
        .flatten()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let now = unix_timestamp();
    let include_scan = force || now.saturating_sub(last_scan) >= DISCOVERY_INTERVAL_SECONDS;
    let homes = discover_codex_homes(home, &additional, &cached, include_scan);

    if include_scan {
        let discovered: Vec<String> = homes
            .iter()
            .filter(|item| item.exists && !item.is_user_configured)
            .map(|item| item.path.clone())
            .collect();
        if let Ok(json) = serde_json::to_string(&discovered) {
            let _ = db.set_setting(DISCOVERED_HOMES_KEY, &json);
            let _ = db.set_setting(DISCOVERY_TIME_KEY, &now.to_string());
        }
    }
    homes
}

pub fn set_additional_homes(
    db: &Database,
    home: &Path,
    paths: &[String],
) -> Result<Vec<CodexHome>, String> {
    let mut normalized = Vec::new();
    for raw in paths {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = expand_home(trimmed, home);
        let value = normalize_path(&path);
        if !normalized.contains(&value) {
            normalized.push(value);
        }
    }
    let json = serde_json::to_string(&normalized).map_err(|error| error.to_string())?;
    db.set_setting(ADDITIONAL_HOMES_KEY, &json)
        .map_err(|error| error.to_string())?;
    Ok(discover_with_database(db, home, true))
}

pub fn discover_codex_homes(
    home: &Path,
    additional: &[PathBuf],
    cached: &[PathBuf],
    include_scan: bool,
) -> Vec<CodexHome> {
    let mut candidates: HashMap<String, Candidate> = HashMap::new();

    for path in additional {
        add_candidate(
            &mut candidates,
            expand_path_buf(path, home),
            CodexHomeSource::UserConfigured,
            true,
            true,
        );
    }

    if dirs::home_dir().as_deref() == Some(home) {
        if let Ok(value) = std::env::var("CODEX_HOME") {
            let value = value.trim();
            if !value.is_empty() {
                add_candidate(
                    &mut candidates,
                    expand_home(value, home),
                    CodexHomeSource::Environment,
                    false,
                    true,
                );
            }
        }
    }

    add_candidate(
        &mut candidates,
        home.join(".codex"),
        CodexHomeSource::Default,
        false,
        true,
    );
    for path in [
        home.join("Library/Application Support/orca/codex-runtime-home/home"),
        home.join(".antigravity_cockpit/instances/codex"),
    ] {
        add_candidate(
            &mut candidates,
            path,
            CodexHomeSource::KnownHost,
            false,
            false,
        );
    }

    // Host apps can create a new isolated CODEX_HOME at any time. Keep this
    // known, narrow root out of the 24-hour broad-scan cache so the next sync
    // sees newly-created Antigravity instances immediately.
    scan_for_codex_homes(
        &home.join(".antigravity_cockpit/instances/codex"),
        2,
        &mut candidates,
    );

    for path in cached {
        add_candidate(
            &mut candidates,
            expand_path_buf(path, home),
            CodexHomeSource::Cached,
            false,
            false,
        );
    }

    if include_scan {
        let scan_roots = [
            (home.join(".antigravity_cockpit"), 6usize),
            (home.join("Library/Application Support"), 6usize),
            (home.join("Library/Containers"), 8usize),
        ];
        for (root, depth) in scan_roots {
            scan_for_codex_homes(&root, depth, &mut candidates);
        }
        scan_home_codex_named_directories(home, &mut candidates);
    }

    let mut homes: Vec<CodexHome> = candidates
        .into_values()
        .filter_map(candidate_to_home)
        .collect();
    homes.sort_by(|left, right| {
        source_priority(&left.source)
            .cmp(&source_priority(&right.source))
            .then_with(|| left.path.cmp(&right.path))
    });
    homes
}

fn scan_home_codex_named_directories(home: &Path, candidates: &mut HashMap<String, Candidate>) {
    let Ok(entries) = fs::read_dir(home) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) && name.contains("codex") {
            if is_valid_discovered_home(&path) {
                add_candidate(candidates, path, CodexHomeSource::Discovered, false, false);
            }
        }
    }
}

fn scan_for_codex_homes(
    root: &Path,
    max_depth: usize,
    candidates: &mut HashMap<String, Candidate>,
) {
    fn walk(
        path: &Path,
        depth: usize,
        max_depth: usize,
        candidates: &mut HashMap<String, Candidate>,
    ) {
        if depth > max_depth || should_prune(path) {
            return;
        }
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() || file_type.is_symlink() {
                continue;
            }
            let child = entry.path();
            if entry.file_name() == "sessions" {
                if let Some(parent) = child.parent() {
                    if is_valid_discovered_home(parent) {
                        add_candidate(
                            candidates,
                            parent.to_path_buf(),
                            CodexHomeSource::Discovered,
                            false,
                            false,
                        );
                    }
                }
                continue;
            }
            walk(&child, depth + 1, max_depth, candidates);
        }
    }
    walk(root, 0, max_depth, candidates);
}

fn should_prune(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "caches" | "cache" | "node_modules" | ".git" | "deriveddata" | "backups" | "backup"
    )
}

fn is_valid_discovered_home(path: &Path) -> bool {
    path.join("auth.json").is_file()
        || path.join("config.toml").is_file()
        || path.join("archived_sessions").is_dir()
        || contains_rollout(&path.join("sessions"), 5)
}

fn contains_rollout(path: &Path, depth: usize) -> bool {
    if depth == 0 || !path.is_dir() {
        return false;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_file() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("rollout-") && name.ends_with(".jsonl") {
                return true;
            }
        } else if file_type.is_dir()
            && !file_type.is_symlink()
            && contains_rollout(&path, depth - 1)
        {
            return true;
        }
    }
    false
}

#[derive(Debug)]
struct Candidate {
    path: PathBuf,
    source: CodexHomeSource,
    user_configured: bool,
    keep_if_missing: bool,
}

fn add_candidate(
    candidates: &mut HashMap<String, Candidate>,
    path: PathBuf,
    source: CodexHomeSource,
    user_configured: bool,
    keep_if_missing: bool,
) {
    let normalized = normalize_path(&path);
    match candidates.get_mut(&normalized) {
        Some(existing) => {
            if source_priority(&source) < source_priority(&existing.source) {
                existing.source = source;
            }
            existing.user_configured |= user_configured;
            existing.keep_if_missing |= keep_if_missing;
        }
        None => {
            candidates.insert(
                normalized,
                Candidate {
                    path,
                    source,
                    user_configured,
                    keep_if_missing,
                },
            );
        }
    }
}

fn candidate_to_home(candidate: Candidate) -> Option<CodexHome> {
    let exists = candidate.path.is_dir();
    if !exists && !candidate.keep_if_missing {
        return None;
    }
    if exists && !candidate.keep_if_missing && !is_valid_discovered_home(&candidate.path) {
        return None;
    }
    Some(CodexHome {
        path: normalize_path(&candidate.path),
        source: candidate.source,
        exists,
        has_sessions: candidate.path.join("sessions").is_dir(),
        has_auth: candidate.path.join("auth.json").is_file(),
        has_config: candidate.path.join("config.toml").is_file(),
        is_user_configured: candidate.user_configured,
    })
}

fn source_priority(source: &CodexHomeSource) -> u8 {
    match source {
        CodexHomeSource::UserConfigured => 0,
        CodexHomeSource::Environment => 1,
        CodexHomeSource::Default => 2,
        CodexHomeSource::KnownHost => 3,
        CodexHomeSource::Discovered => 4,
        CodexHomeSource::Cached => 5,
    }
}

fn setting_paths(db: &Database, key: &str) -> Vec<PathBuf> {
    db.get_setting(key)
        .ok()
        .flatten()
        .and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
        .unwrap_or_default()
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

fn expand_path_buf(path: &Path, home: &Path) -> PathBuf {
    expand_home(path.to_string_lossy().as_ref(), home)
}

fn expand_home(value: &str, home: &Path) -> PathBuf {
    if value == "~" {
        home.to_path_buf()
    } else if let Some(rest) = value.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(value)
    }
}

fn normalize_path(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string()
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn discovers_known_and_nested_codex_homes_without_browser_false_positive() {
        let dir = TempDir::new().unwrap();
        let home = dir.path();
        fs::create_dir_all(home.join(".codex/browser/sessions")).unwrap();
        fs::create_dir_all(home.join(".codex/sessions/2026/08/05")).unwrap();
        fs::write(home.join(".codex/config.toml"), "model = 'test'").unwrap();
        fs::create_dir_all(home.join(".antigravity_cockpit/instances/codex/sessions")).unwrap();
        fs::write(
            home.join(".antigravity_cockpit/instances/codex/auth.json"),
            "{}",
        )
        .unwrap();

        let homes = discover_codex_homes(home, &[], &[], true);
        assert!(homes.iter().any(|item| item.path.ends_with("/.codex")));
        assert!(homes
            .iter()
            .any(|item| item.path.ends_with("/.antigravity_cockpit/instances/codex")));
        assert!(!homes
            .iter()
            .any(|item| item.path.ends_with("/.codex/browser")));
    }

    #[test]
    fn preserves_missing_user_configured_home() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("future-codex-home");
        let homes = discover_codex_homes(dir.path(), &[missing.clone()], &[], false);
        assert!(homes.iter().any(|item| {
            item.path == missing.to_string_lossy() && item.is_user_configured && !item.exists
        }));
    }

    #[test]
    fn discovers_new_antigravity_instance_without_broad_scan() {
        let dir = TempDir::new().unwrap();
        let instance = dir
            .path()
            .join(".antigravity_cockpit/instances/codex/new-instance");
        fs::create_dir_all(instance.join("sessions/2026/08/06")).unwrap();
        fs::write(instance.join("config.toml"), "model = 'test'").unwrap();

        let homes = discover_codex_homes(dir.path(), &[], &[], false);
        let expected_path = normalize_path(&instance);

        assert!(homes.iter().any(|item| {
            item.path == expected_path
                && item.exists
                && item.has_sessions
                && item.source == CodexHomeSource::Discovered
        }));
    }
}
