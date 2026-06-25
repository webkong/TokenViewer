use rusqlite::Connection;
use std::path::Path;

use super::utils::*;
use crate::models::UsageRecord;

/// ZCode (智谱 / Z.ai coding agent) usage parser.
///
/// Reads `~/.zcode/cli/db/db.sqlite`, table `model_usage` — one row per model
/// request with a full token breakdown (input / output / reasoning / cache
/// creation / cache read) and an epoch-ms `started_at`.
///
/// Incremental strategy:
///   - `cursor.file_changed()` skips the DB when its mtime is unchanged.
///   - `cursor.zcode_last_started_at` + `cursor.zcode_last_id` store a stable
///     `(started_at, id)` watermark so same-ms rows can be replayed safely.
///   - `cursor.mark_seen()` dedups by `model_usage.id` (text PK) to guard
///     against boundary races when the same row is revisited after completion.
///
/// Token semantics: zcode's `input_tokens` already EXCLUDES cache-read hits
/// (it is the non-cached prompt portion), so `total_tokens` is computed as
/// input + output + reasoning + cache_creation — cache_read is informational
/// only and must not be added to the total (matches mimocode's convention of
/// not double-counting cumulative cached context).
///
/// Rows with zero billable tokens (e.g. requests that hit rate-limit before
/// generating anything, `status = 'error'` with no usage) and rows still
/// `status = 'running'` (incomplete) are skipped.
pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let mut all_records = Vec::new();
    let mut cursor = FileCursor::from_json(cursor_data);

    let db_path = home_dir.join(".zcode/cli/db/db.sqlite");
    if !db_path.exists() {
        return Ok((vec![], cursor.to_json()));
    }

    let db_key = db_path.to_string_lossy().to_string();
    if !cursor.file_changed(&db_key) {
        return Ok((vec![], cursor.to_json()));
    }

    // Watermark: max (started_at, id) pair we have already counted.
    let legacy_last_ts: i64 = cursor
        .last_timestamp
        .as_deref()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut last_ts = cursor.zcode_last_started_at.max(legacy_last_ts);
    let mut last_id = cursor.zcode_last_id.clone().unwrap_or_default();

    let conn = Connection::open(&db_path)?;
    // rusqlite reads the WAL automatically on open, so -wal/-shm are handled.

    let sql = "SELECT id, started_at, model_id, input_tokens, output_tokens, \
               reasoning_tokens, cache_creation_input_tokens, cache_read_input_tokens, status \
               FROM model_usage \
               WHERE started_at > ?1 OR (started_at = ?1 AND id > ?2) \
               ORDER BY started_at ASC, id ASC";

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map((last_ts, last_id.as_str()), |row| {
        Ok((
            row.get::<_, String>(0)?, // id
            row.get::<_, i64>(1)?,    // started_at (epoch ms)
            row.get::<_, String>(2)?, // model_id
            row.get::<_, i64>(3)?,    // input_tokens
            row.get::<_, i64>(4)?,    // output_tokens
            row.get::<_, i64>(5)?,    // reasoning_tokens
            row.get::<_, i64>(6)?,    // cache_creation_input_tokens
            row.get::<_, i64>(7)?,    // cache_read_input_tokens
            row.get::<_, String>(8)?, // status
        ))
    })?;

    let mut blocked_by_running = false;

    for row in rows.flatten() {
        let (
            id,
            started_at,
            model_id,
            input,
            output,
            reasoning,
            cache_creation,
            cache_read,
            status,
        ) = row;

        // Skip incomplete requests, but keep the cursor pinned before them so
        // the row can be replayed after it transitions to completed.
        if status == "running" {
            blocked_by_running = true;
            continue;
        }

        // Dedup by the text PK — protects against same-ms boundary races.
        if !cursor.mark_seen(&id) {
            continue;
        }

        if !blocked_by_running {
            last_ts = started_at;
            last_id = id.clone();
        }

        let input = input.max(0) as u64;
        let output = output.max(0) as u64;
        let reasoning = reasoning.max(0) as u64;
        let cache_creation = cache_creation.max(0) as u64;
        let cache_read = cache_read.max(0) as u64;

        // Skip rows with no billable tokens (rate-limited before generation).
        let total = input + output + reasoning + cache_creation;
        if total == 0 {
            continue;
        }

        // started_at is epoch milliseconds → 30-min UTC bucket.
        let hour_start = match epoch_millis_to_bucket(started_at) {
            Some(b) => b,
            None => continue,
        };

        let model = if model_id.is_empty() {
            "zcode-agent".to_string()
        } else {
            model_id
        };

        all_records.push(UsageRecord {
            id: None,
            hour_start,
            source: "zcode".to_string(),
            model,
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: cache_read,
            cache_creation_input_tokens: cache_creation,
            reasoning_output_tokens: reasoning,
            total_tokens: total,
            conversation_count: 0,
        });
    }

    cursor.zcode_last_started_at = last_ts;
    cursor.zcode_last_id = if last_id.is_empty() {
        None
    } else {
        Some(last_id.clone())
    };
    cursor.last_timestamp = Some(last_ts.to_string());
    Ok((aggregate_records(all_records), cursor.to_json()))
}
