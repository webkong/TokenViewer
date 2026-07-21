use rusqlite::Connection;
use serde_json::Value;
use std::path::Path;

use super::utils::*;
use crate::models::UsageRecord;

pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let mut all_records = Vec::new();
    let mut cursor = FileCursor::from_json(cursor_data);

    #[cfg(target_os = "macos")]
    let dev_data = home_dir
        .join("Library/Application Support/Kiro/User/globalStorage/kiro.kiroagent/dev_data");
    #[cfg(not(target_os = "macos"))]
    let dev_data = home_dir.join(".config/Kiro/User/globalStorage/kiro.kiroagent/dev_data");

    // Primary: devdata.sqlite — accurate per-session data with timestamps
    let db_path = dev_data.join("devdata.sqlite");
    let db_exists = db_path.exists();
    if db_exists && cursor.file_changed(&db_path.to_string_lossy().to_string()) {
        // Build model timeline from .chat metadata files: start_ms → model_name
        let timeline = build_kiro_model_timeline(&dev_data);
        // Fallback model from Kiro settings (kiroAgent.modelSelection in storage.json)
        let settings_model = read_kiro_settings_model(home_dir);

        if let Ok(conn) = Connection::open(&db_path) {
            let last_id: i64 = cursor
                .last_timestamp
                .as_deref()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let sql = "SELECT id, model, provider, tokens_prompt, tokens_generated, timestamp \
                       FROM tokens_generated WHERE id > ?1 ORDER BY id ASC";
            if let Ok(mut stmt) = conn.prepare(sql) {
                let mut max_id = last_id;
                let rows = stmt.query_map([last_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,    // id
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
                        if total == 0 {
                            if id > max_id {
                                max_id = id;
                            }
                            continue;
                        }
                        let model_name = if model == "agent" {
                            "kiro-agent".to_string()
                        } else {
                            normalize_kiro_model(&model)
                        };
                        // timestamp is UTC "YYYY-MM-DD HH:MM:SS" → ISO bucket
                        let iso = ts.replacen(' ', "T", 1) + "Z";
                        let hour_start = iso_to_bucket(&iso);
                        // Resolve actual model from .chat timeline if possible
                        let ts_ms = {
                            let s = ts.replacen(' ', "T", 1) + "Z";
                            chrono::DateTime::parse_from_rfc3339(&s)
                                .map(|dt| dt.timestamp_millis())
                                .unwrap_or(0)
                        };
                        let resolved_model = if timeline.is_empty() {
                            settings_model.clone().unwrap_or(model_name)
                        } else {
                            resolve_kiro_model(&timeline, ts_ms)
                                .unwrap_or_else(|| settings_model.clone().unwrap_or(model_name))
                        };
                        all_records.push(UsageRecord {
                            id: None,
                            hour_start,
                            source: "kiro".to_string(),
                            model: resolved_model,
                            input_tokens: prompt as u64,
                            output_tokens: generated as u64,
                            cached_input_tokens: 0,
                            cache_creation_input_tokens: 0,
                            reasoning_output_tokens: 0,
                            total_tokens: total,
                            conversation_count: 1,
                        });
                        if id > max_id {
                            max_id = id;
                        }
                    }
                }
                cursor.last_timestamp = Some(max_id.to_string());
            }
        }
    }

    // tokens_generated.jsonl is a SIBLING of devdata.sqlite (same usage events) but
    // carries NO per-line timestamp. When the sqlite exists it is authoritative (it
    // has timestamps + row ids), so we must NOT also read the jsonl — doing so would
    // double-count the overlapping events AND dump the untimestamped rows onto the
    // file's mtime day. Only fall back to the jsonl when the sqlite is absent.
    let jsonl = dev_data.join("tokens_generated.jsonl");
    if jsonl.exists() {
        let key = jsonl.to_string_lossy().to_string();
        if db_exists {
            // Advance the cursor to the file tail without counting, so a later run
            // (if the sqlite ever disappears) doesn't re-read already-covered lines.
            let size = std::fs::metadata(&jsonl).map(|m| m.len()).unwrap_or(0);
            cursor.set_offset(&key, size);
        } else {
            // Fallback only: no timestamps available, bucket by file mtime (best effort).
            let offset = cursor.get_offset(&key);
            let bucket = file_mtime_bucket(&jsonl);
            let (mut records, new_offset) =
                parse_jsonl_file(&jsonl, offset, "kiro", parse_kiro_token_line);
            for r in &mut records {
                if r.hour_start.is_empty() {
                    r.hour_start = bucket.clone();
                }
            }
            all_records.extend(records);
            cursor.set_offset(&key, new_offset);
        }
    }

    // Kiro CLI sessions — read .json for turn metadata + .jsonl for char-based token estimation
    let cli_sessions_dir = home_dir.join(".kiro/sessions/cli");
    if cli_sessions_dir.exists() {
        let pattern = format!("{}/*.json", cli_sessions_dir.display());
        let json_files = cursor.glob_cached(&pattern, &cli_sessions_dir);
        for json_file in json_files {
            let key = json_file.to_string_lossy().to_string();
            if !cursor.file_changed(&key) {
                continue;
            }
            if let Some(records) = parse_kiro_cli_session(&json_file, &mut cursor) {
                all_records.extend(records);
            }
        }
    }

    // kiro-cli historical database: ~/Library/Application Support/kiro-cli/data.sqlite3
    // conversations_v2 table: key=cwd, conversation_id, value=JSON with history[].request_metadata
    #[cfg(target_os = "macos")]
    let kiro_cli_db = home_dir.join("Library/Application Support/kiro-cli/data.sqlite3");
    #[cfg(not(target_os = "macos"))]
    let kiro_cli_db = home_dir.join(".local/share/kiro-cli/data.sqlite3");

    if kiro_cli_db.exists() {
        let db_key = kiro_cli_db.to_string_lossy().to_string();
        if cursor.file_changed(&db_key) {
            if let Some(records) = parse_kiro_cli_db(&kiro_cli_db, &mut cursor) {
                all_records.extend(records);
            }
        }
    }

    // Kiro CLI v3 sessions — new per-workspace session store:
    // ~/.kiro/sessions/<workspace-hash>/sess_<uuid>/session.json + messages.jsonl
    // Introduced after the flat ~/.kiro/sessions/cli/*.json format above; the two
    // never overlap (different directory shapes), so no dedup is needed between them.
    parse_kiro_v3_sessions(home_dir, &mut cursor, &mut all_records);

    Ok((aggregate_records(all_records), cursor.to_json()))
}

