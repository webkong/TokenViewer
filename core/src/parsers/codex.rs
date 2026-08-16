use serde_json::Value;
use std::collections::HashMap;
use std::ffi::OsString;
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
    // Codex writes its own nested `sessions/YYYY/MM/DD/` tree, while Codex-Manager
    // archives sessions FLAT into `archived_sessions/`. The rollout glob matches
    // at any depth, so scanning both bases covers both layouts.
    let mut session_bases: Vec<PathBuf> = Vec::new();
    for item in homes.iter().filter(|item| item.exists) {
        session_bases.push(item.path_buf().join("sessions"));
        session_bases.push(item.path_buf().join("archived_sessions"));
    }
    if session_bases.is_empty() {
        session_bases.push(home_dir.join(".codex/sessions"));
        session_bases.push(home_dir.join(".codex/archived_sessions"));
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
    scan_codex_bases(&[base], &mut cursor, &mut all_records, source);
    Ok((aggregate_records(all_records), cursor.to_json()))
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
    [
        "model_provider",
        "model_provider_id",
        "provider",
        "provider_id",
    ]
    .into_iter()
    .find_map(|key| value.get(key).and_then(Value::as_str))
    .map(str::trim)
    .filter(|value| !value.is_empty())
}

/// Scan all known Codex session roots. Rollout filenames carry a globally
/// unique session UUID, so copies and hard links from isolated host apps are
/// grouped before parsing. The largest/newest copy wins.
fn scan_codex_bases(
    bases: &[PathBuf],
    cursor: &mut FileCursor,
    all_records: &mut Vec<UsageRecord>,
    source: &str,
) {
    let mut rollouts: HashMap<OsString, PathBuf> = HashMap::new();
    for base in bases {
        if !base.exists() {
            continue;
        }
        let pattern = format!("{}/**/rollout-*.jsonl", base.display());
        for file in cursor.glob_cached(&pattern, base) {
            let Some(name) = file.file_name().map(OsString::from) else {
                continue;
            };
            match rollouts.get(&name) {
                Some(existing) if file_rank(existing) >= file_rank(&file) => {}
                _ => {
                    rollouts.insert(name, file);
                }
            }
        }
    }

    let mut files: Vec<(OsString, PathBuf)> = rollouts.into_iter().collect();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    for (rollout_name, file) in files {
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
        let mut last_project_key = cursor
            .last_project_keys
            .get(&logical_key)
            .cloned()
            .unwrap_or_default();
        let mut last_project_ref = cursor
            .last_project_refs
            .get(&logical_key)
            .cloned()
            .unwrap_or_default();
        let reader = match OffsetLineReader::new(&file, start_offset) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let mut new_offset = start_offset;
        let bucket = file_mtime_bucket(&file);

        for (line, line_offset) in reader {
            new_offset = line_offset;

            // Cheap substring pre-filter before any JSON parse. The loop body
            // only reacts to session_meta / turn_context / thread_settings /
            // task_started / token_count events; any line that can match one of
            // those branches necessarily contains its literal type name.
            if !line.contains("token_count")
                && !line.contains("turn_context")
                && !line.contains("session_meta")
                && !line.contains("thread_settings")
                && !line.contains("task_started")
            {
                continue;
            }

            let v: Value = match serde_json::from_str(&line) {
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
                let cwd = payload.get("cwd").and_then(Value::as_str);
                let git_url = payload
                    .pointer("/git/repository_url")
                    .and_then(Value::as_str);
                if cwd.is_some() || git_url.is_some() {
                    let identity = project_identity(cwd, git_url);
                    if !identity.0.is_empty() {
                        last_project_key = identity.0;
                        last_project_ref = identity.1;
                    }
                }
            } else if payload.get("type").and_then(Value::as_str) == Some("thread_settings_applied")
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
                model: reported_model,
                project_key: last_project_key.clone(),
                project_ref: last_project_ref.clone(),
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
        if !last_project_key.is_empty() {
            cursor
                .last_project_keys
                .insert(logical_key.clone(), last_project_key);
            cursor
                .last_project_refs
                .insert(logical_key.clone(), last_project_ref);
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
