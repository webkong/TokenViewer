use std::path::Path;
use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    parse_ui_messages(home_dir, cursor_data, "rooveterinaryinc.roo-cline", "roocode")
}

/// Shared parser for VS Code extensions that write tasks/*/ui_messages.json.
pub fn parse_ui_messages(
    home_dir: &Path,
    cursor_data: Option<&str>,
    ext_id: &str,
    source: &str,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let storage = vscode_global_storage(home_dir).join(ext_id);
    if !storage.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let pattern = format!("{}/tasks/*/ui_messages.json", storage.display());
    let mut cursor = FileCursor::from_json(cursor_data);
    let files = cursor.glob_cached(&pattern, &storage);
    let mut all_records = Vec::new();

    for file in files {
        let key = file.to_string_lossy().to_string();
        if !cursor.file_changed(&key) { continue; }
        let prev = cursor.get_offset(&key);
        let file_len = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
        if prev >= file_len && prev > 0 { continue; }

        let content = match read_to_string_capped(&file) {
            Some(c) => c,
            None => continue,
        };
        let messages: Vec<Value> = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(_) => continue,
        };

        for msg in &messages {
            let say = msg.get("say").and_then(|s| s.as_str()).unwrap_or("");
            if say != "api_req_started" && say != "api_req_deleted" {
                continue;
            }
            let text_str = match msg.get("text").and_then(|t| t.as_str()) {
                Some(s) => s,
                None => continue,
            };
            let payload: Value = match serde_json::from_str(text_str) {
                Ok(p) => p,
                Err(_) => continue,
            };

            let ts = msg.get("ts").and_then(|t| t.as_i64()).unwrap_or(0);
            let ts_key = format!("{}", ts);
            if !cursor.mark_seen(&ts_key) { continue; }

            let input = payload.get("tokensIn").and_then(|x| x.as_u64()).unwrap_or(0);
            let output = payload.get("tokensOut").and_then(|x| x.as_u64()).unwrap_or(0);
            let cached = payload.get("cacheReads").and_then(|x| x.as_u64()).unwrap_or(0);
            let cache_creation = payload.get("cacheWrites").and_then(|x| x.as_u64()).unwrap_or(0);
            let total = input + output + cached + cache_creation;
            if total == 0 { continue; }

            let model = payload.get("inferenceProvider")
                .and_then(|p| p.as_str())
                .map(|s| format!("provider:{}", s))
                .unwrap_or_else(|| "unknown".to_string());

            let bucket = epoch_millis_to_bucket(ts).unwrap_or_else(now_bucket);

            all_records.push(UsageRecord {
                id: None,
                hour_start: bucket,
                source: source.to_string(),
                model,
                input_tokens: input,
                output_tokens: output,
                cached_input_tokens: cached,
                cache_creation_input_tokens: cache_creation,
                reasoning_output_tokens: 0,
                total_tokens: total,
                conversation_count: 1,
            });
        }
        cursor.set_offset(&key, file_len);
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}
