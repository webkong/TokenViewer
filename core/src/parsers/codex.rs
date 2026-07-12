use serde_json::Value;
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
    scan_codex_base(&standard_base, &mut cursor, &mut all_records, "codex");

    // Some launchers (e.g. sandboxed dev tooling) redirect Codex's own config
    // root via $CODEX_HOME, so its rollout files land outside ~/.codex.
    // Only consult it as an *additional* location when `home_dir` is the
    // real OS home directory — never for synthetic/test home dirs — so
    // behavior (and tests, which use temp dirs) is unaffected by whatever
    // $CODEX_HOME happens to be set to in the calling process's shell.
    if dirs::home_dir().as_deref() == Some(home_dir) {
        if let Ok(dir) = std::env::var("CODEX_HOME") {
            if !dir.is_empty() {
                let alt_base = PathBuf::from(dir).join("sessions");
                if alt_base != standard_base {
                    scan_codex_base(&alt_base, &mut cursor, &mut all_records, "codex");
                }
            }
        }
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
    scan_codex_base(&base, &mut cursor, &mut all_records, source);
    Ok((aggregate_records(all_records), cursor.to_json()))
}

/// Scan a single sessions base directory, appending matched records into
/// `all_records` and advancing `cursor` in place. No-op if `base` doesn't exist.
fn scan_codex_base(
    base: &Path,
    cursor: &mut FileCursor,
    all_records: &mut Vec<UsageRecord>,
    source: &str,
) {
    if !base.exists() {
        return;
    }
    let pattern = format!("{}/**/rollout-*.jsonl", base.display());
    let files = cursor.glob_cached(&pattern, base);

    for file in files {
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
