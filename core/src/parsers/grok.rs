use std::path::Path;
use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let grok_home = std::env::var("TOKENTRACKER_GROK_HOME")
        .or_else(|_| std::env::var("GROK_HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| home_dir.join(".grok"));

    let sessions_dir = grok_home.join("sessions");
    if !sessions_dir.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let mut cursor = FileCursor::from_json(cursor_data);
    let mut all_records = Vec::new();

    let pattern = format!("{}/**/updates.jsonl", sessions_dir.display());
    let files = glob_files(&pattern);

    for file in files {
        let key = file.to_string_lossy().to_string();
        let offset = cursor.get_offset(&key);
        let (lines, new_offset) = match read_lines_from_offset(&file, offset) {
            Ok(r) => r,
            Err(_) => continue,
        };

        let bucket = file_mtime_bucket(&file);

        for line in &lines {
            let v: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Extract totalTokens from various nested locations
            let total_tokens = v.get("params")
                .and_then(|p| p.get("_meta"))
                .and_then(|m| m.get("totalTokens"))
                .and_then(|x| x.as_u64())
                .or_else(|| v.get("_meta").and_then(|m| m.get("totalTokens")).and_then(|x| x.as_u64()))
                .or_else(|| v.get("totalTokens").and_then(|x| x.as_u64()));

            let total_tokens = match total_tokens {
                Some(t) if t > 0 => t,
                _ => continue,
            };

            // Cumulative high-watermark delta
            let cur = [total_tokens, 0, 0, 0, 0];
            let d = cursor.delta(&key, cur);
            let delta_total = d[0];
            if delta_total == 0 {
                continue;
            }

            // Split: 80% input, 20% output
            let input = ((delta_total as f64) * 0.8).round() as u64;
            let output = delta_total - input;

            // Extract timestamp
            let meta = v.get("params").and_then(|p| p.get("_meta"))
                .or_else(|| v.get("_meta"));

            let hour_start = meta
                .and_then(|m| m.get("agentTimestampMs").or(m.get("timestampMs")))
                .and_then(|x| x.as_i64())
                .and_then(epoch_millis_to_bucket)
                .or_else(|| v.get("timestamp_ms").and_then(|x| x.as_i64()).and_then(epoch_millis_to_bucket))
                .or_else(|| v.get("timestamp").and_then(|t| t.as_str()).map(|s| iso_to_bucket(s)))
                .unwrap_or_else(|| bucket.clone());

            all_records.push(UsageRecord {
                id: None,
                hour_start,
                source: "grok".to_string(),
                model: "grok-build".to_string(),
                input_tokens: input,
                output_tokens: output,
                cached_input_tokens: 0,
                cache_creation_input_tokens: 0,
                reasoning_output_tokens: 0,
                total_tokens: delta_total,
                conversation_count: 1,
            });
        }

        cursor.set_offset(&key, new_offset);
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}
