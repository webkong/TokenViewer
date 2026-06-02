use std::fs;
use std::path::Path;
use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let base = home_dir.join(".gemini/tmp");
    if !base.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }
    let pattern = format!("{}/*/chats/session-*.json", base.display());
    let files = glob_files(&pattern);
    let mut cursor = FileCursor::from_json(cursor_data);
    let mut all_records = Vec::new();

    for file in files {
        let key = file.to_string_lossy().to_string();
        let prev_offset = cursor.get_offset(&key);
        let file_len = fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
        if prev_offset >= file_len && prev_offset > 0 {
            continue;
        }

        let content = match read_to_string_capped(&file) {
            Some(c) => c,
            None => continue,
        };
        let v: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let messages = match v.get("messages").and_then(|m| m.as_array()) {
            Some(m) => m,
            None => continue,
        };

        // Accumulate totals for this file
        let mut sum_input: u64 = 0;
        let mut sum_output: u64 = 0;
        let mut sum_cached: u64 = 0;
        let mut sum_thoughts: u64 = 0;
        let mut last_model = String::new();
        let mut last_ts = String::new();

        for msg in messages {
            if let Some(tokens) = msg.get("tokens") {
                sum_input += tokens.get("input").and_then(|x| x.as_u64()).unwrap_or(0);
                let out = tokens.get("output").and_then(|x| x.as_u64()).unwrap_or(0);
                let tool = tokens.get("tool").and_then(|x| x.as_u64()).unwrap_or(0);
                sum_output += out + tool;
                sum_cached += tokens.get("cached").and_then(|x| x.as_u64()).unwrap_or(0);
                sum_thoughts += tokens.get("thoughts").and_then(|x| x.as_u64()).unwrap_or(0);
            }
            if let Some(m) = msg.get("model").and_then(|m| m.as_str()) {
                if !m.is_empty() {
                    last_model = m.to_string();
                }
            }
            if let Some(ts) = msg.get("timestamp").and_then(|t| t.as_str()) {
                if !ts.is_empty() {
                    last_ts = ts.to_string();
                }
            }
        }

        // Delta against previous cumulative snapshot
        let cur = [sum_input, sum_output, sum_cached, 0, sum_thoughts];
        let d = cursor.delta(&key, cur);
        let total = d[0] + d[1] + d[2] + d[3] + d[4];
        if total > 0 {
            let hour_start = if !last_ts.is_empty() {
                iso_to_bucket(&last_ts)
            } else {
                file_mtime_bucket(&file)
            };
            let model = if last_model.is_empty() { "gemini".to_string() } else { last_model };

            all_records.push(UsageRecord {
                id: None,
                hour_start,
                source: "gemini".to_string(),
                model,
                input_tokens: d[0],
                output_tokens: d[1],
                cached_input_tokens: d[2],
                cache_creation_input_tokens: d[3],
                reasoning_output_tokens: d[4],
                total_tokens: total,
                conversation_count: 1,
            });
        }

        cursor.set_offset(&key, file_len);
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}
