use rusqlite::{params, Connection, Result as SqlResult};
use std::path::Path;

use crate::models::{DailyUsage, HeatmapPoint, ModelBreakdownEntry, SyncCursor, UsageRecord, UsageSummary};

const SCHEMA_VERSION: i32 = 1;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> SqlResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> SqlResult<()> {
        let version: i32 = self.conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap_or(0);

        if version < 1 {
            self.conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS usage (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    hour_start TEXT NOT NULL,
                    source TEXT NOT NULL,
                    model TEXT NOT NULL,
                    input_tokens INTEGER DEFAULT 0,
                    output_tokens INTEGER DEFAULT 0,
                    cached_input_tokens INTEGER DEFAULT 0,
                    cache_creation_input_tokens INTEGER DEFAULT 0,
                    reasoning_output_tokens INTEGER DEFAULT 0,
                    total_tokens INTEGER DEFAULT 0,
                    conversation_count INTEGER DEFAULT 1,
                    created_at TEXT DEFAULT (datetime('now')),
                    UNIQUE(source, model, hour_start)
                );
                CREATE INDEX IF NOT EXISTS idx_usage_hour ON usage(hour_start);
                CREATE INDEX IF NOT EXISTS idx_usage_source ON usage(source);

                CREATE TABLE IF NOT EXISTS sync_cursors (
                    source TEXT PRIMARY KEY,
                    cursor_data TEXT NOT NULL,
                    updated_at TEXT DEFAULT (datetime('now'))
                );

                CREATE TABLE IF NOT EXISTS settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );"
            )?;
        }

        self.conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        Ok(())
    }

    // --- Usage CRUD ---

    pub fn upsert_usage(&self, record: &UsageRecord) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO usage (hour_start, source, model, input_tokens, output_tokens,
                cached_input_tokens, cache_creation_input_tokens, reasoning_output_tokens,
                total_tokens, conversation_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(source, model, hour_start) DO UPDATE SET
                input_tokens = usage.input_tokens + excluded.input_tokens,
                output_tokens = usage.output_tokens + excluded.output_tokens,
                cached_input_tokens = usage.cached_input_tokens + excluded.cached_input_tokens,
                cache_creation_input_tokens = usage.cache_creation_input_tokens + excluded.cache_creation_input_tokens,
                reasoning_output_tokens = usage.reasoning_output_tokens + excluded.reasoning_output_tokens,
                total_tokens = usage.total_tokens + excluded.total_tokens,
                conversation_count = usage.conversation_count + excluded.conversation_count",
            params![
                record.hour_start, record.source, record.model,
                record.input_tokens, record.output_tokens,
                record.cached_input_tokens, record.cache_creation_input_tokens,
                record.reasoning_output_tokens, record.total_tokens,
                record.conversation_count,
            ],
        )?;
        Ok(())
    }

    // --- Queries ---

    /// Upsert a batch of records inside a single transaction.
    /// All-or-nothing: rolls back on any error.
    pub fn upsert_usage_batch(&self, records: &[UsageRecord]) -> SqlResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        for record in records {
            self.upsert_usage(record)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Clear processed usage data and sync cursors so the next sync replays
    /// the original raw files from scratch.
    pub fn clear_processed_data(&self) -> SqlResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute_batch("DELETE FROM usage; DELETE FROM sync_cursors;")?;
        tx.commit()?;
        Ok(())
    }

    pub fn query_summary(&self, from: &str, to: &str) -> SqlResult<UsageSummary> {
        let mut stmt = self.conn.prepare(
            "SELECT
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cached_input_tokens), 0),
                COALESCE(SUM(reasoning_output_tokens), 0),
                COALESCE(SUM(conversation_count), 0),
                COUNT(DISTINCT substr(hour_start, 1, 10))
             FROM usage WHERE hour_start >= ?1 AND hour_start < ?2"
        )?;

        stmt.query_row(params![from, to], |row| {
            Ok(UsageSummary {
                total_tokens: row.get::<_, i64>(0)? as u64,
                input_tokens: row.get::<_, i64>(1)? as u64,
                output_tokens: row.get::<_, i64>(2)? as u64,
                cached_input_tokens: row.get::<_, i64>(3)? as u64,
                reasoning_output_tokens: row.get::<_, i64>(4)? as u64,
                conversation_count: row.get::<_, i32>(5)? as u32,
                active_days: row.get::<_, i32>(6)? as u32,
                total_cost_usd: 0.0, // computed by caller with pricing
            })
        })
    }

    pub fn query_daily(&self, from: &str, to: &str) -> SqlResult<Vec<DailyUsage>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                substr(hour_start, 1, 10) as date,
                SUM(total_tokens), SUM(input_tokens), SUM(output_tokens),
                SUM(cached_input_tokens), SUM(cache_creation_input_tokens),
                SUM(reasoning_output_tokens), SUM(conversation_count)
             FROM usage
             WHERE hour_start >= ?1 AND hour_start < ?2
             GROUP BY date ORDER BY date"
        )?;

        let rows = stmt.query_map(params![from, to], |row| {
            Ok(DailyUsage {
                date: row.get(0)?,
                total_tokens: row.get::<_, i64>(1)? as u64,
                input_tokens: row.get::<_, i64>(2)? as u64,
                output_tokens: row.get::<_, i64>(3)? as u64,
                cached_input_tokens: row.get::<_, i64>(4)? as u64,
                cache_creation_input_tokens: row.get::<_, i64>(5)? as u64,
                reasoning_output_tokens: row.get::<_, i64>(6)? as u64,
                conversation_count: row.get::<_, i32>(7)? as u32,
                total_cost_usd: 0.0,
            })
        })?;

        rows.collect()
    }

    /// Hourly breakdown (for single-day view). Groups by hour (YYYY-MM-DDTHH).
    pub fn query_hourly(&self, from: &str, to: &str) -> SqlResult<Vec<DailyUsage>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                substr(hour_start, 1, 13) as hour,
                SUM(total_tokens), SUM(input_tokens), SUM(output_tokens),
                SUM(cached_input_tokens), SUM(cache_creation_input_tokens),
                SUM(reasoning_output_tokens), SUM(conversation_count)
             FROM usage
             WHERE hour_start >= ?1 AND hour_start < ?2
             GROUP BY hour ORDER BY hour"
        )?;

        let rows = stmt.query_map(params![from, to], |row| {
            Ok(DailyUsage {
                date: row.get(0)?,
                total_tokens: row.get::<_, i64>(1)? as u64,
                input_tokens: row.get::<_, i64>(2)? as u64,
                output_tokens: row.get::<_, i64>(3)? as u64,
                cached_input_tokens: row.get::<_, i64>(4)? as u64,
                cache_creation_input_tokens: row.get::<_, i64>(5)? as u64,
                reasoning_output_tokens: row.get::<_, i64>(6)? as u64,
                conversation_count: row.get::<_, i32>(7)? as u32,
                total_cost_usd: 0.0,
            })
        })?;

        rows.collect()
    }

    pub fn query_model_breakdown(&self, from: &str, to: &str) -> SqlResult<Vec<ModelBreakdownEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT model, source, SUM(total_tokens)
             FROM usage
             WHERE hour_start >= ?1 AND hour_start < ?2
             GROUP BY model, source
             ORDER BY SUM(total_tokens) DESC"
        )?;

        let rows = stmt.query_map(params![from, to], |row| {
            Ok(ModelBreakdownEntry {
                model: row.get(0)?,
                source: row.get(1)?,
                total_tokens: row.get::<_, i64>(2)? as u64,
                total_cost_usd: 0.0,
                percentage: 0.0,
            })
        })?;

        let mut entries: Vec<_> = rows.collect::<SqlResult<Vec<_>>>()?;
        let grand_total: u64 = entries.iter().map(|e| e.total_tokens).sum();
        if grand_total > 0 {
            for e in &mut entries {
                e.percentage = e.total_tokens as f64 / grand_total as f64 * 100.0;
            }
        }
        Ok(entries)
    }

    /// Aggregate full token columns grouped by (source, model) for a range.
    /// hour_start is left empty. Used by the cost layer.
    pub fn aggregate_by_model(&self, from: &str, to: &str) -> SqlResult<Vec<UsageRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT source, model,
                SUM(input_tokens), SUM(output_tokens), SUM(cached_input_tokens),
                SUM(cache_creation_input_tokens), SUM(reasoning_output_tokens),
                SUM(total_tokens), SUM(conversation_count)
             FROM usage WHERE hour_start >= ?1 AND hour_start < ?2
             GROUP BY source, model ORDER BY SUM(total_tokens) DESC"
        )?;
        let rows = stmt.query_map(params![from, to], |row| Ok(UsageRecord {
            id: None,
            hour_start: String::new(),
            source: row.get(0)?,
            model: row.get(1)?,
            input_tokens: row.get::<_, i64>(2)? as u64,
            output_tokens: row.get::<_, i64>(3)? as u64,
            cached_input_tokens: row.get::<_, i64>(4)? as u64,
            cache_creation_input_tokens: row.get::<_, i64>(5)? as u64,
            reasoning_output_tokens: row.get::<_, i64>(6)? as u64,
            total_tokens: row.get::<_, i64>(7)? as u64,
            conversation_count: row.get::<_, i32>(8)? as u32,
        }))?;
        rows.collect()
    }

    /// Aggregate full token columns grouped by (date, source, model).
    /// hour_start holds the YYYY-MM-DD date. Used by the daily cost layer.
    pub fn aggregate_by_day_model(&self, from: &str, to: &str) -> SqlResult<Vec<UsageRecord>> {
        self.aggregate_by_bucket_model(from, to, 10)
    }

    /// Aggregate full token columns grouped by (hour, source, model).
    pub fn aggregate_by_hour_model(&self, from: &str, to: &str) -> SqlResult<Vec<UsageRecord>> {
        self.aggregate_by_bucket_model(from, to, 13)
    }

    fn aggregate_by_bucket_model(&self, from: &str, to: &str, len: usize) -> SqlResult<Vec<UsageRecord>> {
        // hour_start is stored in UTC; group by LOCAL-time bucket so day/hour
        // dimensions match the user's wall clock. len 10 = day, 13 = hour.
        let fmt = if len <= 10 { "%Y-%m-%d" } else { "%Y-%m-%dT%H" };
        let sql = format!(
            "SELECT strftime('{fmt}', hour_start, 'localtime') as bucket, source, model,
                SUM(input_tokens), SUM(output_tokens), SUM(cached_input_tokens),
                SUM(cache_creation_input_tokens), SUM(reasoning_output_tokens),
                SUM(total_tokens), SUM(conversation_count)
             FROM usage WHERE hour_start >= ?1 AND hour_start < ?2
             GROUP BY bucket, source, model ORDER BY bucket"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![from, to], |row| Ok(UsageRecord {
            id: None,
            hour_start: row.get(0)?,
            source: row.get(1)?,
            model: row.get(2)?,
            input_tokens: row.get::<_, i64>(3)? as u64,
            output_tokens: row.get::<_, i64>(4)? as u64,
            cached_input_tokens: row.get::<_, i64>(5)? as u64,
            cache_creation_input_tokens: row.get::<_, i64>(6)? as u64,
            reasoning_output_tokens: row.get::<_, i64>(7)? as u64,
            total_tokens: row.get::<_, i64>(8)? as u64,
            conversation_count: row.get::<_, i32>(9)? as u32,
        }))?;
        rows.collect()
    }

    pub fn query_heatmap(&self, weeks: i32) -> SqlResult<Vec<HeatmapPoint>> {
        let days = weeks * 7;
        let mut stmt = self.conn.prepare(
            "SELECT strftime('%Y-%m-%d', hour_start, 'localtime') as date, SUM(total_tokens)
             FROM usage
             WHERE hour_start >= date('now', ?1)
             GROUP BY date ORDER BY date"
        )?;

        let offset = format!("-{days} days");
        let rows = stmt.query_map(params![offset], |row| {
            let count: i64 = row.get(1)?;
            Ok(HeatmapPoint {
                date: row.get(0)?,
                count: count as u64,
                level: 0, // computed below
            })
        })?;

        let mut points: Vec<_> = rows.collect::<SqlResult<Vec<_>>>()?;
        if !points.is_empty() {
            let max_count = points.iter().map(|p| p.count).max().unwrap_or(1).max(1);
            for p in &mut points {
                p.level = match (p.count as f64 / max_count as f64 * 4.0).ceil() as u8 {
                    0 => if p.count > 0 { 1 } else { 0 },
                    v => v.min(4),
                };
            }
        }
        Ok(points)
    }

    // --- Sync Cursors ---

    pub fn get_cursor(&self, source: &str) -> SqlResult<Option<SyncCursor>> {
        let mut stmt = self.conn.prepare(
            "SELECT source, cursor_data, updated_at FROM sync_cursors WHERE source = ?1"
        )?;

        let mut rows = stmt.query_map(params![source], |row| {
            Ok(SyncCursor {
                source: row.get(0)?,
                cursor_data: row.get(1)?,
                updated_at: row.get(2)?,
            })
        })?;

        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn set_cursor(&self, source: &str, cursor_data: &str) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO sync_cursors (source, cursor_data, updated_at)
             VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(source) DO UPDATE SET cursor_data = excluded.cursor_data, updated_at = datetime('now')",
            params![source, cursor_data],
        )?;
        Ok(())
    }

    // --- Settings ---

    pub fn get_setting(&self, key: &str) -> SqlResult<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> SqlResult<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn count_records_by_source(&self, source: &str) -> SqlResult<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM usage WHERE source = ?1",
            params![source],
            |row| row.get(0),
        )
    }
}