/// Parse Kiro CLI v3 session directories: `~/.kiro/sessions/*/sess_*/session.json`
/// (metadata) + sibling `messages.jsonl` (append-only event log). Unlike the legacy
/// per-turn `.json` format, the metadata file's mtime lags far behind live activity
/// (observed: `.json` frozen for 20+ hours while `messages.jsonl` kept growing), so
/// incremental sync is driven entirely by the `messages.jsonl` byte offset — never
/// by the `.json` mtime, which was the mistake that caused today's session to go
/// completely uncounted.
///
/// Kiro CLI doesn't expose real per-turn token counts here either: `usage_summary`
/// events only carry an opaque `credit` unit with no token breakdown or per-model
/// exchange rate, so it can't be converted to tokens. We fall back to the same
/// char-based estimate (÷4) used for the legacy CLI format, keeping one consistent
/// estimation convention across both Kiro CLI session formats.
fn parse_kiro_v3_sessions(
    home_dir: &Path,
    cursor: &mut FileCursor,
    all_records: &mut Vec<UsageRecord>,
) {
    let sessions_root = home_dir.join(".kiro/sessions");
    if !sessions_root.exists() {
        return;
    }
    let pattern = format!("{}/*/sess_*/session.json", sessions_root.display());
    let session_files = cursor.glob_cached(&pattern, &sessions_root);

    for session_json_path in session_files {
        let Some(session_dir) = session_json_path.parent() else {
            continue;
        };
        let messages_path = session_dir.join("messages.jsonl");
        if !messages_path.exists() {
            continue;
        }

        // modelId is read fresh each sync (cheap: one small JSON file) so a
        // mid-session model switch is picked up for subsequent turns without
        // needing extra cursor state.
        let model = read_kiro_v3_session_model(&session_json_path);

        let key = messages_path.to_string_lossy().to_string();
        let offset = cursor.get_offset(&key);
        let (lines, new_offset) = match read_lines_from_offset(&messages_path, offset) {
            Ok(r) => r,
            Err(_) => continue,
        };
        if lines.is_empty() {
            continue;
        }

        let mut input_chars: usize = 0;
        let mut output_chars: usize = 0;
        let mut turn_active = false;

        for line in &lines {
            let v: Value = match serde_json::from_str(line) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let Some(payload) = v.get("payload") else {
                continue;
            };
            let event_type = payload.get("type").and_then(|t| t.as_str()).unwrap_or("");

            match event_type {
                "turn_start" => {
                    input_chars = 0;
                    output_chars = 0;
                    turn_active = true;
                }
                "user" | "tool_result" => {
                    input_chars += kiro_v3_content_chars(payload);
                }
                "assistant" | "tool_call" => {
                    output_chars += kiro_v3_content_chars(payload);
                }
                "turn_end" | "usage_summary" => {
                    if !turn_active {
                        continue;
                    }
                    if input_chars + output_chars > 0 {
                        let hour_start = v
                            .get("timestamp")
                            .and_then(|t| t.as_str())
                            .map(iso_to_bucket)
                            .unwrap_or_default();
                        if !hour_start.is_empty() {
                            let input_tokens = (input_chars / 4) as u64;
                            let output_tokens = (output_chars / 4) as u64;
                            all_records.push(UsageRecord {
                                id: None,
                                hour_start,
                                source: "kiro".to_string(),
                                model: model.clone(),
                                input_tokens,
                                output_tokens,
                                cached_input_tokens: 0,
                                cache_creation_input_tokens: 0,
                                reasoning_output_tokens: 0,
                                // Sum the already-truncated per-field token counts rather
                                // than truncating (input_chars + output_chars) / 4 again —
                                // otherwise integer division can make total != input + output.
                                total_tokens: input_tokens + output_tokens,
                                conversation_count: 1,
                            });
                        }
                    }
                    // A turn_end and its matching usage_summary share the same
                    // executionId and arrive back-to-back; only emit once per
                    // turn by resetting here regardless of which one fired first.
                    input_chars = 0;
                    output_chars = 0;
                    turn_active = false;
                }
                _ => {}
            }
        }

        cursor.set_offset(&key, new_offset);
    }
}

