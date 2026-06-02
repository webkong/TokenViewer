use std::path::Path;
use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let base = home_dir.join(".kimi/sessions");
    if !base.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let pattern = format!("{}/**/wire.jsonl", base.display());
    let mut cursor = FileCursor::from_json(cursor_data);
    let files = cursor.glob_cached(&pattern, &base);
    let mut all_records = Vec::new();

    for file in files {
        let key = file.to_string_lossy().to_string();
        if !cursor.file_changed(&key) { continue; }
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
            if let Some(r) = parse_kimi_line(&v, &mut cursor) {
                all_records.push(r);
            }
        }
        cursor.set_offset(&key, new_offset);
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}

fn parse_kimi_line(v: &Value, cursor: &mut FileCursor) -> Option<UsageRecord> {
    let msg = v.get("message")?;
    if msg.get("type").and_then(|t| t.as_str()) != Some("StatusUpdate") {
        return None;
    }
    let payload = msg.get("payload")?;
    let usage = payload.get("token_usage")?;

    let msg_id = payload.get("message_id").and_then(|m| m.as_str())?;
    if !cursor.mark_seen(msg_id) {
        return None;
    }

    let input = usage.get("input_other").and_then(|x| x.as_u64()).unwrap_or(0);
    let output = usage.get("output").and_then(|x| x.as_u64()).unwrap_or(0);
    let cached = usage.get("input_cache_read").and_then(|x| x.as_u64()).unwrap_or(0);
    let cache_creation = usage.get("input_cache_creation").and_then(|x| x.as_u64()).unwrap_or(0);
    let total = input + output + cached + cache_creation;
    if total == 0 { return None; }

    let bucket = v.get("timestamp").and_then(|t| t.as_str()).map(iso_to_bucket)
        .or_else(|| payload.get("timestamp").and_then(|t| t.as_i64()).and_then(epoch_secs_to_bucket))
        .unwrap_or_else(now_bucket);

    Some(UsageRecord {
        id: None,
        hour_start: bucket,
        source: "kimi".to_string(),
        model: "kimi-for-coding".to_string(),
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: cached,
        cache_creation_input_tokens: cache_creation,
        reasoning_output_tokens: 0,
        total_tokens: total,
        conversation_count: 1,
    })
}
