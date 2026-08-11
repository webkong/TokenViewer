use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use super::utils::*;
use crate::codex_home::{discover_codex_homes, CodexHome};
use crate::models::UsageRecord;

pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let homes = discover_codex_homes(home_dir, &[], &[], false);
    parse_with_homes(home_dir, cursor_data, &homes)
}

pub fn parse_with_homes(
    home_dir: &Path,
    cursor_data: Option<&str>,
    homes: &[CodexHome],
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let mut cursor = FileCursor::from_json(cursor_data);
    let mut all_records = Vec::new();
    let shared_aliases = unambiguous_model_aliases(homes);
    let mut session_bases: Vec<SessionBase> = homes
        .iter()
        .filter(|item| item.exists && item.has_sessions)
        .map(|item| {
            let home = item.path_buf();
            let mut resolver = ModelResolver::from_home(&home);
            resolver.add_fallbacks(&shared_aliases);
            SessionBase {
                path: home.join("sessions"),
                resolver,
            }
        })
        .collect();
    if session_bases.is_empty() {
        let home = home_dir.join(".codex");
        let mut resolver = ModelResolver::from_home(&home);
        resolver.add_fallbacks(&shared_aliases);
        session_bases.push(SessionBase {
            path: home.join("sessions"),
            resolver,
        });
    }
    scan_codex_bases(&session_bases, &mut cursor, &mut all_records, "codex");

    Ok((aggregate_records(all_records), cursor.to_json()))
}

pub fn parse_codex_format(
    home_dir: &Path,
    cursor_data: Option<&str>,
    rel_dir: &str,
    source: &str,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let base = home_dir.join(rel_dir);
    let mut cursor = FileCursor::from_json(cursor_data);
    let mut all_records = Vec::new();
    scan_codex_bases(
        &[SessionBase {
            path: base,
            resolver: ModelResolver::default(),
        }],
        &mut cursor,
        &mut all_records,
        source,
    );
    Ok((aggregate_records(all_records), cursor.to_json()))
}

/// Return alias mappings that agree across all discovered Codex homes. These
/// can safely relabel legacy aggregate rows that predate model resolution.
pub fn unambiguous_model_aliases(homes: &[CodexHome]) -> HashMap<String, String> {
    let mut targets: HashMap<String, HashSet<String>> = HashMap::new();
    for home in homes.iter().filter(|item| item.exists) {
        let resolver = ModelResolver::from_home(&home.path_buf());
        for reported in resolver.aliases.keys() {
            let resolved = resolver.resolve(reported);
            targets
                .entry(reported.clone())
                .or_default()
                .insert(resolved);
        }
    }
    targets
        .into_iter()
        .filter_map(|(reported, resolved)| {
            if resolved.len() == 1 {
                resolved.into_iter().next().map(|value| (reported, value))
            } else {
                None
            }
        })
        .collect()
}

#[derive(Clone, Default)]
struct ModelResolver {
    /// Lower-cased reported model -> actual upstream model.
    aliases: HashMap<String, String>,
    conflicts: HashSet<String>,
}

impl ModelResolver {
    fn from_home(home: &Path) -> Self {
        let mut resolver = Self::default();
        let Ok(entries) = fs::read_dir(home) else {
            return resolver;
        };
        let mut catalogs: Vec<PathBuf> = entries
            .flatten()
            .filter_map(|entry| {
                let file_type = entry.file_type().ok()?;
                if !file_type.is_file() || file_type.is_symlink() {
                    return None;
                }
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.ends_with("provider-model-catalog.json")
                    || name.ends_with("model-alias-catalog.json")
                    || name.ends_with("model-aliases.json")
                {
                    Some(entry.path())
                } else {
                    None
                }
            })
            .collect();
        catalogs.sort();
        for catalog in catalogs {
            resolver.load_catalog(&catalog);
        }
        resolver
    }

