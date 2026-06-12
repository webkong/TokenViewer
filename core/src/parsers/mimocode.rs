use std::path::Path;
use rusqlite::Connection;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let mut all_records = Vec::new();
    let mut cursor = FileCursor::from_json(cursor_data);

    let db_path = home_dir.join(".local/share/mimocode/mimocode.db");
    if !db_path.exists() {
        return Ok((vec![], cursor.to_json()));
    }

    let db_key = db_path.to_string_lossy().to_string();
    if !cursor.file_changed(&db_key) {
        return Ok((vec![], cursor.to_json()));
    }

    let conn = Connection::open(&db_path)?;

    // last_timestamp stores the max time_created (epoch ms) we've seen
    let last_ms: i64 = cursor.last_timestamp
        .as_deref()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let sql = "SELECT id, time_created, data FROM message \
               WHERE time_created > ?1 \
               ORDER BY time_created ASC";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([last_ms], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    let mut max_ms = last_ms;

    for row in rows.flatten() {
        let (_id, time_created, data_str) = row;
        if time_created > max_ms {
            max_ms = time_created;
        }

        let v: serde_json::Value = match serde_json::from_str(&data_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Only assistant messages carry usage
        if v.get("role").and_then(|r| r.as_str()) != Some("assistant") {
            continue;
        }

        // Skip synthetic/system messages
        let model_id = v.get("modelID").and_then(|m| m.as_str()).unwrap_or("");
        if model_id.is_empty() || model_id == "<synthetic>" {
            continue;
        }

        let tokens = match v.get("tokens") {
            Some(t) => t,
            None => continue,
        };

        let input = tokens.get("input").and_then(|x| x.as_u64()).unwrap_or(0);
        let output = tokens.get("output").and_then(|x| x.as_u64()).unwrap_or(0);
        let reasoning = tokens.get("reasoning").and_then(|x| x.as_u64()).unwrap_or(0);
        let cache_read = tokens.get("cache_read").and_then(|x| x.as_u64()).unwrap_or(0);
        let cache_write = tokens.get("cache_write").and_then(|x| x.as_u64()).unwrap_or(0);
        let total = input + output + reasoning + cache_read + cache_write;

        if total == 0 {
            continue;
        }

        // time_created is epoch milliseconds
        let hour_start = epoch_millis_to_bucket(time_created).unwrap_or_default();
        if hour_start.is_empty() {
            continue;
        }

        all_records.push(UsageRecord {
            id: None,
            hour_start,
            source: "mimocode".to_string(),
            model: model_id.to_string(),
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: cache_read,
            cache_creation_input_tokens: cache_write,
            reasoning_output_tokens: reasoning,
            total_tokens: total,
            conversation_count: 0,
        });
    }

    cursor.last_timestamp = Some(max_ms.to_string());
    Ok((aggregate_records(all_records), cursor.to_json()))
}
