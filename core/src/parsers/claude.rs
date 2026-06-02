use std::path::Path;
use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

/// Parse Claude Code JSONL logs from ~/.claude/projects/
/// Lines with "usage" field contain message.usage or usage with token data.
/// Model is in message.model or model field.
fn parse_claude_line(v: &Value, source: &str) -> Option<UsageRecord> {
    // Usage can be at message.usage or top-level usage
    let usage = v.pointer("/message/usage")
        .or_else(|| v.get("usage"))?;

    if !usage.is_object() {
        return None;
    }

    // Model from message.model or top-level model
    let model = v.pointer("/message/model")
        .or_else(|| v.get("model"))
        .and_then(|m| m.as_str())
        .unwrap_or("claude-unknown")
        .to_string();

    let input = usage.get("input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
    let output = usage.get("output_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
    let cache_creation = usage.get("cache_creation_input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
    let cache_read = usage.get("cache_read_input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
    let total = input + output + cache_creation + cache_read;

    if total == 0 {
        return None;
    }

    // Get timestamp from the line
    let hour_start = v.get("timestamp")
        .and_then(|t| t.as_str())
        .map(|s| iso_to_bucket(s))
        .unwrap_or_default();

    Some(UsageRecord {
        id: None,
        hour_start,
        source: source.to_string(),
        model,
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: cache_read,
        cache_creation_input_tokens: cache_creation,
        reasoning_output_tokens: 0,
        total_tokens: total,
        conversation_count: 1,
    })
}

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    parse_claude_format(home_dir, cursor_data, ".claude/projects", "claude")
}

/// Shared logic for Claude-format JSONL (used by claude and codebuddy).
pub fn parse_claude_format(
    home_dir: &Path,
    cursor_data: Option<&str>,
    rel_dir: &str,
    source: &str,
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
        let (records, new_offset) = parse_jsonl_file(&file, offset, source, |v, src| {
            // Quick filter: only parse lines that contain usage data
            if v.pointer("/message/usage").is_none() && v.get("usage").is_none() {
                return None;
            }
            parse_claude_line(v, src)
        });
        all_records.extend(records);
        cursor.set_offset(&key, new_offset);
    }

    let aggregated = aggregate_records(all_records);
    Ok((aggregated, cursor.to_json()))
}
