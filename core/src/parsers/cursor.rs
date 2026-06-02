use std::path::Path;
use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    // Simplified: read usage.json from Cursor's globalStorage
    #[cfg(target_os = "macos")]
    let usage_path = home_dir.join("Library/Application Support/Cursor/User/globalStorage/cursorDiskModel/usage.json");
    #[cfg(target_os = "linux")]
    let usage_path = home_dir.join(".config/Cursor/User/globalStorage/cursorDiskModel/usage.json");
    #[cfg(target_os = "windows")]
    let usage_path = home_dir.join("AppData/Roaming/Cursor/User/globalStorage/cursorDiskModel/usage.json");

    if !usage_path.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let mut cursor = FileCursor::from_json(cursor_data);
    let content = match read_to_string_capped(&usage_path) {
        Some(c) => c,
        None => return Ok((vec![], cursor.to_json())),
    };
    let v: Value = serde_json::from_str(&content)?;

    let mut records = Vec::new();
    // usage.json typically has an array of usage entries or a map
    if let Some(arr) = v.as_array() {
        for entry in arr {
            if let Some(r) = parse_cursor_entry(entry) {
                records.push(r);
            }
        }
    } else if let Some(obj) = v.as_object() {
        // Could be keyed by date or model
        for (_key, entry) in obj {
            if let Some(r) = parse_cursor_entry(entry) {
                records.push(r);
            }
        }
    }

    // Filter by cursor timestamp
    if let Some(last_ts) = &cursor.last_timestamp {
        records.retain(|r| r.hour_start > *last_ts);
    }
    if let Some(last) = records.iter().map(|r| &r.hour_start).max() {
        cursor.last_timestamp = Some(last.clone());
    }

    let aggregated = aggregate_records(records);
    Ok((aggregated, cursor.to_json()))
}

fn parse_cursor_entry(v: &Value) -> Option<UsageRecord> {
    let model = v.get("model").and_then(|m| m.as_str()).unwrap_or("unknown").to_string();
    let input = v.get("inputTokens").or_else(|| v.get("input_tokens")).and_then(|x| x.as_u64()).unwrap_or(0);
    let output = v.get("outputTokens").or_else(|| v.get("output_tokens")).and_then(|x| x.as_u64()).unwrap_or(0);
    let total = input + output;
    if total == 0 {
        return None;
    }
    let hour_start = v.get("timestamp")
        .and_then(|t| t.as_str())
        .map(iso_to_bucket)
        .or_else(|| v.get("startedAt").and_then(|t| t.as_str()).map(iso_to_bucket))
        .unwrap_or_else(now_bucket);

    Some(UsageRecord {
        id: None,
        hour_start,
        source: "cursor".to_string(),
        model,
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: 0,
        cache_creation_input_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: total,
        conversation_count: 1,
    })
}