    fn load_catalog(&mut self, path: &Path) {
        let Ok(data) = fs::read(path) else {
            return;
        };
        let Ok(root) = serde_json::from_slice::<Value>(&data) else {
            return;
        };

        for key in ["aliases", "model_aliases"] {
            if let Some(aliases) = root.get(key).and_then(Value::as_object) {
                for (reported, value) in aliases {
                    let resolved = value
                        .as_str()
                        .or_else(|| resolved_model_from_entry(value, reported));
                    if let Some(resolved) = resolved {
                        self.insert(reported, resolved);
                    }
                }
            }
        }

        if let Some(models) = root.get("models").and_then(Value::as_array) {
            for entry in models {
                let reported = ["slug", "alias", "reported_model", "id"]
                    .into_iter()
                    .find_map(|key| entry.get(key).and_then(Value::as_str));
                if let Some(reported) = reported {
                    if let Some(resolved) = resolved_model_from_entry(entry, reported) {
                        self.insert(reported, resolved);
                    }
                }
            }
        }
    }

    fn insert(&mut self, reported: &str, resolved: &str) {
        let reported = reported.trim();
        let resolved = resolved.trim();
        if reported.is_empty()
            || reported.eq_ignore_ascii_case(resolved)
            || !is_model_identifier(resolved)
        {
            return;
        }
        let key = reported.to_ascii_lowercase();
        if self.conflicts.contains(&key) {
            return;
        }
        if self
            .aliases
            .get(&key)
            .is_some_and(|existing| !existing.eq_ignore_ascii_case(resolved))
        {
            self.aliases.remove(&key);
            self.conflicts.insert(key);
            return;
        }
        self.aliases.insert(key, resolved.to_string());
    }

    fn resolve(&self, reported: &str) -> String {
        let mut value = reported.trim().to_string();
        let mut visited = HashSet::new();
        for _ in 0..8 {
            let key = value.to_ascii_lowercase();
            if !visited.insert(key.clone()) {
                break;
            }
            let Some(next) = self.aliases.get(&key) else {
                break;
            };
            value = next.clone();
        }
        value
    }

    fn add_fallbacks(&mut self, aliases: &HashMap<String, String>) {
        for (reported, resolved) in aliases {
            if !self.aliases.contains_key(reported) && !self.conflicts.contains(reported) {
                self.aliases.insert(reported.clone(), resolved.clone());
            }
        }
    }
}

fn resolved_model_from_entry<'a>(entry: &'a Value, reported: &str) -> Option<&'a str> {
    [
        "upstream_model",
        "actual_model",
        "resolved_model",
        "model_id",
        "display_name",
        "description",
    ]
    .into_iter()
    .filter_map(|key| entry.get(key).and_then(Value::as_str))
    .find(|candidate| {
        let candidate = candidate.trim();
        !reported.eq_ignore_ascii_case(candidate) && is_model_identifier(candidate)
    })
}

fn is_model_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value.chars().any(|ch| ch.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '/' | '@'))
}

fn model_from_value(value: &Value) -> Option<&str> {
    [
        "upstream_model",
        "actual_model",
        "resolved_model",
        "model",
        "model_id",
        "model_name",
    ]
    .into_iter()
    .find_map(|key| value.get(key).and_then(Value::as_str))
    .map(str::trim)
    .filter(|value| !value.is_empty())
}

