use serde_json::Value;
use std::env;
use std::path::Path;

use super::utils::*;
use crate::models::UsageRecord;

pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let mut cursor = FileCursor::from_json(cursor_data);
    let mut files_vec = Vec::new();

    // Default directory
    let otel_dir = home_dir.join(".copilot/otel");
    if otel_dir.exists() {
        let pattern = format!("{}/*.jsonl", otel_dir.display());
        files_vec.extend(cursor.glob_cached(&pattern, &otel_dir));
    }

    // Environment variable (single file)
    if let Ok(path_str) = env::var("COPILOT_OTEL_FILE_EXPORTER_PATH") {
        let p = std::path::PathBuf::from(&path_str);
        if p.exists() && !files_vec.contains(&p) {
            files_vec.push(p);
        }
    }

    if files_vec.is_empty() {
        return Ok((vec![], cursor.to_json()));
    }

    let mut all_records = Vec::new();

    for file in files_vec {
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
            if let Some(r) = parse_otel_record(&v, &mut cursor) {
                all_records.push(r);
            }
        }
        cursor.set_offset(&key, new_offset);
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}

/// Get attribute value from either object format or array format.
fn get_attr_str<'a>(attrs: &'a Value, key: &str) -> Option<&'a str> {
    if let Some(obj) = attrs.as_object() {
        return obj.get(key).and_then(|v| v.as_str());
    }
    if let Some(arr) = attrs.as_array() {
        for item in arr {
            if item.get("key").and_then(|k| k.as_str()) == Some(key) {
                let val = item.get("value")?;
                return val.get("stringValue").and_then(|s| s.as_str());
            }
        }
    }
    None
}

fn get_attr_int(attrs: &Value, key: &str) -> Option<u64> {
    if let Some(obj) = attrs.as_object() {
        return obj.get(key).and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        });
    }
    if let Some(arr) = attrs.as_array() {
        for item in arr {
            if item.get("key").and_then(|k| k.as_str()) == Some(key) {
                let val = item.get("value")?;
                return val.get("intValue").and_then(|v| {
                    v.as_u64()
                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                });
            }
        }
    }
    None
}

fn parse_otel_record(v: &Value, cursor: &mut FileCursor) -> Option<UsageRecord> {
    let attrs = v.get("attributes")?;

    // Filter: gen_ai.operation.name == "chat" or name starts with "chat "
    let is_chat = get_attr_str(attrs, "gen_ai.operation.name") == Some("chat")
        || v.get("name")
            .and_then(|n| n.as_str())
            .map(|n| n.starts_with("chat "))
            .unwrap_or(false);
    if !is_chat {
        return None;
    }

    // Dedup: traceId:spanId or record id
    let dedup_key = match (
        v.get("traceId").and_then(|t| t.as_str()),
        v.get("spanId").and_then(|s| s.as_str()),
    ) {
        (Some(t), Some(s)) => format!("{}:{}", t, s),
        _ => v
            .get("id")
            .and_then(|i| i.as_str())
            .map(|s| format!("resp:{}", s))?,
    };
    if !cursor.mark_seen(&dedup_key) {
        return None;
    }

    let raw_input = get_attr_int(attrs, "gen_ai.usage.input_tokens").unwrap_or(0);
    let output = get_attr_int(attrs, "gen_ai.usage.output_tokens").unwrap_or(0);
    let cache_read = get_attr_int(attrs, "gen_ai.usage.cache_read.input_tokens").unwrap_or(0);
    let cache_write = get_attr_int(attrs, "gen_ai.usage.cache_write.input_tokens").unwrap_or(0);
    let reasoning = get_attr_int(attrs, "gen_ai.usage.reasoning.output_tokens").unwrap_or(0);

    // input = raw_input - min(cache_read, raw_input)
    let input = raw_input - cache_read.min(raw_input);
    let total = input + output + cache_read + cache_write + reasoning;
    if total == 0 {
        return None;
    }

    let model = get_attr_str(attrs, "gen_ai.response.model")
        .or_else(|| get_attr_str(attrs, "gen_ai.request.model"))
        .unwrap_or("github-copilot")
        .to_string();

    // Timestamp: endTime or startTime as [secs, nanos]
    let bucket = extract_time(v.get("endTime"))
        .or_else(|| extract_time(v.get("startTime")))
        .unwrap_or_else(now_bucket);

    Some(UsageRecord {
        id: None,
        hour_start: bucket,
        source: "copilot".to_string(),
        model,
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: cache_read,
        cache_creation_input_tokens: cache_write,
        reasoning_output_tokens: reasoning,
        total_tokens: total,
        conversation_count: 1,
    })
}

/// Extract bucket from OTEL time field: [secs, nanos] array or integer secs.
fn extract_time(val: Option<&Value>) -> Option<String> {
    let v = val?;
    if let Some(arr) = v.as_array() {
        let secs = arr.first().and_then(|s| s.as_i64())?;
        return epoch_secs_to_bucket(secs);
    }
    v.as_i64().and_then(epoch_secs_to_bucket)
}
