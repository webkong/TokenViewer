use std::path::Path;
use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    parse_pi_session(home_dir, cursor_data, ".omp/agent/sessions", "ohmypi", "omp-unknown")
}

/// Shared parser for ohmypi/pi session JSONL files.
pub fn parse_pi_session(
    home_dir: &Path,
    cursor_data: Option<&str>,
    rel_dir: &str,
    source: &str,
    default_model: &str,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let base = home_dir.join(rel_dir);
    if !base.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let pattern = format!("{}/**/*.jsonl", base.display());
    let files = glob_files(&pattern);
    let mut cursor = FileCursor::from_json(cursor_data);
    let mut all_records = Vec::new();

    for file in files {
        let key = file.to_string_lossy().to_string();
        let offset = cursor.get_offset(&key);
        let (lines, new_offset) = match read_lines_from_offset(&file, offset) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for line in &lines {
            let v: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if let Some(r) = parse_line(&v, source, default_model, &mut cursor) {
                all_records.push(r);
            }
        }
        cursor.set_offset(&key, new_offset);
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}

fn parse_line(v: &Value, source: &str, default_model: &str, cursor: &mut FileCursor) -> Option<UsageRecord> {
    if v.get("type").and_then(|t| t.as_str()) != Some("message") {
        return None;
    }
    let msg = v.get("message")?;
    if msg.get("role").and_then(|r| r.as_str()) != Some("assistant") {
        return None;
    }
    let usage = msg.get("usage")?;

    // Dedup
    let id = v.get("id").and_then(|i| i.as_str())?;
    if !cursor.mark_seen(id) {
        return None;
    }

    let input = usage.get("input").and_then(|x| x.as_u64()).unwrap_or(0);
    let output = usage.get("output").and_then(|x| x.as_u64()).unwrap_or(0);
    let cached = usage.get("cacheRead").and_then(|x| x.as_u64()).unwrap_or(0);
    let cache_creation = usage.get("cacheWrite").and_then(|x| x.as_u64()).unwrap_or(0);
    let reasoning = usage.get("reasoningTokens").and_then(|x| x.as_u64()).unwrap_or(0);
    let total = usage.get("totalTokens").and_then(|x| x.as_u64())
        .unwrap_or(input + output + cached + cache_creation + reasoning);
    if total == 0 { return None; }

    let model = msg.get("model").and_then(|m| m.as_str()).unwrap_or(default_model).to_string();

    // Timestamp: message.timestamp (ms) or entry.timestamp (ISO)
    let bucket = msg.get("timestamp").and_then(|t| t.as_i64()).and_then(epoch_millis_to_bucket)
        .or_else(|| v.get("timestamp").and_then(|t| t.as_str()).map(iso_to_bucket))
        .unwrap_or_else(now_bucket);

    Some(UsageRecord {
        id: None,
        hour_start: bucket,
        source: source.to_string(),
        model,
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: cached,
        cache_creation_input_tokens: cache_creation,
        reasoning_output_tokens: reasoning,
        total_tokens: total,
        conversation_count: 1,
    })
}
