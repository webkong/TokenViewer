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
    if db_path.exists() && cursor.file_changed(&db_path.to_string_lossy().to_string()) {
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

    // Kiro CLI sessions — read .json for turn metadata + .jsonl for char-based token estimation
    let cli_sessions_dir = home_dir.join(".kiro/sessions/cli");
    if cli_sessions_dir.exists() {
        let pattern = format!("{}/*.json", cli_sessions_dir.display());
        let json_files = cursor.glob_cached(&pattern, &cli_sessions_dir);
        for json_file in json_files {
            let key = json_file.to_string_lossy().to_string();
            if !cursor.file_changed(&key) { continue; }
            if let Some(records) = parse_kiro_cli_session(&json_file, &mut cursor) {
                all_records.extend(records);
            }
        }
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

/// Parse a Kiro CLI session .json file + its .jsonl sibling for token estimation.
/// Kiro CLI does NOT store real token counts; we approximate from char counts (÷4).
fn parse_kiro_cli_session(json_path: &Path, cursor: &mut FileCursor) -> Option<Vec<UsageRecord>> {
    let content = read_to_string_capped(json_path)?;
    let data: Value = serde_json::from_str(&content).ok()?;
    let ss = data.get("session_state")?;
    let turns = ss.pointer("/conversation_metadata/user_turn_metadatas")?.as_array()?;
    if turns.is_empty() { return Some(vec![]); }

    let model_id = ss.pointer("/rts_model_state/model_info/model_id")
        .and_then(|m| m.as_str())
        .unwrap_or("kiro-cli-agent");
    let model = normalize_kiro_model(model_id);

    // Dedup key: use session file path + turn count to detect new turns
    let key = json_path.to_string_lossy().to_string();
    let prev_turn_count = cursor.get_offset(&key) as usize;
    if turns.len() <= prev_turn_count {
        return Some(vec![]);
    }

    // Read .jsonl sibling for char counts
    let jsonl_path = json_path.with_extension("jsonl");
    let char_map = build_cli_char_map(&jsonl_path, turns);

    let mut records = Vec::new();
    for (i, turn) in turns.iter().enumerate().skip(prev_turn_count) {
        // Timestamp: end_timestamp (ISO) or request_start_timestamp_ms
        let hour_start = turn.get("end_timestamp")
            .and_then(|t| t.as_str())
            .map(iso_to_bucket)
            .or_else(|| turn.get("request_start_timestamp_ms")
                .and_then(|ms| ms.as_i64())
                .and_then(epoch_millis_to_bucket))
            .unwrap_or_default();
        if hour_start.is_empty() { continue; }

        // Real token counts (usually 0 in kiro-cli)
        let mut input = turn.get("input_token_count").and_then(|x| x.as_u64()).unwrap_or(0);
        let mut output = turn.get("output_token_count").and_then(|x| x.as_u64()).unwrap_or(0);

        // Fallback: char-based estimation
        if input == 0 && output == 0 {
            if let Some((in_chars, out_chars)) = char_map.get(&i) {
                input = (*in_chars / 4) as u64;
                output = (*out_chars / 4) as u64;
            }
        }

        let total = input + output;
        if total == 0 { continue; }

        records.push(UsageRecord {
            id: None, hour_start,
            source: "kiro".to_string(),
            model: model.clone(),
            input_tokens: input, output_tokens: output,
            cached_input_tokens: 0, cache_creation_input_tokens: 0,
            reasoning_output_tokens: 0, total_tokens: total,
            conversation_count: 1,
        });
    }

    cursor.set_offset(&key, turns.len() as u64);
    Some(records)
}

/// Build a map of turn_index → (input_chars, output_chars) from the .jsonl sibling.
fn build_cli_char_map(jsonl_path: &Path, turns: &[Value]) -> std::collections::HashMap<usize, (usize, usize)> {
    let mut result = std::collections::HashMap::new();

    // Build message_id → turn_index mapping
    let mut mid_to_turn: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, turn) in turns.iter().enumerate() {
        if let Some(ids) = turn.get("message_ids").and_then(|a| a.as_array()) {
            for id in ids {
                if let Some(s) = id.as_str() {
                    mid_to_turn.insert(s, i);
                }
            }
        }
    }

    let content = match read_to_string_capped(jsonl_path) {
        Some(c) => c,
        None => return result,
    };

    // Track Prompt chars and attribute to next AssistantMessage's turn
    let mut pending_prompt_chars: usize = 0;
    let mut attributed_turns = std::collections::HashSet::new();

    for line in content.lines() {
        let evt: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let kind = evt.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        let data = match evt.get("data") {
            Some(d) => d,
            None => continue,
        };
        let mid = data.get("message_id").and_then(|m| m.as_str()).unwrap_or("");

        // Count chars in content
        let chars = count_content_chars(data);

        match kind {
            "Prompt" => { pending_prompt_chars += chars; }
            "AssistantMessage" => {
                if let Some(&turn_idx) = mid_to_turn.get(mid) {
                    // Attribute pending prompt chars to this turn
                    if !attributed_turns.contains(&turn_idx) {
                        let entry = result.entry(turn_idx).or_insert((0, 0));
                        entry.0 += pending_prompt_chars;
                        attributed_turns.insert(turn_idx);
                        pending_prompt_chars = 0;
                    }
                    // Output chars
                    let entry = result.entry(turn_idx).or_insert((0, 0));
                    entry.1 += chars;
                }
            }
            "ToolResults" => {
                // Tool results count as input to the model
                if let Some(&turn_idx) = mid_to_turn.get(mid) {
                    let entry = result.entry(turn_idx).or_insert((0, 0));
                    entry.0 += chars;
                }
            }
            _ => {}
        }
    }
    result
}

/// Count chars in a JSONL event's data.content array.
fn count_content_chars(data: &Value) -> usize {
    let content = match data.get("content").and_then(|c| c.as_array()) {
        Some(c) => c,
        None => return 0,
    };
    let mut chars = 0;
    for item in content {
        match item.get("kind").and_then(|k| k.as_str()) {
            Some("text") => {
                if let Some(s) = item.get("data").and_then(|d| d.as_str()) {
                    chars += s.len();
                }
            }
            Some("toolUse") => {
                if let Some(d) = item.get("data") {
                    if let Some(input) = d.get("input") {
                        chars += input.to_string().len();
                    }
                }
            }
            Some("toolResult") => {
                if let Some(d) = item.get("data").and_then(|d| d.as_str()) {
                    chars += d.len();
                } else if let Some(d) = item.get("data") {
                    chars += d.to_string().len();
                }
            }
            _ => {}
        }
    }
    chars
}

/// Normalize Kiro internal model IDs to canonical names.
/// e.g. CLAUDE_SONNET_4_20250514_V1_0 → claude-sonnet-4
fn normalize_kiro_model(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower == "agent" { return "kiro-agent".to_string(); }

    // Already a clean model name like "claude-opus-4.6", "claude-sonnet-4.5" — keep as-is
    let version_re = ["claude-opus-4", "claude-sonnet-4", "claude-haiku-4",
                      "claude-3-5-sonnet", "claude-3-5-haiku",
                      "gpt-4", "gpt-4o", "gpt-5", "gemini"];
    for prefix in &version_re {
        if lower.starts_with(prefix) { return lower; }
    }

    // Convert internal IDs like CLAUDE_SONNET_4_20250514_V1_0 → family name
    let slug = lower.replace('_', "-");
    // Try specific versioned matches first (longer prefix wins)
    for prefix in &["claude-opus-4-5", "claude-opus-4-6", "claude-opus-4-7", "claude-opus-4-8",
                     "claude-sonnet-4-5", "claude-sonnet-4-6",
                     "claude-haiku-4", "claude-opus-4", "claude-sonnet-4",
                     "claude-3-5-sonnet", "claude-3-5-haiku",
                     "gpt-4o", "gpt-4", "gpt-5", "gemini"] {
        if slug.contains(prefix) {
            // Convert dashes back to dots for version: claude-opus-4-6 → claude-opus-4.6
            let model = prefix.replace("-4-", "-4.").replace("-3-5-", "-3.5-");
            return model;
        }
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
