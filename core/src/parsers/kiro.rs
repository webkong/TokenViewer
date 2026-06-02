use std::path::Path;
use rusqlite::Connection;
use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(home_dir: &Path, cursor_data: Option<&str>) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let mut all_records = Vec::new();
    let mut cursor = FileCursor::from_json(cursor_data);

    #[cfg(target_os = "macos")]
    let dev_data = home_dir.join("Library/Application Support/Kiro/User/globalStorage/kiro.kiroagent/dev_data");
    #[cfg(not(target_os = "macos"))]
    let dev_data = home_dir.join(".config/Kiro/User/globalStorage/kiro.kiroagent/dev_data");

    // Primary: devdata.sqlite — accurate per-session data with timestamps
    let db_path = dev_data.join("devdata.sqlite");
    if db_path.exists() {
        // Build model timeline from .chat metadata files: start_ms → model_name
        let timeline = build_kiro_model_timeline(&dev_data);
        // Fallback model from Kiro settings (kiroAgent.modelSelection in storage.json)
        let settings_model = read_kiro_settings_model(home_dir);

        if let Ok(conn) = Connection::open(&db_path) {
            let last_id: i64 = cursor.last_timestamp
                .as_deref().and_then(|s| s.parse().ok()).unwrap_or(0);
            let sql = "SELECT id, model, provider, tokens_prompt, tokens_generated, timestamp \
                       FROM tokens_generated WHERE id > ?1 ORDER BY id ASC";
            if let Ok(mut stmt) = conn.prepare(sql) {
                let mut max_id = last_id;
                let rows = stmt.query_map([last_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,   // id
                        row.get::<_, String>(1)?, // model
                        row.get::<_, i64>(3)?,    // tokens_prompt
                        row.get::<_, i64>(4)?,    // tokens_generated
                        row.get::<_, String>(5)?, // timestamp "YYYY-MM-DD HH:MM:SS"
                    ))
                });
                if let Ok(rows) = rows {
                    for row in rows.flatten() {
                        let (id, model, prompt, generated, ts) = row;
                        let total = (prompt + generated) as u64;
                        if total == 0 { if id > max_id { max_id = id; } continue; }
                        let model_name = if model == "agent" { "kiro-agent".to_string() } else { normalize_kiro_model(&model) };
                        // timestamp is UTC "YYYY-MM-DD HH:MM:SS" → ISO bucket
                        let iso = ts.replacen(' ', "T", 1) + "Z";
                        let hour_start = iso_to_bucket(&iso);
                        // Resolve actual model from .chat timeline if possible
                        let ts_ms = {
                            let s = ts.replacen(' ', "T", 1) + "Z";
                            chrono::DateTime::parse_from_rfc3339(&s)
                                .map(|dt| dt.timestamp_millis()).unwrap_or(0)
                        };
                        let resolved_model = if timeline.is_empty() {
                            settings_model.clone().unwrap_or(model_name)
                        } else {
                            resolve_kiro_model(&timeline, ts_ms)
                                .unwrap_or_else(|| settings_model.clone().unwrap_or(model_name))
                        };
                        all_records.push(UsageRecord {
                            id: None, hour_start,
                            source: "kiro".to_string(), model: resolved_model,
                            input_tokens: prompt as u64, output_tokens: generated as u64,
                            cached_input_tokens: 0, cache_creation_input_tokens: 0,
                            reasoning_output_tokens: 0, total_tokens: total,
                            conversation_count: 1,
                        });
                        if id > max_id { max_id = id; }
                    }
                }
                cursor.last_timestamp = Some(max_id.to_string());
            }
        }
    }

    // Fallback: tokens_generated.jsonl (if SQLite unavailable or for older data)
    let jsonl = dev_data.join("tokens_generated.jsonl");
    if jsonl.exists() && all_records.is_empty() {
        let key = jsonl.to_string_lossy().to_string();
        let offset = cursor.get_offset(&key);
        let bucket = file_mtime_bucket(&jsonl);
        let (mut records, new_offset) = parse_jsonl_file(&jsonl, offset, "kiro", parse_kiro_token_line);
        for r in &mut records { if r.hour_start.is_empty() { r.hour_start = bucket.clone(); } }
        all_records.extend(records);
        cursor.set_offset(&key, new_offset);
    }

    // Kiro CLI sessions
    let cli_pattern = format!("{}/.kiro/sessions/**/*.jsonl", home_dir.display());
    for file in glob_files(&cli_pattern) {
        let key = file.to_string_lossy().to_string();
        let offset = cursor.get_offset(&key);
        let bucket = file_mtime_bucket(&file);
        let (mut records, new_offset) = parse_jsonl_file(&file, offset, "kiro", parse_kiro_cli_line);
        for r in &mut records { if r.hour_start.is_empty() { r.hour_start = bucket.clone(); } }
        all_records.extend(records);
        cursor.set_offset(&key, new_offset);
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}

