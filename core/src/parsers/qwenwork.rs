use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::path::Path;

use super::utils::*;
use crate::models::UsageRecord;

/// QwenWork's Flash harness persists OpenCode-style messages. The same usage
/// also appears in parts and session totals; only completed messages are read.
pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let db_path = home_dir.join(".qwenwork/.harness/flash-cli/qwenwork/qwenwork.db");
    if !db_path.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let mut cursor = FileCursor::from_json(cursor_data);
    // Read the live WAL too. Database mtime alone does not track WAL writes.
    let conn = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut stmt = conn.prepare(
        "SELECT id, data FROM message
         WHERE json_extract(data, '$.role') = 'assistant'
           AND json_extract(data, '$.time.completed') IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut records = Vec::new();
    for row in rows {
        let (id, raw) = row?;
        let data: Value = serde_json::from_str(&raw)?;
        let Some(tokens) = data.get("tokens") else {
            continue;
        };
        let Some(hour_start) = data
            .pointer("/time/completed")
            .and_then(Value::as_i64)
            .and_then(epoch_millis_to_bucket)
        else {
            continue;
        };
        let count = |path| tokens.pointer(path).and_then(Value::as_u64).unwrap_or(0);
        let totals = [
            count("/input"),
            count("/output"),
            count("/cache/read"),
            count("/cache/write"),
            count("/reasoning"),
        ];
        if totals.iter().all(|value| *value == 0) {
            continue;
        }
        // Snapshots keep repeated syncs idempotent even for >50k messages,
        // while allowing finalized usage to grow after the first observation.
        let first_seen = !cursor.snapshots.contains_key(&id);
        let [input, output, cache_read, cache_write, reasoning] = cursor.delta(&id, totals);
        let total = input + output + cache_read + cache_write + reasoning;
        if total == 0 {
            continue;
        }
        let model = data
            .get("modelID")
            .and_then(Value::as_str)
            .filter(|model| !model.is_empty())
            .unwrap_or("qwenwork-unknown")
            .to_string();
        records.push(UsageRecord {
            id: None,
            hour_start,
            source: "qwenwork".to_string(),
            model,
            project_key: String::new(),
            project_ref: String::new(),
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: cache_read,
            cache_creation_input_tokens: cache_write,
            reasoning_output_tokens: reasoning,
            total_tokens: total,
            conversation_count: u32::from(first_seen),
        });
    }
    Ok((aggregate_records(records), cursor.to_json()))
}
