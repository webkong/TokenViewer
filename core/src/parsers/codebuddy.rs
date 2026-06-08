use std::path::Path;
use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let base = home_dir.join(".codebuddy/projects");
    if !base.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }
    let pattern = format!("{}/**/*.jsonl", base.display());
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

            // Only require providerData.rawUsage to exist
            let provider_data = match v.get("providerData") {
                Some(pd) => pd,
                None => continue,
            };
            let raw_usage = match provider_data.get("rawUsage") {
                Some(u) if u.is_object() => u,
                _ => continue,
            };

            // Dedup
            let dedup_id = v.get("uuid").and_then(|x| x.as_str())
                .or_else(|| v.get("id").and_then(|x| x.as_str()))
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}", line.len()));
            if !cursor.mark_seen(&dedup_id) {
                continue;
            }

            let prompt_tokens = raw_usage.get("prompt_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let completion_tokens = raw_usage.get("completion_tokens").and_then(|x| x.as_u64()).unwrap_or(0);

            let prompt_details = raw_usage.get("prompt_tokens_details").unwrap_or(&Value::Null);
            let details_cached = prompt_details.get("cached_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let cache_read_field = raw_usage.get("cache_read_input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let prompt_cache_hit = raw_usage.get("prompt_cache_hit_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let cache_read = details_cached.max(cache_read_field).max(prompt_cache_hit);

            let cache_creation = raw_usage.get("cache_creation_input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let completion_details = raw_usage.get("completion_tokens_details").unwrap_or(&Value::Null);
            let reasoning = completion_details.get("reasoning_tokens").and_then(|x| x.as_u64()).unwrap_or(0);

            let input = prompt_tokens.saturating_sub(cache_read);
            let output = completion_tokens;
            let total = input + output + cache_read + cache_creation + reasoning;
            if total == 0 {
                continue;
            }

            let model = provider_data.get("model").and_then(|m| m.as_str())
                .or_else(|| v.get("model").and_then(|m| m.as_str()))
                .unwrap_or("codebuddy-agent")
                .to_string();

            let hour_start = v.get("timestamp")
                .and_then(|t| t.as_i64())
                .and_then(epoch_millis_to_bucket)
                .unwrap_or_else(|| file_mtime_bucket(&file));

            all_records.push(UsageRecord {
                id: None,
                hour_start,
                source: "codebuddy".to_string(),
                model,
                input_tokens: input,
                output_tokens: output,
                cached_input_tokens: cache_read,
                cache_creation_input_tokens: cache_creation,
                reasoning_output_tokens: reasoning,
                total_tokens: total,
                conversation_count: 1,
            });
        }

        cursor.set_offset(&key, new_offset);
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}
