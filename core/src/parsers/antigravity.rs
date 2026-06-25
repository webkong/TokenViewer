use serde_json::Value;
use std::path::Path;

use super::utils::*;
use crate::models::UsageRecord;

pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let gemini_dir = home_dir.join(".gemini");
    if !gemini_dir.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let mut cursor = FileCursor::from_json(cursor_data);
    let mut all_records = Vec::new();

    // Scan multiple antigravity directories
    let dirs = ["antigravity", "antigravity-ide", "antigravity-cli"];
    for dir in &dirs {
        let pattern = format!("{}/{}/brain/**/transcript.jsonl", gemini_dir.display(), dir);
        let base = gemini_dir.join(dir);
        let files = cursor.glob_cached(&pattern, &base);
        for file in files {
            let key = file.to_string_lossy().to_string();
            if !cursor.file_changed(&key) {
                continue;
            }
            let offset = cursor.get_offset(&key);
            let bucket = file_mtime_bucket(&file);
            let (mut records, new_offset) =
                parse_jsonl_file(&file, offset, "antigravity", |v, src| {
                    parse_transcript_line(v, src)
                });
            for r in &mut records {
                if r.hour_start.is_empty() {
                    r.hour_start = bucket.clone();
                }
            }
            all_records.extend(records);
            cursor.set_offset(&key, new_offset);
        }
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}

fn parse_transcript_line(v: &Value, source: &str) -> Option<UsageRecord> {
    let usage = v.get("usageMetadata").or_else(|| v.get("usage"))?;
    let model = v
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("gemini")
        .to_string();
    let input = usage
        .get("promptTokenCount")
        .or(usage.get("input_tokens"))
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let output = usage
        .get("candidatesTokenCount")
        .or(usage.get("output_tokens"))
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let cached = usage
        .get("cachedContentTokenCount")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let total = usage
        .get("totalTokenCount")
        .and_then(|x| x.as_u64())
        .unwrap_or(input + output);
    if total == 0 {
        return None;
    }
    Some(UsageRecord {
        id: None,
        hour_start: String::new(),
        source: source.to_string(),
        model,
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: cached,
        cache_creation_input_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: total,
        conversation_count: 1,
    })
}
