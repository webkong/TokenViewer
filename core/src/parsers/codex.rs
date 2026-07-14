use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::utils::*;
use crate::models::UsageRecord;

pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let mut cursor = FileCursor::from_json(cursor_data);
    let mut all_records = Vec::new();

    let standard_base = home_dir.join(".codex/sessions");
    let mut session_bases = vec![standard_base.clone()];
    let mut seen_bases = std::collections::HashSet::new();
    seen_bases.insert(standard_base);

    // $CODEX_HOME overrides/extends where Codex looks for its config + session
    // data. Some launchers (e.g. sandboxed dev tooling) redirect it to a
    // sandboxed runtime home, so rollout files land outside ~/.codex. Codex
    // itself (and reference tools like ccusage) treat $CODEX_HOME as one
    // directory OR a comma-separated list of directories — mirror that so any
    // tool following the same convention is picked up without special-casing
    // it by name. Missing entries are silently skipped (scan_codex_base is a
    // no-op if the path doesn't exist), so an unresolvable/irrelevant entry
    // costs nothing.
    //
    // Only consult it when `home_dir` is the real OS home directory — never
    // for synthetic/test home dirs — so behavior (and tests, which use temp
    // dirs) is unaffected by whatever $CODEX_HOME happens to be set to in the
    // calling process's shell.
    if dirs::home_dir().as_deref() == Some(home_dir) {
        if let Ok(raw) = std::env::var("CODEX_HOME") {
            for entry in raw.split(',') {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                let alt_base = PathBuf::from(entry).join("sessions");
                if seen_bases.insert(alt_base.clone()) {
                    session_bases.push(alt_base);
                }
            }
        }

        // Orca runs Codex with a sandboxed runtime home. TokenViewer is often
        // launched from Finder or a login item and therefore cannot rely on
        // inheriting Orca's CODEX_HOME environment variable. Discover Orca's
        // stable macOS runtime path directly so those sessions are always
        // included in usage sync.
        let orca_base =
            home_dir.join("Library/Application Support/orca/codex-runtime-home/home/sessions");
        if seen_bases.insert(orca_base.clone()) {
            session_bases.push(orca_base);
        }
    }

    let mut seen_rollouts = std::collections::HashSet::new();
    for base in session_bases {
        scan_codex_base(
            &base,
            &mut cursor,
            &mut all_records,
            &mut seen_rollouts,
            "codex",
        );
    }

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
    let mut seen_rollouts = std::collections::HashSet::new();
    scan_codex_base(
        &base,
        &mut cursor,
        &mut all_records,
        &mut seen_rollouts,
        source,
    );
    Ok((aggregate_records(all_records), cursor.to_json()))
}

/// Scan a single sessions base directory, appending matched records into
/// `all_records` and advancing `cursor` in place. No-op if `base` doesn't exist.
fn scan_codex_base(
    base: &Path,
    cursor: &mut FileCursor,
    all_records: &mut Vec<UsageRecord>,
    seen_rollouts: &mut std::collections::HashSet<OsString>,
    source: &str,
) {
    if !base.exists() {
        return;
    }
    let pattern = format!("{}/**/rollout-*.jsonl", base.display());
    let files = cursor.glob_cached(&pattern, base);

    for file in files {
        // Orca mirrors standard Codex sessions as hard links under its runtime
        // home. A rollout filename contains the globally unique session ID, so
        // use it as the cross-root identity and import each session only once.
        let Some(rollout_name) = file.file_name().map(OsString::from) else {
            continue;
        };
        if !seen_rollouts.insert(rollout_name) {
            continue;
        }
        let key = file.to_string_lossy().to_string();
        if !cursor.file_changed(&key) {
            continue;
        }
        let offset = cursor.get_offset(&key);
        let file_len = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
        let start_offset = if offset > file_len {
            cursor.last_models.remove(&key);
            cursor.last_providers.remove(&key);
            0
        } else {
            offset
        };
        let mut last_model = cursor
            .last_models
            .get(&key)
            .cloned()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| String::from("unknown"));
        let mut last_provider = cursor.last_providers.get(&key).cloned().unwrap_or_default();
        let (lines, new_offset) = match read_lines_from_offset(&file, start_offset) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if lines.is_empty() {
            cursor.set_offset(&key, new_offset);
            if !last_model.is_empty() {
                cursor.last_models.insert(key.clone(), last_model);
            }
            if !last_provider.is_empty() {
                cursor.last_providers.insert(key.clone(), last_provider);
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

            // Track model from turn_context / session_meta
            if event_type == "session_meta" {
                if let Some(p) = payload.get("model_provider").and_then(|m| m.as_str()) {
                    if !p.is_empty() {
                        last_provider = p.to_string();
                    }
                }
            }

            if event_type == "turn_context" || event_type == "session_meta" {
                if let Some(m) = payload.get("model").and_then(|m| m.as_str()) {
                    if !m.is_empty() {
                        last_model = m.to_string();
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
                let d = cursor.delta(&key, cur);
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
            if total == 0 {
                continue;
            }

            let hour_start = v
                .get("timestamp")
                .and_then(|t| t.as_str())
                .map(iso_to_bucket)
                .unwrap_or_else(|| bucket.clone());

            all_records.push(UsageRecord {
                id: None,
                hour_start,
                source: source.to_string(),
                model: {
                    if !last_model.is_empty() && last_model != "unknown" {
                        last_model.clone()
                    } else if !last_provider.is_empty() {
                        last_provider.clone()
                    } else {
                        last_model.clone()
                    }
                },
                input_tokens: fi,
                output_tokens: fo,
                cached_input_tokens: fc_read,
                cache_creation_input_tokens: fc_write,
                reasoning_output_tokens: fr,
                total_tokens: total,
                conversation_count: 1,
            });
        }

        cursor.set_offset(&key, new_offset);
        if !last_model.is_empty() {
            cursor.last_models.insert(key.clone(), last_model);
        }
        if !last_provider.is_empty() {
            cursor.last_providers.insert(key.clone(), last_provider);
        }
    }
}