fn provider_from_value(value: &Value) -> Option<&str> {
    ["model_provider", "model_provider_id", "provider", "provider_id"]
        .into_iter()
        .find_map(|key| value.get(key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

#[derive(Clone)]
struct SessionBase {
    path: PathBuf,
    resolver: ModelResolver,
}

#[derive(Clone)]
struct RolloutCandidate {
    path: PathBuf,
    resolver: ModelResolver,
}

/// Scan all known Codex session roots. Rollout filenames carry a globally
/// unique session UUID, so copies and hard links from isolated host apps are
/// grouped before parsing. The largest/newest copy wins.
fn scan_codex_bases(
    bases: &[SessionBase],
    cursor: &mut FileCursor,
    all_records: &mut Vec<UsageRecord>,
    source: &str,
) {
    let mut rollouts: HashMap<OsString, RolloutCandidate> = HashMap::new();
    for base in bases {
        if !base.path.exists() {
            continue;
        }
        let pattern = format!("{}/**/rollout-*.jsonl", base.path.display());
        for file in cursor.glob_cached(&pattern, &base.path) {
            let Some(name) = file.file_name().map(OsString::from) else {
                continue;
            };
            match rollouts.get(&name) {
                Some(existing) if file_rank(&existing.path) >= file_rank(&file) => {}
                _ => {
                    rollouts.insert(
                        name,
                        RolloutCandidate {
                            path: file,
                            resolver: base.resolver.clone(),
                        },
                    );
                }
            }
        }
    }

    let mut files: Vec<(OsString, RolloutCandidate)> = rollouts.into_iter().collect();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    for (rollout_name, candidate) in files {
        let file = candidate.path;
        let physical_key = file.to_string_lossy().to_string();
        let logical_key = format!("rollout:{}", rollout_name.to_string_lossy());
        migrate_legacy_rollout_cursor(cursor, &logical_key, &rollout_name);
        if !cursor.file_changed(&physical_key) {
            continue;
        }
        let offset = cursor.offsets.get(&logical_key).copied().unwrap_or(0);
        let file_len = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
        // Codex rollouts are append-only. A shorter copy from another host is
        // stale; replaying it from zero would double-count historical usage.
        if offset > file_len {
            continue;
        }
        let start_offset = offset;
        let mut fork_replay_session_id =
            cursor.codex_fork_replay_pending.get(&logical_key).cloned();
        let mut last_model = cursor
            .last_models
            .get(&logical_key)
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| String::from("unknown"));
        let mut last_provider = cursor
            .last_providers
            .get(&logical_key)
            .cloned()
            .unwrap_or_default();
        let (lines, new_offset) = match read_lines_from_offset(&file, start_offset) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if lines.is_empty() {
            cursor.offsets.insert(logical_key.clone(), new_offset);
            if !last_model.is_empty() {
                cursor.last_models.insert(logical_key.clone(), last_model);
            }
            if !last_provider.is_empty() {
                cursor
                    .last_providers
                    .insert(logical_key.clone(), last_provider);
            }
            continue;
        }
        let bucket = file_mtime_bucket(&file);

        for line in &lines {
            let v: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let event_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let payload = v.get("payload").unwrap_or(&Value::Null);

            if event_type == "session_meta"
                && payload
                    .get("forked_from_id")
                    .and_then(Value::as_str)
                    .is_some()
            {
                fork_replay_session_id = payload
                    .get("session_id")
                    .or_else(|| payload.get("id"))
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if let Some(session_id) = &fork_replay_session_id {
                    cursor
                        .codex_fork_replay_pending
                        .insert(logical_key.clone(), session_id.clone());
                }
            }

            let skip_fork_replay = fork_replay_session_id.is_some();
            if fork_replay_session_id
                .as_deref()
                .is_some_and(|session_id| starts_child_fork_turn(event_type, payload, session_id))
            {
                fork_replay_session_id = None;
                cursor.codex_fork_replay_pending.remove(&logical_key);
            }

            if event_type == "turn_context" || event_type == "session_meta" {
                if let Some(provider) = provider_from_value(payload) {
                    last_provider = provider.to_string();
                }
                if let Some(model) = model_from_value(payload) {
                    last_model = model.to_string();
                }
            } else if payload.get("type").and_then(Value::as_str)
                == Some("thread_settings_applied")
            {
                if let Some(settings) = payload.get("thread_settings") {
                    if let Some(provider) = provider_from_value(settings) {
                        last_provider = provider.to_string();
                    }
                    if let Some(model) = model_from_value(settings) {
                        last_model = model.to_string();
                    }
                }
            }

            // Check for token_count in payload.type or payload.msg.type
            let is_token_count = payload.get("type").and_then(|t| t.as_str())
                == Some("token_count")
                || payload
                    .get("msg")
                    .and_then(|m| m.get("type"))
                    .and_then(|t| t.as_str())
                    == Some("token_count");

            if !is_token_count {
                continue;
            }

            let info = if payload.get("type").and_then(|t| t.as_str()) == Some("token_count") {
                payload.get("info").cloned().unwrap_or(Value::Null)
            } else {
                payload
                    .get("msg")
                    .and_then(|m| m.get("info"))
                    .cloned()
                    .unwrap_or(Value::Null)
            };

            // Prefer total_token_usage (cumulative) with delta, fallback to last_token_usage
            let (usage, use_delta) = if let Some(u) = info.get("total_token_usage") {
                (u, true)
            } else if let Some(u) = info.get("last_token_usage") {
                (u, false)
            } else {
                continue;
            };

            let raw_input = usage
                .get("input_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let output = usage
                .get("output_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let cached = usage
                .get("cached_input_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let cache_creation = usage
                .get("cache_creation_input_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let reasoning = usage
                .get("reasoning_output_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);

            let (fi, fo, fc_read, fc_write, fr) = if use_delta {
                // Delta on raw values first, then normalize input -= cached
                let cur = [raw_input, output, cached, cache_creation, reasoning];
                let d = cursor.delta(&logical_key, cur);
                (d[0].saturating_sub(d[2]), d[1], d[2], d[3], d[4])
            } else {
                (
                    raw_input.saturating_sub(cached),
                    output,
                    cached,
                    cache_creation,
                    reasoning,
                )
            };

            let total = fi + fo + fc_read + fc_write + fr;
            if total == 0 || skip_fork_replay {
                continue;
            }

            let hour_start = v
                .get("timestamp")
                .and_then(|t| t.as_str())
                .map(iso_to_bucket)
                .unwrap_or_else(|| bucket.clone());

            let reported_model = model_from_value(&info)
                .or_else(|| model_from_value(usage))
                .map(str::to_string)
                .unwrap_or_else(|| {
                    if !last_model.is_empty() && last_model != "unknown" {
                        last_model.clone()
                    } else if !last_provider.is_empty() {
                        last_provider.clone()
                    } else {
                        last_model.clone()
                    }
                });

            all_records.push(UsageRecord {
                id: None,
                hour_start,
                source: source.to_string(),
                model: candidate.resolver.resolve(&reported_model),
                input_tokens: fi,
                output_tokens: fo,
                cached_input_tokens: fc_read,
                cache_creation_input_tokens: fc_write,
                reasoning_output_tokens: fr,
                total_tokens: total,
                conversation_count: 1,
            });
        }

        cursor.offsets.insert(logical_key.clone(), new_offset);
        if !last_model.is_empty() {
            cursor.last_models.insert(logical_key.clone(), last_model);
        }
        if !last_provider.is_empty() {
            cursor
                .last_providers
                .insert(logical_key.clone(), last_provider);
        }
    }
}

/// Forked Codex rollouts begin with a rewritten copy of the parent history.
/// UUIDv7 strings sort chronologically, so the first task/turn whose ID is at
/// least the child session ID marks the start of genuinely new activity.
fn starts_child_fork_turn(event_type: &str, payload: &Value, session_id: &str) -> bool {
    if event_type != "turn_context"
        && !(event_type == "event_msg"
            && payload.get("type").and_then(Value::as_str) == Some("task_started"))
    {
        return false;
    }
    payload
        .get("turn_id")
        .and_then(Value::as_str)
        .is_some_and(|turn_id| turn_id >= session_id)
}

fn file_rank(path: &Path) -> (u64, u64) {
    let Ok(metadata) = std::fs::metadata(path) else {
        return (0, 0);
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_secs())
        .unwrap_or(0);
    (metadata.len(), modified)
}

fn migrate_legacy_rollout_cursor(
    cursor: &mut FileCursor,
    logical_key: &str,
    rollout_name: &OsString,
) {
    if cursor.offsets.contains_key(logical_key) {
        return;
    }
    let rollout_name = rollout_name.to_string_lossy();
    let legacy_key = cursor
        .offsets
        .iter()
        .filter(|(key, _)| {
            Path::new(key)
                .file_name()
                .is_some_and(|name| name == rollout_name.as_ref())
        })
        .max_by_key(|(_, offset)| **offset)
        .map(|(key, _)| key.clone());
    let Some(legacy_key) = legacy_key else {
        return;
    };
    if let Some(offset) = cursor.offsets.get(&legacy_key).copied() {
        cursor.offsets.insert(logical_key.to_string(), offset);
    }
    if let Some(snapshot) = cursor.snapshots.get(&legacy_key).copied() {
        cursor.snapshots.insert(logical_key.to_string(), snapshot);
    }
    if let Some(model) = cursor.last_models.get(&legacy_key).cloned() {
        cursor.last_models.insert(logical_key.to_string(), model);
    }
    if let Some(provider) = cursor.last_providers.get(&legacy_key).cloned() {
        cursor
            .last_providers
            .insert(logical_key.to_string(), provider);
    }
}