/// Parse tokens_generated.jsonl format:
/// {"model":"agent","provider":"kiro","promptTokens":13893,"generatedTokens":0}
fn parse_kiro_token_line(v: &Value, source: &str) -> Option<UsageRecord> {
    let prompt = v.get("promptTokens").and_then(|x| x.as_u64()).unwrap_or(0);
    let generated = v.get("generatedTokens").and_then(|x| x.as_u64()).unwrap_or(0);
    let total = prompt + generated;
    if total == 0 {
        return None;
    }
    let model = v.get("model").and_then(|m| m.as_str()).unwrap_or("kiro-agent");
    let model_name = if model == "agent" { "kiro-agent" } else { model };

    Some(UsageRecord {
        id: None,
        hour_start: String::new(), // filled by caller
        source: source.to_string(),
        model: model_name.to_string(),
        input_tokens: prompt,
        output_tokens: generated,
        cached_input_tokens: 0,
        cache_creation_input_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: total,
        conversation_count: 1,
    })
}

/// Parse Kiro CLI session JSONL (similar to Claude format)
fn parse_kiro_cli_line(v: &Value, source: &str) -> Option<UsageRecord> {
    let usage = v.pointer("/message/usage")
        .or_else(|| v.get("usage"))?;
    if !usage.is_object() {
        return None;
    }
    let model = v.pointer("/message/model")
        .or_else(|| v.get("model"))
        .and_then(|m| m.as_str())
        .unwrap_or("kiro-agent")
        .to_string();

    let input = usage.get("input_tokens").and_then(|x| x.as_u64())
        .or_else(|| usage.get("promptTokens").and_then(|x| x.as_u64()))
        .unwrap_or(0);
    let output = usage.get("output_tokens").and_then(|x| x.as_u64())
        .or_else(|| usage.get("generatedTokens").and_then(|x| x.as_u64()))
        .unwrap_or(0);
    let cache_read = usage.get("cache_read_input_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
    let total = input + output + cache_read;
    if total == 0 {
        return None;
    }

    let hour_start = v.get("timestamp")
        .and_then(|t| t.as_str())
        .map(|s| iso_to_bucket(s))
        .unwrap_or_default();

    Some(UsageRecord {
        id: None,
        hour_start,
        source: source.to_string(),
        model,
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: cache_read,
        cache_creation_input_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: total,
        conversation_count: 1,
    })
}

/// Normalize Kiro internal model IDs to canonical names.
/// e.g. CLAUDE_SONNET_4_20250514_V1_0 → claude-sonnet-4
fn normalize_kiro_model(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower == "agent" { return "kiro-agent".to_string(); }
    // Convert CLAUDE_SONNET_4_20250514_V1_0 → claude-sonnet-4
    let slug = lower.replace('_', "-");
    for prefix in &["claude-opus-4", "claude-sonnet-4", "claude-haiku-4",
                     "claude-3-5-sonnet", "claude-3-5-haiku",
                     "gpt-4", "gpt-5", "gemini"] {
        if slug.contains(prefix) { return prefix.to_string(); }
    }
    if slug.contains("claude") { return "claude-sonnet-4".to_string(); }
    raw.to_string()
}

/// Build a sorted timeline of (start_ms, model_name) from .chat metadata files.
fn build_kiro_model_timeline(dev_data: &std::path::Path) -> Vec<(i64, String)> {
    let pattern = format!("{}/**/*.chat", dev_data.parent().unwrap_or(dev_data).display());
    let mut timeline: Vec<(i64, String)> = glob_files(&pattern)
        .into_iter()
        .filter_map(|f| {
            let content = std::fs::read_to_string(&f).ok()?;
            let v: serde_json::Value = serde_json::from_str(&content).ok()?;
            let meta = v.get("metadata")?;
            let model_id = meta.get("modelId").and_then(|m| m.as_str())?;
            let start_ms = meta.get("startTime").and_then(|t| t.as_i64())?;
            if start_ms == 0 { return None; }
            Some((start_ms, normalize_kiro_model(model_id)))
        })
        .collect();
    timeline.sort_by_key(|(ms, _)| *ms);
    timeline
}

/// Find the model active at the given timestamp (ms) from the timeline.
fn resolve_kiro_model(timeline: &[(i64, String)], ts_ms: i64) -> Option<String> {
    if timeline.is_empty() || ts_ms == 0 { return None; }
    // Find the last chat that started within 10 minutes before ts_ms
    let window = 10 * 60 * 1000i64;
    timeline.iter()
        .filter(|(start, _)| ts_ms >= *start && ts_ms - start <= window)
        .max_by_key(|(start, _)| *start)
        .map(|(_, model)| model.clone())
}

/// Read kiroAgent.modelSelection from Kiro's storage.json (the user's active model choice).
fn read_kiro_settings_model(home_dir: &std::path::Path) -> Option<String> {
    #[cfg(target_os = "macos")]
    let storage_path = home_dir.join("Library/Application Support/Kiro/User/settings.json");
    #[cfg(not(target_os = "macos"))]
    let storage_path = home_dir.join(".config/Kiro/User/settings.json");

    let content = std::fs::read_to_string(&storage_path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    let model = v.get("kiroAgent.modelSelection").and_then(|m| m.as_str())?;
    if model.is_empty() || model == "auto" { return None; }
    Some(normalize_kiro_model(model))
}
