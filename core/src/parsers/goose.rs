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
    let db_path = home_dir.join("Library/Application Support/goose/sessions/sessions.db");
    #[cfg(target_os = "linux")]
    let db_path = home_dir.join(".local/share/goose/sessions/sessions.db");
    #[cfg(target_os = "windows")]
    let db_path = home_dir.join("AppData/Roaming/goose/sessions/sessions.db");

    if !db_path.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let mut cursor = FileCursor::from_json(cursor_data);
    let db_key = db_path.to_string_lossy().to_string();
    if !cursor.file_changed(&db_key) {
        return Ok((vec![], cursor.to_json()));
    }
    let conn = Connection::open(&db_path)?;

    let mut stmt = conn.prepare(
        "SELECT id, model_config_json, created_at, total_tokens, input_tokens, output_tokens, \
         accumulated_total_tokens, accumulated_input_tokens, accumulated_output_tokens \
         FROM sessions WHERE model_config_json IS NOT NULL",
    )?;

    let mut records = Vec::new();

    let rows = stmt.query_map([], |row| {
        let id: String = row.get(0)?;
        let model_cfg: Option<String> = row.get(1)?;
        let created_at: String = row.get(2)?;
        // Dynamic columns may be missing; fallback to 0
        let total: i64 = row.get::<_, i64>(3).unwrap_or(0);
        let input: i64 = row.get::<_, i64>(4).unwrap_or(0);
        let output: i64 = row.get::<_, i64>(5).unwrap_or(0);
        let acc_total: i64 = row.get::<_, i64>(6).unwrap_or(0);
        let acc_input: i64 = row.get::<_, i64>(7).unwrap_or(0);
        let acc_output: i64 = row.get::<_, i64>(8).unwrap_or(0);
        Ok((
            id, model_cfg, created_at, total, input, output, acc_total, acc_input, acc_output,
        ))
    })?;

    for row in rows.flatten() {
        let (id, model_cfg, created_at, total, input, output, acc_total, acc_input, acc_output) =
            row;

        // Prefer accumulated_* columns, fallback to single-turn columns
        let (use_input, use_output, use_total) = if acc_input > 0 || acc_output > 0 || acc_total > 0
        {
            (acc_input, acc_output, acc_total)
        } else {
            (input, output, total)
        };

        let reasoning = std::cmp::max(0, use_total - use_input - use_output) as u64;
        let cur = [use_input as u64, use_output as u64, 0, 0, reasoning];
        let delta = cursor.delta(&id, cur);
        let [d_in, d_out, _, _, d_reason] = delta;

        let d_total = d_in + d_out + d_reason;
        if d_total == 0 {
            continue;
        }

        // Parse model from model_config_json
        let model = model_cfg
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .and_then(|v| {
                v.get("model_name")
                    .and_then(|m| m.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "goose".to_string());

        let hour_start = iso_to_bucket(&created_at);

        records.push(UsageRecord {
            id: None,
            hour_start,
            source: "goose".to_string(),
            model,
            input_tokens: d_in,
            output_tokens: d_out,
            cached_input_tokens: 0,
            cache_creation_input_tokens: 0,
            reasoning_output_tokens: d_reason,
            total_tokens: d_total,
            conversation_count: 1,
        });
    }

    Ok((aggregate_records(records), cursor.to_json()))
}