/// Read `modelId` from a Kiro CLI v3 `session.json`, normalized to a canonical
/// model name. Falls back to the generic agent bucket if missing/unreadable.
fn read_kiro_v3_session_model(session_json_path: &Path) -> String {
    read_to_string_capped(session_json_path)
        .and_then(|content| serde_json::from_str::<Value>(&content).ok())
        .and_then(|data| {
            data.get("modelId")
                .and_then(|m| m.as_str())
                .map(normalize_kiro_model)
        })
        .unwrap_or_else(|| "kiro-agent".to_string())
}

/// Char count for a v3 message event's payload. `content` is the primary text
/// field (user/assistant/tool_result); `args` is the tool-call input object.
fn kiro_v3_content_chars(payload: &Value) -> usize {
    if let Some(s) = payload.get("content").and_then(|c| c.as_str()) {
        return s.len();
    }
    if let Some(args) = payload.get("args") {
        return args.to_string().len();
    }
    0
}

/// Parse tokens_generated.jsonl format:
/// {"model":"agent","provider":"kiro","promptTokens":13893,"generatedTokens":0}
fn parse_kiro_token_line(v: &Value, source: &str) -> Option<UsageRecord> {
    let prompt = v.get("promptTokens").and_then(|x| x.as_u64()).unwrap_or(0);
    let generated = v
        .get("generatedTokens")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let total = prompt + generated;
    if total == 0 {
        return None;
    }
    let model = v
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("kiro-agent");
    let model_name = if model == "agent" {
        "kiro-agent"
    } else {
        model
    };

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
    let turns = ss
        .pointer("/conversation_metadata/user_turn_metadatas")?
        .as_array()?;
    if turns.is_empty() {
        return Some(vec![]);
    }

    let model_id = ss
        .pointer("/rts_model_state/model_info/model_id")
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
        let hour_start = turn
            .get("end_timestamp")
            .and_then(|t| t.as_str())
            .map(iso_to_bucket)
            .or_else(|| {
                turn.get("request_start_timestamp_ms")
                    .and_then(|ms| ms.as_i64())
                    .and_then(epoch_millis_to_bucket)
            })
            .unwrap_or_default();
        if hour_start.is_empty() {
            continue;
        }

        // Real token counts (usually 0 in kiro-cli)
        let mut input = turn
            .get("input_token_count")
            .and_then(|x| x.as_u64())
            .unwrap_or(0);
        let mut output = turn
            .get("output_token_count")
            .and_then(|x| x.as_u64())
            .unwrap_or(0);

        // Fallback: char-based estimation
        if input == 0 && output == 0 {
            if let Some((in_chars, out_chars)) = char_map.get(&i) {
                input = (*in_chars / 4) as u64;
                output = (*out_chars / 4) as u64;
            }
        }

        let total = input + output;
        if total == 0 {
            continue;
        }

        records.push(UsageRecord {
            id: None,
            hour_start,
            source: "kiro".to_string(),
            model: model.clone(),
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: 0,
            cache_creation_input_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: total,
            conversation_count: 1,
        });
    }

    cursor.set_offset(&key, turns.len() as u64);
    Some(records)
}

