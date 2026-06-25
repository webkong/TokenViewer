use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;

use super::utils::*;
use crate::models::UsageRecord;

pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    let db_path = home_dir.join("Library/Application Support/Zed/threads/threads.db");
    #[cfg(target_os = "linux")]
    let db_path = home_dir.join(".local/share/zed/threads/threads.db");
    #[cfg(target_os = "windows")]
    let db_path = home_dir.join("AppData/Roaming/Zed/threads/threads.db");

    if !db_path.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let mut cursor = FileCursor::from_json(cursor_data);
    let db_key = db_path.to_string_lossy().to_string();
    if !cursor.file_changed(&db_key) {
        return Ok((vec![], cursor.to_json()));
    }
    let last_ts = cursor.last_timestamp.clone().unwrap_or_default();
    let conn = Connection::open(&db_path)?;

    let mut stmt = conn.prepare("SELECT id, updated_at, data_type, data FROM threads")?;

    let mut records = Vec::new();
    let mut max_ts = last_ts.clone();

    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let updated_at: String = row.get(1)?;
        let _data_type: Option<String> = row.get(2)?;
        let blob: Vec<u8> = row.get(3)?;

        // Filter by cursor timestamp
        if !last_ts.is_empty() && updated_at <= last_ts {
            continue;
        }

        // Try to parse blob as UTF-8 JSON; skip if not parseable (e.g. zstd compressed)
        let json_str = match std::str::from_utf8(&blob) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let data: Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Only count zed.dev provider
        let provider = data
            .pointer("/model/provider")
            .and_then(|p| p.as_str())
            .unwrap_or("");
        if provider != "zed.dev" {
            continue;
        }

        let usage = match data.get("cumulative_token_usage") {
            Some(u) => u,
            None => continue,
        };

        let input = usage
            .get("input_tokens")
            .and_then(|x| x.as_u64())
            .unwrap_or(0);
        let output = usage
            .get("output_tokens")
            .and_then(|x| x.as_u64())
            .unwrap_or(0);
        let cache_read = usage
            .get("cache_read_input_tokens")
            .and_then(|x| x.as_u64())
            .unwrap_or(0);
        let cache_write = usage
            .get("cache_creation_input_tokens")
            .and_then(|x| x.as_u64())
            .unwrap_or(0);

        let cur = [input, output, cache_read, cache_write, 0];
        let delta = cursor.delta(&id, cur);
        let [d_in, d_out, d_cr, d_cw, _] = delta;

        let total = d_in + d_out + d_cr + d_cw;
        if total == 0 {
            continue;
        }

        let model = data
            .pointer("/model/model")
            .and_then(|m| m.as_str())
            .unwrap_or("zed-unknown")
            .to_string();

        let hour_start = iso_to_bucket(&updated_at);

        records.push(UsageRecord {
            id: None,
            hour_start,
            source: "zed".to_string(),
            model,
            input_tokens: d_in,
            output_tokens: d_out,
            cached_input_tokens: d_cr,
            cache_creation_input_tokens: d_cw,
            reasoning_output_tokens: 0,
            total_tokens: total,
            conversation_count: 1,
        });

        if updated_at > max_ts {
            max_ts = updated_at;
        }
    }

    cursor.last_timestamp = Some(max_ts);
    Ok((aggregate_records(records), cursor.to_json()))
}
