use std::path::Path;
use rusqlite::Connection;
use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let db_path = home_dir.join(".local/share/opencode/opencode.db");
    parse_opencode_db(&db_path, cursor_data, "opencode")
}

/// Shared parser for OpenCode-schema SQLite databases (used by kilocli too).
pub fn parse_opencode_db(db_path: &Path, cursor_data: Option<&str>, source: &str) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    if !db_path.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let mut cursor = FileCursor::from_json(cursor_data);
    let db_key = db_path.to_string_lossy().to_string();
    if !cursor.file_changed(&db_key) {
        return Ok((vec![], cursor.to_json()));
    }
    let conn = Connection::open(db_path)?;

    let mut stmt = conn.prepare(
        "SELECT id, data FROM message WHERE json_extract(data, '$.role') = 'assistant'"
    )?;

    let mut records = Vec::new();

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
        ))
    })?;

    for row in rows.flatten() {
        let (id, data_str) = row;
        if !cursor.mark_seen(&id) {
            continue;
        }

        let data: Value = match serde_json::from_str(&data_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let tokens = match data.get("tokens") {
            Some(t) => t,
            None => continue,
        };

        let input = tokens.get("input").and_then(|x| x.as_u64()).unwrap_or(0);
        let output = tokens.get("output").and_then(|x| x.as_u64()).unwrap_or(0);
        let reasoning = tokens.get("reasoning").and_then(|x| x.as_u64()).unwrap_or(0);
        let cache_read = tokens.pointer("/cache/read").and_then(|x| x.as_u64()).unwrap_or(0);
        let cache_write = tokens.pointer("/cache/write").and_then(|x| x.as_u64()).unwrap_or(0);

        let total = input + output + reasoning + cache_read + cache_write;
        if total == 0 {
            continue;
        }

        let model = data.get("modelID").and_then(|m| m.as_str())
            .or_else(|| data.get("model").and_then(|m| m.as_str()))
            .unwrap_or("unknown")
            .to_string();

        // timestamp: time.completed or time.created (epoch ms)
        let ts_ms = data.pointer("/time/completed").and_then(|x| x.as_i64())
            .or_else(|| data.pointer("/time/created").and_then(|x| x.as_i64()));

        let hour_start = ts_ms
            .and_then(epoch_millis_to_bucket)
            .unwrap_or_else(now_bucket);

        records.push(UsageRecord {
            id: None,
            hour_start,
            source: source.to_string(),
            model,
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: cache_read,
            cache_creation_input_tokens: cache_write,
            reasoning_output_tokens: reasoning,
            total_tokens: total,
            conversation_count: 1,
        });
    }

    Ok((aggregate_records(records), cursor.to_json()))
}
