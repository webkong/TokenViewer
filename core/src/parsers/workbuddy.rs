use serde_json::Value;
use std::path::Path;

use super::utils::*;
use crate::models::UsageRecord;

/// Parse real WorkBuddy conversation logs. WorkBuddy stores sessions in the
/// same JSONL format as CodeBuddy (~/.codebuddy/projects), under
/// `~/.workbuddy/projects/**/*.jsonl` — per-message `providerData.rawUsage`
/// with prompt/completion/cache token counts and an epoch-ms `timestamp`.
///
/// NOTE: this used to read the quota snapshot (~/.antigravity_cockpit/
/// workbuddy_accounts/*.json) and record consumed *credits* as if they were
/// tokens, dumping the whole delta onto a single refresh-time bucket. That
/// misattributed expired July packages (Status 3) to one day in August and
/// ignored the real logs entirely. Actual token usage lives in the jsonl.
pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let base = home_dir.join(".workbuddy/projects");
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
            let dedup_id = v
                .get("uuid")
                .and_then(|x| x.as_str())
                .or_else(|| v.get("id").and_then(|x| x.as_str()))
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("{}", line.len()));
            if !cursor.mark_seen(&dedup_id) {
                continue;
            }

            let prompt_tokens = raw_usage
                .get("prompt_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let completion_tokens = raw_usage
                .get("completion_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);

            let prompt_details = raw_usage
                .get("prompt_tokens_details")
                .unwrap_or(&Value::Null);
            let details_cached = prompt_details
                .get("cached_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let cache_read_field = raw_usage
                .get("cache_read_input_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let prompt_cache_hit = raw_usage
                .get("prompt_cache_hit_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let cache_read = details_cached.max(cache_read_field).max(prompt_cache_hit);

            let cache_creation = raw_usage
                .get("cache_creation_input_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let completion_details = raw_usage
                .get("completion_tokens_details")
                .unwrap_or(&Value::Null);
            let reasoning = completion_details
                .get("reasoning_tokens")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);

            let input = prompt_tokens.saturating_sub(cache_read);
            let output = completion_tokens;
            let total = input + output + cache_read + cache_creation + reasoning;
            if total == 0 {
                continue;
            }

            let model = provider_data
                .get("model")
                .and_then(|m| m.as_str())
                .or_else(|| v.get("model").and_then(|m| m.as_str()))
                .unwrap_or("workbuddy-agent")
                .to_string();

            let hour_start = v
                .get("timestamp")
                .and_then(|t| t.as_i64())
                .and_then(epoch_millis_to_bucket)
                .unwrap_or_else(|| file_mtime_bucket(&file));

            all_records.push(UsageRecord {
                id: None,
                hour_start,
                source: "workbuddy".to_string(),
                model,
                project_key: String::new(),
                project_ref: String::new(),
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
