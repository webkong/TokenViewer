use std::path::Path;
use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let mut all_records = Vec::new();
    let mut cursor = FileCursor::from_json(cursor_data);

    // Primary: tokens_generated.jsonl (Kiro IDE extension)
    #[cfg(target_os = "macos")]
    let tokens_file = home_dir.join("Library/Application Support/Kiro/User/globalStorage/kiro.kiroagent/dev_data/tokens_generated.jsonl");
    #[cfg(not(target_os = "macos"))]
    let tokens_file = home_dir.join(".config/Kiro/User/globalStorage/kiro.kiroagent/dev_data/tokens_generated.jsonl");

    if tokens_file.exists() {
        let key = tokens_file.to_string_lossy().to_string();
        let offset = cursor.get_offset(&key);
        let bucket = file_mtime_bucket(&tokens_file);
        let (mut records, new_offset) = parse_jsonl_file(&tokens_file, offset, "kiro", |v, src| {
            parse_kiro_token_line(v, src)
        });
        for r in &mut records {
            if r.hour_start.is_empty() {
                r.hour_start = bucket.clone();
            }
        }
        all_records.extend(records);
        cursor.set_offset(&key, new_offset);
    }

    // Secondary: ~/.kiro/sessions/cli/*.jsonl (Kiro CLI)
    let cli_pattern = format!("{}/.kiro/sessions/**/*.jsonl", home_dir.display());
    let cli_files = glob_files(&cli_pattern);
    for file in cli_files {
        let key = file.to_string_lossy().to_string();
        let offset = cursor.get_offset(&key);
        let bucket = file_mtime_bucket(&file);
        let (mut records, new_offset) = parse_jsonl_file(&file, offset, "kiro", |v, src| {
            // CLI sessions may have usage in message.usage or direct fields
            parse_kiro_cli_line(v, src)
        });
        for r in &mut records {
            if r.hour_start.is_empty() {
                r.hour_start = bucket.clone();
            }
        }
        all_records.extend(records);
        cursor.set_offset(&key, new_offset);
    }

    let aggregated = aggregate_records(all_records);
    Ok((aggregated, cursor.to_json()))
}

/// Parse tokens_generated.jsonl format:
/// {"model":"agent","provider":"kiro","promptTokens":13893,"generatedTokens":0}
fn parse_kiro_token_line(v: &Value, source: &str) -> Option<UsageRecord> {
    let prompt = v.get("promptTokens").and_then(|x| x.as_u64()).unwrap_or(0);
    let generated = v.get("generatedTokens").and_then(|x| x.as_u64()).unwrap_or(0);
    let total = prompt + generated;
    if total == 0 {
        return None;
    }
    let model = v.get("model").and_then(|m| m.as_str()).unwrap_or("kiro-agent");
    let model_name = if model == "agent" { "kiro-agent" } else { model };

    Some(UsageRecord {
        id: None,
        hour_start: String::new(), // filled by caller
        source: source.to_string(),
        model: model_name.to_string(),
        input_tokens: prompt,
        output_tokens: generated,
        cached_input_tokens: 0,
        cache_creation_input_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: total,
        conversation_count: 1,
    })
}

/// Parse Kiro CLI session JSONL (similar to Claude format)
fn parse_kiro_cli_line(v: &Value, source: &str) -> Option<UsageRecord> {
    let usage = v.pointer("/message/usage")
        .or_else(|| v.get("usage"))?;
    if !usage.is_object() {
        return None;
    }
    let model = v.pointer("/message/model")
        .or_else(|| v.get("model"))
        .and_then(|m| m.as_str())
        .unwrap_or("kiro-agent")
        .to_string();

    let input = usage.get("input_tokens").and_then(|x| x.as_u64())
        .or_else(|| usage.get("promptTokens").and_then(|x| x.as_u64()))
        .unwrap_or(0);
    let output = usage.get("output_tokens").and_then(|x| x.as_u64())
        .or_else(|| usage.get("generatedTokens").and_then(|x| x.as_u64()))
        .unwrap_or(0);
    let cache_read = usage.get("cache_read_input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
    let total = input + output + cache_read;
    if total == 0 {
        return None;
    }

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
        cache_creation_input_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: total,
        conversation_count: 1,
    })
}
