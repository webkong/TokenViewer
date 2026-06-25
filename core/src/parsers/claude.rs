use serde_json::Value;
use std::path::Path;

use super::utils::*;
use crate::models::UsageRecord;

/// Parse Claude Code JSONL logs from ~/.claude/projects/
/// Lines with "usage" field contain message.usage or usage with token data.
/// Model is in message.model or model field.
fn parse_claude_line(v: &Value, source: &str) -> Option<UsageRecord> {
    // Usage can be at message.usage or top-level usage
    let usage = v.pointer("/message/usage").or_else(|| v.get("usage"))?;

    if !usage.is_object() {
        return None;
    }

    // Skip if no valid timestamp
    let hour_start = v
        .get("timestamp")
        .and_then(|t| t.as_str())
        .filter(|s| !s.is_empty())
        .map(iso_to_bucket)?;

    // Model from message.model or top-level model
    let model = v
        .pointer("/message/model")
        .or_else(|| v.get("model"))
        .and_then(|m| m.as_str())
        .unwrap_or("claude-unknown")
        .to_string();

    let input = usage
        .get("input_tokens")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let output = usage
        .get("output_tokens")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let cache_creation = usage
        .get("cache_creation_input_tokens")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let cache_read = usage
        .get("cache_read_input_tokens")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let total = input + output + cache_creation + cache_read;

    if total == 0 {
        return None;
    }

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
        conversation_count: 0, // set by caller from user-turn count
    })
}

/// True if a Claude `type:"user"` line is a real typed prompt (has a text block),
/// as opposed to an auto-generated tool_result message.
fn claude_user_has_text(v: &Value) -> bool {
    match v.pointer("/message/content") {
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(arr)) => arr
            .iter()
            .any(|b| b.get("type").and_then(|t| t.as_str()) == Some("text")),
        _ => false,
    }
}

pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
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
    let mut cursor = FileCursor::from_json(cursor_data);
    let files = cursor.glob_cached(&pattern, &base);
    let mut all_records = Vec::new();

    for file in files {
        let key = file.to_string_lossy().to_string();
        if !cursor.file_changed(&key) {
            continue;
        }
        // Conversations are counted from real user prompts in MAIN sessions only
        // (subagent files contribute tokens but not conversation turns), matching
        // the reference implementation.
        let is_main = !key.contains("/subagents/");
        let offset = cursor.get_offset(&key);
        let (lines, new_offset) = match read_lines_from_offset(&file, offset) {
            Ok(r) => r,
            Err(_) => continue,
        };

        // Accumulate user-typed turns and attribute them to the next assistant
        // usage record (so conv counts land on a real model bucket, not a synthetic
        // one). One user prompt typically triggers several assistant messages; only
        // the first carries the turn count, the rest carry 0 — so conv ≈ #prompts.
        let mut pending_convs: u32 = 0;
        for line in &lines {
            let v: serde_json::Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Count real user prompts as conversation turns.
            if is_main
                && v.get("type").and_then(|t| t.as_str()) == Some("user")
                && claude_user_has_text(&v)
            {
                pending_convs += 1;
            }

            if v.pointer("/message/usage").is_none() && v.get("usage").is_none() {
                continue;
            }
            // Dedup by message.id + requestId
            let msg_id = v
                .pointer("/message/id")
                .and_then(|id| id.as_str())
                .unwrap_or("");
            if !msg_id.is_empty() {
                let req_id = v.get("requestId").and_then(|id| id.as_str()).unwrap_or("");
                let dedup_key = if !req_id.is_empty() {
                    format!("{}:{}", msg_id, req_id)
                } else {
                    msg_id.to_string()
                };
                if !cursor.mark_seen(&dedup_key) {
                    continue;
                }
            }
            if let Some(mut record) = parse_claude_line(&v, source) {
                record.conversation_count = pending_convs;
                pending_convs = 0;
                all_records.push(record);
            }
        }
        cursor.set_offset(&key, new_offset);
    }

    let aggregated = aggregate_records(all_records);
    Ok((aggregated, cursor.to_json()))
}
