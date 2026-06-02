use std::io::{BufRead, BufReader};
use std::fs::File;
use std::path::Path;
use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let base = home_dir.join(".craft-agent/workspaces");
    if !base.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let pattern = format!("{}/**/sessions/**/session.jsonl", base.display());
    let files = glob_files(&pattern);
    let mut cursor = FileCursor::from_json(cursor_data);
    let mut all_records = Vec::new();

    for file in files {
        let key = file.to_string_lossy().to_string();

        // Read only the first line (header)
        let f = match File::open(&file) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let mut reader = BufReader::new(f);
        let mut first_line = String::new();
        if reader.read_line(&mut first_line).unwrap_or(0) == 0 {
            continue;
        }
        let header: Value = match serde_json::from_str(first_line.trim()) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let usage = match header.get("tokenUsage") {
            Some(u) => u,
            None => continue,
        };

        let input = usage.get("inputTokens").and_then(|x| x.as_u64()).unwrap_or(0);
        let output = usage.get("outputTokens").and_then(|x| x.as_u64()).unwrap_or(0);
        let cached = usage.get("cacheReadTokens").and_then(|x| x.as_u64()).unwrap_or(0);
        let cache_creation = usage.get("cacheCreationTokens").and_then(|x| x.as_u64()).unwrap_or(0);
        // Delta-based: cumulative totals
        let cur = [input, output, cached, cache_creation, 0];
        let delta = cursor.delta(&key, cur);
        let [d_in, d_out, d_cr, d_cw, _] = delta;
        let d_total = d_in + d_out + d_cr + d_cw;
        if d_total == 0 { continue; }

        let model = header.get("model").and_then(|m| m.as_str()).unwrap_or("craft-unknown").to_string();

        let bucket = header.get("lastMessageAt").and_then(|t| t.as_i64()).and_then(epoch_millis_to_bucket)
            .or_else(|| header.get("lastUsedAt").and_then(|t| t.as_i64()).and_then(epoch_millis_to_bucket))
            .or_else(|| header.get("createdAt").and_then(|t| t.as_i64()).and_then(epoch_millis_to_bucket))
            .unwrap_or_else(|| file_mtime_bucket(&file));

        all_records.push(UsageRecord {
            id: None,
            hour_start: bucket,
            source: "craft".to_string(),
            model,
            input_tokens: d_in,
            output_tokens: d_out,
            cached_input_tokens: d_cr,
            cache_creation_input_tokens: d_cw,
            reasoning_output_tokens: 0,
            total_tokens: d_total,
            conversation_count: 1,
        });
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}
