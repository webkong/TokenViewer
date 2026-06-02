use std::path::Path;
use rusqlite::Connection;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let db_path = home_dir.join(".hermes/state.db");
    if !db_path.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let mut cursor = FileCursor::from_json(cursor_data);
    let conn = Connection::open(&db_path)?;

    let mut stmt = conn.prepare(
        "SELECT id, model, started_at, ended_at, input_tokens, output_tokens, \
         cache_read_tokens, cache_write_tokens, reasoning_tokens \
         FROM sessions \
         WHERE (input_tokens > 0 OR output_tokens > 0 OR cache_read_tokens > 0 OR reasoning_tokens > 0) \
         ORDER BY started_at ASC"
    )?;

    let mut records = Vec::new();

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, i64>(8)?,
        ))
    })?;

    for row in rows.flatten() {
        let (id, model, started_at, ended_at, input, output, cache_read, cache_write, reasoning) = row;

        let cur = [input as u64, output as u64, cache_read as u64, cache_write as u64, reasoning as u64];
        let delta = cursor.delta(&id, cur);
        let [d_in, d_out, d_cr, d_cw, d_reason] = delta;

        let total = d_in + d_out + d_cr + d_cw + d_reason;
        if total == 0 {
            continue;
        }

        let ts = ended_at.unwrap_or(started_at);
        let hour_start = epoch_secs_to_bucket(ts).unwrap_or_else(now_bucket);
        let model_name = model.unwrap_or_else(|| "hermes-agent".to_string());

        records.push(UsageRecord {
            id: None,
            hour_start,
            source: "hermes".to_string(),
            model: model_name,
            input_tokens: d_in,
            output_tokens: d_out,
            cached_input_tokens: d_cr,
            cache_creation_input_tokens: d_cw,
            reasoning_output_tokens: d_reason,
            total_tokens: total,
            conversation_count: 1,
        });
    }

    Ok((aggregate_records(records), cursor.to_json()))
}