/// Build a map of turn_index → (input_chars, output_chars) from the .jsonl sibling.
fn build_cli_char_map(
    jsonl_path: &Path,
    turns: &[Value],
) -> std::collections::HashMap<usize, (usize, usize)> {
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
        let mid = data
            .get("message_id")
            .and_then(|m| m.as_str())
            .unwrap_or("");

        // Count chars in content
        let chars = count_content_chars(data);

        match kind {
            "Prompt" => {
                pending_prompt_chars += chars;
            }
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
/// e.g. CLAUDE_SONNET_4_20250514_V1_0 -> claude-sonnet-4
fn normalize_kiro_model(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower == "agent" {
        return "kiro-agent".to_string();
    }

    let slug = lower.replace('_', "-");
    if let Some(model) = normalize_claude_model_slug(&slug) {
        return model;
    }
    if let Some(model) = normalize_gpt_model_slug(&slug) {
        return model;
    }
    if let Some(model) = normalize_gemini_model_slug(&slug) {
        return model;
    }

    raw.to_string()
}

fn normalize_claude_model_slug(slug: &str) -> Option<String> {
    let start = slug.find("claude-")?;
    let parts: Vec<&str> = slug[start..].split('-').collect();
    if parts.len() < 3 || parts[0] != "claude" {
        return None;
    }

    match parts[1] {
        "opus" | "sonnet" | "haiku" => {
            let major = parts[2];
            if !major.chars().all(|c| c.is_ascii_digit()) {
                return None;
            }

            let mut model = format!("claude-{}-{}", parts[1], major);
            if let Some(minor) = parts.get(3) {
                if minor.len() == 1 && minor.chars().all(|c| c.is_ascii_digit()) {
                    model.push('.');
                    model.push_str(minor);
                }
            }
            Some(model)
        }
        major if major.chars().all(|c| c.is_ascii_digit()) => {
            let minor = parts.get(2)?;
            let family = parts.get(3)?;
            if minor.len() == 1
                && minor.chars().all(|c| c.is_ascii_digit())
                && matches!(*family, "opus" | "sonnet" | "haiku")
            {
                Some(format!("claude-{}.{}-{}", major, minor, family))
            } else {
                Some(slug[start..].to_string())
            }
        }
        _ => Some(slug[start..].to_string()),
    }
}

fn normalize_gpt_model_slug(slug: &str) -> Option<String> {
    normalize_dash_versioned_model_slug(slug, "gpt")
}

fn normalize_gemini_model_slug(slug: &str) -> Option<String> {
    normalize_dash_versioned_model_slug(slug, "gemini")
}

fn normalize_dash_versioned_model_slug(slug: &str, family: &str) -> Option<String> {
    let needle = format!("{family}-");
    let start = slug.find(&needle)?;
    let parts: Vec<&str> = slug[start..].split('-').collect();
    if parts.len() < 2 || parts[0] != family {
        return None;
    }

    let mut model_parts = vec![family.to_string()];
    let mut index = 1;
    if index >= parts.len() || is_build_marker(parts[index]) {
        return None;
    }

    let first = parts[index];
    model_parts.push(first.to_string());
    index += 1;

    if first.chars().all(|c| c.is_ascii_digit()) {
        if let Some(next) = parts.get(index) {
            if next.len() <= 2 && next.chars().all(|c| c.is_ascii_digit()) {
                let last = model_parts.last_mut().unwrap();
                last.push('.');
                last.push_str(next);
                index += 1;
            }
        }
    }

    while let Some(part) = parts.get(index) {
        if is_build_marker(part) {
            break;
        }
        model_parts.push((*part).to_string());
        index += 1;
    }

    Some(model_parts.join("-"))
}

fn is_build_marker(part: &str) -> bool {
    (part.len() >= 4 && part.chars().all(|c| c.is_ascii_digit()))
        || (part.len() > 1
            && part.starts_with('v')
            && part[1..].chars().all(|c| c.is_ascii_digit()))
}

/// Build a sorted timeline of (start_ms, model_name) from .chat metadata files.
fn build_kiro_model_timeline(dev_data: &std::path::Path) -> Vec<(i64, String)> {
    let pattern = format!(
        "{}/**/*.chat",
        dev_data.parent().unwrap_or(dev_data).display()
    );
    let mut timeline: Vec<(i64, String)> = glob_files(&pattern)
        .into_iter()
        .filter_map(|f| {
            let content = std::fs::read_to_string(&f).ok()?;
            let v: serde_json::Value = serde_json::from_str(&content).ok()?;
            let meta = v.get("metadata")?;
            let model_id = meta.get("modelId").and_then(|m| m.as_str())?;
            let start_ms = meta.get("startTime").and_then(|t| t.as_i64())?;
            if start_ms == 0 {
                return None;
            }
            Some((start_ms, normalize_kiro_model(model_id)))
        })
        .collect();
    timeline.sort_by_key(|(ms, _)| *ms);
    timeline
}

/// Find the model active at the given timestamp (ms) from the timeline.
fn resolve_kiro_model(timeline: &[(i64, String)], ts_ms: i64) -> Option<String> {
    if timeline.is_empty() || ts_ms == 0 {
        return None;
    }
    // Find the last chat that started within 10 minutes before ts_ms
    let window = 10 * 60 * 1000i64;
    timeline
        .iter()
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
    if model.is_empty() || model == "auto" {
        return None;
    }
    Some(normalize_kiro_model(model))
}

/// Parse kiro-cli historical database (conversations_v2 table).
/// Each row has a JSON `value` field containing history[].request_metadata with
/// user_prompt_length, response_size, model_id, request_start_timestamp_ms.
/// Tokens are estimated at 4 chars/token (same as reference project).
fn parse_kiro_cli_db(db_path: &Path, cursor: &mut FileCursor) -> Option<Vec<UsageRecord>> {
    let conn = Connection::open(db_path).ok()?;

    // Two-level incremental watermark (accuracy + performance):
    //  1) SQL filter `updated_at >= last` only re-reads conversations that changed
    //     since the previous sync (>= is boundary-safe; the per-conversation
    //     watermark below prevents any double-count of already-seen turns).
    //  2) Per-conversation timestamp watermark counts only turns whose event time
    //     is strictly newer than what we processed before. Timestamps are
    //     monotonic per conversation and survive history compaction (unlike an
    //     index), so this is both compaction-safe and dedup-free.
    let last_updated = cursor.kiro_cli_updated_at;
    let mut stmt = conn
        .prepare("SELECT conversation_id, value, updated_at FROM conversations_v2 WHERE updated_at >= ?1")
        .ok()?;

    let mut records = Vec::new();
    let rows = stmt
        .query_map([last_updated], |row| {
            Ok((
                row.get::<_, String>(0)?,          // conversation_id
                row.get::<_, String>(1)?,          // value (JSON blob)
                row.get::<_, i64>(2).unwrap_or(0), // updated_at (epoch ms)
            ))
        })
        .ok()?;

    let mut max_updated = last_updated;

    for row in rows.flatten() {
        let (conv_id, value_str, updated_at) = row;
        if updated_at > max_updated {
            max_updated = updated_at;
        }
        let val: Value = match serde_json::from_str(&value_str) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let history = match val.get("history").and_then(|h| h.as_array()) {
            Some(h) => h,
            None => continue,
        };

        let conv_wm = cursor.kiro_cli_conv_ts.get(&conv_id).copied().unwrap_or(0);
        let mut new_wm = conv_wm;

        for turn in history {
            let user = match turn.get("user") {
                Some(u) => u,
                None => continue,
            };
            let meta = match turn
                .get("assistant")
                .and_then(|a| a.get("request_metadata"))
                .or_else(|| turn.get("request_metadata"))
            {
                Some(m) => m,
                None => continue,
            };
            let request_id = meta
                .get("request_id")
                .and_then(|r| r.as_str())
                .unwrap_or("");
            if request_id.is_empty() {
                continue;
            }

            // Event time (epoch ms): prefer request_start_timestamp_ms, fall back to
            // the user message timestamp. Turns with neither cannot be placed in
            // time nor deduplicated, so they are skipped.
            let event_ts_ms = {
                let req_ts = meta
                    .get("request_start_timestamp_ms")
                    .and_then(|t| t.as_i64())
                    .unwrap_or(0);
                if req_ts > 0 {
                    req_ts
                } else {
                    user.get("timestamp")
                        .and_then(|t| t.as_str())
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| dt.timestamp_millis())
                        .unwrap_or(0)
                }
            };
            if event_ts_ms <= 0 {
                continue;
            }
            // Per-conversation watermark: skip turns already counted.
            if event_ts_ms <= conv_wm {
                continue;
            }
            if event_ts_ms > new_wm {
                new_wm = event_ts_ms;
            }

            let model_id = meta
                .get("model_id")
                .and_then(|m| m.as_str())
                .unwrap_or("auto");
            let model = normalize_kiro_model(model_id);
            let hour_start = epoch_millis_to_bucket(event_ts_ms).unwrap_or_else(now_bucket);

            let prompt_chars = meta
                .get("user_prompt_length")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let resp_chars = meta
                .get("response_size")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let input = prompt_chars / 4;
            let output = resp_chars / 4;
            let total = input + output;
            if total == 0 {
                continue;
            }

            records.push(UsageRecord {
                id: None,
                hour_start,
                source: "kiro".to_string(),
                model,
                input_tokens: input,
                output_tokens: output,
                cached_input_tokens: 0,
                cache_creation_input_tokens: 0,
                reasoning_output_tokens: 0,
                total_tokens: total,
                conversation_count: 1,
            });
        }

        if new_wm > conv_wm {
            cursor.kiro_cli_conv_ts.insert(conv_id, new_wm);
        }
    }

    cursor.kiro_cli_updated_at = max_updated;
    Some(records)
}

#[cfg(test)]
mod tests {
    use super::normalize_kiro_model;

    #[test]
    fn normalizes_future_claude_sonnet_clean_name() {
        assert_eq!(normalize_kiro_model("claude-sonnet-5"), "claude-sonnet-5");
    }

    #[test]
    fn normalizes_future_claude_sonnet_internal_id_without_downgrading() {
        assert_eq!(
            normalize_kiro_model("CLAUDE_SONNET_5_20260701_V1_0"),
            "claude-sonnet-5"
        );
    }

    #[test]
    fn preserves_existing_claude_sonnet_four_internal_id() {
        assert_eq!(
            normalize_kiro_model("CLAUDE_SONNET_4_20250514_V1_0"),
            "claude-sonnet-4"
        );
    }

    #[test]
    fn normalizes_claude_minor_versions_without_enumerating_each_release() {
        assert_eq!(
            normalize_kiro_model("CLAUDE_OPUS_4_8_20260101_V1_0"),
            "claude-opus-4.8"
        );
        assert_eq!(
            normalize_kiro_model("claude-sonnet-6-2-20270101"),
            "claude-sonnet-6.2"
        );
    }

    #[test]
    fn normalizes_gpt_models_without_enumerating_each_release() {
        assert_eq!(normalize_kiro_model("gpt-5"), "gpt-5");
        assert_eq!(normalize_kiro_model("GPT_5_20260701_V1_0"), "gpt-5");
        assert_eq!(normalize_kiro_model("GPT_4_1_MINI_20250101"), "gpt-4.1-mini");
        assert_eq!(normalize_kiro_model("gpt-6-nano"), "gpt-6-nano");
    }

    #[test]
    fn normalizes_gemini_models_without_enumerating_each_release() {
        assert_eq!(normalize_kiro_model("gemini-2.5-pro"), "gemini-2.5-pro");
        assert_eq!(
            normalize_kiro_model("GEMINI_2_5_FLASH_20250601"),
            "gemini-2.5-flash"
        );
        assert_eq!(normalize_kiro_model("gemini-3-pro"), "gemini-3-pro");
    }

    #[test]
    fn leaves_unknown_non_family_models_unchanged() {
        assert_eq!(normalize_kiro_model("CUSTOM_MODEL_X"), "CUSTOM_MODEL_X");
    }
}
