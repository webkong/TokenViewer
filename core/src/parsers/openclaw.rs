use std::path::Path;
use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let base = std::env::var("TOKENTRACKER_OPENCLAW_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| home_dir.join(".openclaw"));

    let agents_dir = base.join("agents");
    if !agents_dir.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let pattern = format!("{}/**/sessions/*.jsonl", agents_dir.display());
    let mut cursor = FileCursor::from_json(cursor_data);
    let files = cursor.glob_cached(&pattern, &agents_dir);
    let mut all_records = Vec::new();

    for file in files {
        let key = file.to_string_lossy().to_string();
        if !cursor.file_changed(&key) { continue; }
        let offset = cursor.get_offset(&key);
        let (records, new_offset) = parse_jsonl_file(&file, offset, "openclaw", parse_line);
        all_records.extend(records);
        cursor.set_offset(&key, new_offset);
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}

fn parse_line(v: &Value, source: &str) -> Option<UsageRecord> {
    if v.get("type").and_then(|t| t.as_str()) != Some("message") {
        return None;
    }

    let message = v.get("message")?;
    let usage = message.get("usage")?;

    let input = usage.get("input").and_then(|x| x.as_u64()).unwrap_or(0);
    let output = usage.get("output").and_then(|x| x.as_u64()).unwrap_or(0);
    let cached = usage.get("cacheRead").and_then(|x| x.as_u64()).unwrap_or(0);
    let cache_write = usage.get("cacheWrite").and_then(|x| x.as_u64()).unwrap_or(0);
    let total = input + output + cached + cache_write;

    if total == 0 {
        return None;
    }

    let model = message.get("model").and_then(|m| m.as_str()).unwrap_or("unknown").to_string();

    let hour_start = v.get("timestamp")
        .and_then(|t| t.as_str())
        .map(|s| iso_to_bucket(s))
        .unwrap_or_else(now_bucket);

    Some(UsageRecord {
        id: None,
        hour_start,
        source: source.to_string(),
        model,
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: cached,
        cache_creation_input_tokens: cache_write,
        reasoning_output_tokens: 0,
        total_tokens: total,
        conversation_count: 1,
    })
}
