use std::path::Path;
use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    parse_codex_format(home_dir, cursor_data, ".codex/sessions", "codex")
}

pub fn parse_codex_format(
    home_dir: &Path,
    cursor_data: Option<&str>,
    rel_dir: &str,
    source: &str,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let base = home_dir.join(rel_dir);
    if !base.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }
    let pattern = format!("{}/**/rollout-*.jsonl", base.display());
    let mut cursor = FileCursor::from_json(cursor_data);
    let files = cursor.glob_cached(&pattern, &base);
    let mut all_records = Vec::new();

    for file in files {
        let key = file.to_string_lossy().to_string();
        if !cursor.file_changed(&key) {
            continue;
        }
        let offset = cursor.get_offset(&key);
        let (lines, new_offset) = match read_lines_from_offset(&file, offset) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if lines.is_empty() {
            cursor.set_offset(&key, new_offset);
            continue;
        }

        let mut last_model = String::from("unknown");
        let bucket = file_mtime_bucket(&file);

        for line in &lines {
            let v: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let event_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
            let payload = v.get("payload").unwrap_or(&Value::Null);

            // Track model from turn_context / session_meta
            if event_type == "turn_context" || event_type == "session_meta" {
                if let Some(m) = payload.get("model").and_then(|m| m.as_str()) {
                    if !m.is_empty() {
                        last_model = m.to_string();
                    }
                }
            }

            // Check for token_count in payload.type or payload.msg.type
            let is_token_count = payload.get("type").and_then(|t| t.as_str()) == Some("token_count")
                || payload.get("msg").and_then(|m| m.get("type")).and_then(|t| t.as_str()) == Some("token_count");

            if !is_token_count {
                continue;
            }

            let info = if payload.get("type").and_then(|t| t.as_str()) == Some("token_count") {
                payload.get("info").cloned().unwrap_or(Value::Null)
            } else {
                payload.get("msg").and_then(|m| m.get("info")).cloned().unwrap_or(Value::Null)
            };

            // Prefer total_token_usage (cumulative) with delta, fallback to last_token_usage
            let (usage, use_delta) = if let Some(u) = info.get("total_token_usage") {
                (u, true)
            } else if let Some(u) = info.get("last_token_usage") {
                (u, false)
            } else {
                continue;
            };

            let raw_input = usage.get("input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let output = usage.get("output_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let cached = usage.get("cached_input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let cache_creation = usage.get("cache_creation_input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let reasoning = usage.get("reasoning_output_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let input = raw_input.saturating_sub(cached);

            let (fi, fo, fc_read, fc_write, fr) = if use_delta {
                let cur = [input, output, cached, cache_creation, reasoning];
                let d = cursor.delta(&key, cur);
                (d[0], d[1], d[2], d[3], d[4])
            } else {
                (input, output, cached, cache_creation, reasoning)
            };

            let total = fi + fo + fc_read + fc_write + fr;
            if total == 0 {
                continue;
            }

            let hour_start = v.get("timestamp")
                .and_then(|t| t.as_str())
                .map(iso_to_bucket)
                .unwrap_or_else(|| bucket.clone());

            all_records.push(UsageRecord {
                id: None,
                hour_start,
                source: source.to_string(),
                model: last_model.clone(),
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
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}
