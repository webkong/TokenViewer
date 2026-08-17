//! Session discovery for the Sessions browser.
//!
//! Scans per-agent conversation files on disk and materializes a lightweight,
//! resumable `Session` row per conversation. Agents are registered as small
//! scanner closures (the same registry/adapter spirit as the Swift command
//! registry) so the agent list is derived from what is actually on disk rather
//! than a hardcoded set.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use rusqlite::params;

use crate::models::{Session, UsageRecord};
use crate::parsers::utils::{glob_files, OffsetLineReader};
use crate::storage::Database;

const CURSOR_KEY: &str = "sessions.scan_cursor.v4";

/// Per-file mtime map used to skip unchanged files on incremental rescans.
#[derive(Default, Serialize, Deserialize)]
struct ScanCursor {
    stamps: HashMap<String, String>,
}

impl ScanCursor {
    fn from_json(data: Option<&str>) -> Self {
        data.and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default()
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// True if the file changed since the last scan. Nanosecond precision plus
    /// length avoids missing rapid appends that land within one wall-clock second.
    fn changed(&mut self, path: &str) -> bool {
        let stamp = std::fs::metadata(path)
            .ok()
            .and_then(|metadata| {
                let modified = metadata.modified().ok()?;
                let elapsed = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
                Some(format!("{}:{}:{}", elapsed.as_secs(), elapsed.subsec_nanos(), metadata.len()))
            })
            .unwrap_or_default();
        if self.stamps.get(path) != Some(&stamp) {
            self.stamps.insert(path.to_string(), stamp);
            true
        } else {
            false
        }
    }
}

#[derive(Default)]
struct SessionMetrics {
    model: String,
    input_tokens: u64,
    output_tokens: u64,
    cached_input_tokens: u64,
    cache_creation_input_tokens: u64,
    reasoning_output_tokens: u64,
    turn_count: u32,
    edit_count: u32,
    duration_seconds: u64,
    last_timestamp_ms: Option<i64>,
}

impl SessionMetrics {
    fn observe_timestamp(&mut self, timestamp: Option<&str>) {
        let Some(timestamp) = timestamp else { return };
        let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(timestamp) else { return };
        let current = parsed.timestamp_millis();
        if let Some(previous) = self.last_timestamp_ms {
            let delta = current.saturating_sub(previous);
            if delta > 0 && delta <= 30 * 60 * 1000 {
                self.duration_seconds = self.duration_seconds.saturating_add(delta as u64 / 1000);
            }
        }
        self.last_timestamp_ms = Some(current);
    }

    fn total_tokens(&self) -> u64 {
        self.input_tokens
            .saturating_add(self.output_tokens)
            .saturating_add(self.cached_input_tokens)
            .saturating_add(self.cache_creation_input_tokens)
            .saturating_add(self.reasoning_output_tokens)
    }

    fn cost(&self, source: &str) -> f64 {
        if self.total_tokens() == 0 {
            return 0.0;
        }
        crate::pricing::compute_row_cost(&UsageRecord {
            id: None,
            hour_start: String::new(),
            source: source.to_string(),
            model: self.model.clone(),
            project_key: String::new(),
            project_ref: String::new(),
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cached_input_tokens: self.cached_input_tokens,
            cache_creation_input_tokens: self.cache_creation_input_tokens,
            reasoning_output_tokens: self.reasoning_output_tokens,
            total_tokens: self.total_tokens(),
            conversation_count: self.turn_count,
        })
    }
}

fn collect_claude_assistant_metrics(v: &Value, metrics: &mut SessionMetrics) {
    if let Some(model) = v.pointer("/message/model").and_then(Value::as_str) {
        if !model.trim().is_empty() {
            metrics.model = model.trim().to_string();
        }
    }
    if let Some(usage) = v.pointer("/message/usage").or_else(|| v.get("usage")) {
        metrics.input_tokens = metrics.input_tokens.saturating_add(
            usage.get("input_tokens").and_then(Value::as_u64).unwrap_or(0),
        );
        metrics.output_tokens = metrics.output_tokens.saturating_add(
            usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0),
        );
        metrics.cached_input_tokens = metrics.cached_input_tokens.saturating_add(
            usage.get("cache_read_input_tokens").and_then(Value::as_u64).unwrap_or(0),
        );
        metrics.cache_creation_input_tokens = metrics.cache_creation_input_tokens.saturating_add(
            usage
                .get("cache_creation_input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        );
    }
    if let Some(blocks) = v.pointer("/message/content").and_then(Value::as_array) {
        for block in blocks {
            if block.get("type").and_then(Value::as_str) != Some("tool_use") {
                continue;
            }
            let name = block.get("name").and_then(Value::as_str).unwrap_or("");
            if is_edit_tool(name) {
                metrics.edit_count = metrics.edit_count.saturating_add(1);
            }
        }
    }
}

fn is_edit_tool(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "apply_patch" | "edit" | "write" | "multiedit" | "notebookedit"
            | "search_replace" | "str_replace" | "create_file" | "write_file"
    )
}

/// Scan all registered session sources and store the results. Returns the number
/// of sessions upserted. Cache-first: unchanged files are skipped via mtime.
pub fn scan_and_store(db: &Database, home: &Path) -> Result<usize, String> {
    let cursor_json = db.get_setting(CURSOR_KEY).map_err(|e| e.to_string())?;
    let mut cursor = ScanCursor::from_json(cursor_json.as_deref());

    let mut sessions: Vec<Session> = Vec::new();
    let mut all_ids: Vec<String> = Vec::new();

    scan_claude(home, &mut cursor, &mut sessions, &mut all_ids);
    scan_codex(home, &mut cursor, &mut sessions, &mut all_ids);
    scan_grok(home, &mut cursor, &mut sessions, &mut all_ids);
    scan_opencode(home, &mut sessions, &mut all_ids);
    scan_kiro(home, &mut cursor, &mut sessions, &mut all_ids);
    scan_copilot(home, &mut sessions, &mut all_ids);
    scan_gemini(home, &mut cursor, &mut sessions, &mut all_ids);
    scan_hermes(home, &mut sessions, &mut all_ids);

    let mut upserted = 0usize;
    for session in &sessions {
        if db.upsert_session(session).is_ok() {
            upserted += 1;
        }
    }

    // Drop rows whose source file no longer exists (moved/rotated/deleted),
    // scoped to the sources this scanner owns.
    let _ = db.prune_sessions(
        &[
            "claude", "codex", "grok", "opencode", "kiro", "copilot", "gemini", "hermes",
        ],
        &all_ids,
    );

    let _ = db.set_setting(CURSOR_KEY, &cursor.to_json());
    Ok(upserted)
}

// --- Claude Code ----------------------------------------------------------

fn scan_claude(home: &Path, cursor: &mut ScanCursor, out: &mut Vec<Session>, all_ids: &mut Vec<String>) {
    let base = home.join(".claude").join("projects");
    if !base.exists() {
        return;
    }
    let pattern = format!("{}/**/*.jsonl", base.display());
    for file in glob_files(&pattern) {
        let path_str = file.to_string_lossy().to_string();
        // Subagent transcripts live under `<session>/subagents/` and are not
        // resumable top-level sessions — skip them.
        if path_str.contains("/subagents/") {
            continue;
        }
        let Some(raw_id) = file.file_stem().map(|s| s.to_string_lossy().to_string()) else {
            continue;
        };
        all_ids.push(format!("claude:{raw_id}"));
        if !cursor.changed(&path_str) {
            continue;
        }
        if let Some(session) = parse_claude_session(&file, &raw_id) {
            out.push(session);
        }
    }
}

fn parse_claude_session(file: &Path, raw_id: &str) -> Option<Session> {
    let reader = OffsetLineReader::new(file, 0).ok()?;
    let mut cwd = String::new();
    let mut agent_title = String::new();
    let mut first_user = String::new();
    let mut started_at = String::new();
    let mut metrics = SessionMetrics::default();

    for (line, _) in reader {
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        metrics.observe_timestamp(v.get("timestamp").and_then(Value::as_str));

        if let Some(t) = v.get("type").and_then(Value::as_str) {
            match t {
                "ai-title" => {
                    if let Some(title) = v.get("aiTitle").and_then(Value::as_str) {
                        let cleaned = clean_user_message(title).unwrap_or_else(|| title.trim().to_string());
                        if !cleaned.is_empty() {
                            agent_title = cleaned;
                        }
                    }
                }
                "user" if first_user.is_empty() => {
                    if let Some(text) = claude_user_text(&v) {
                        metrics.turn_count = metrics.turn_count.saturating_add(1);
                        if let Some(cleaned) = clean_user_message(&text) {
                            first_user = cleaned;
                        }
                    }
                    if started_at.is_empty() {
                        started_at = v.get("timestamp").and_then(Value::as_str).unwrap_or("").to_string();
                    }
                }
                "user" => {
                    if claude_user_text(&v).is_some() {
                        metrics.turn_count = metrics.turn_count.saturating_add(1);
                    }
                }
                "assistant" => collect_claude_assistant_metrics(&v, &mut metrics),
                _ => {}
            }
        }
        if cwd.is_empty() {
            cwd = v.get("cwd").and_then(Value::as_str).unwrap_or("").to_string();
        }
        if started_at.is_empty() {
            started_at = v.get("timestamp").and_then(Value::as_str).unwrap_or("").to_string();
        }
    }

    let project = project_label(&cwd);
    let title = derive_title(&agent_title, &first_user, &project, &started_at);
    let last_active_at = mtime_iso(file);
    Some(Session {
        id: format!("claude:{raw_id}"),
        source: "claude".to_string(),
        cwd,
        project,
        title,
        custom_title: None,
        first_user_message: first_user,
        started_at,
        last_active_at,
        file_path: file.to_string_lossy().to_string(),
        codex_home: String::new(),
        model: metrics.model.clone(),
        total_tokens: metrics.total_tokens(),
        total_cost_usd: metrics.cost("claude"),
        turn_count: metrics.turn_count,
        edit_count: metrics.edit_count,
        duration_seconds: metrics.duration_seconds,
    })
}

/// Extract the plain-text content of a Claude `type:"user"` line.
fn claude_user_text(v: &Value) -> Option<String> {
    if v.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let content = v.pointer("/message/content")?;
    match content {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty()
                || trimmed == "[Request interrupted by user]"
                || trimmed.starts_with("<task-notification>")
            {
                None
            } else {
                Some(s.clone())
            }
        }
        Value::Array(blocks) => {
            if !blocks.is_empty()
                && blocks
                    .iter()
                    .all(|block| block.get("type").and_then(Value::as_str) == Some("tool_result"))
            {
                return None;
            }
            let mut parts: Vec<&str> = Vec::new();
            for block in blocks {
                if block.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        parts.push(text);
                    }
                }
            }
            let joined = parts.join("\n");
            if joined.trim().is_empty()
                || joined.trim() == "[Request interrupted by user]"
                || joined.trim_start().starts_with("<task-notification>")
            {
                None
            } else {
                Some(joined)
            }
        }
        _ => None,
    }
}

// --- Codex -----------------------------------------------------------------

fn scan_codex(home: &Path, cursor: &mut ScanCursor, out: &mut Vec<Session>, all_ids: &mut Vec<String>) {
    let homes = crate::codex_home::discover_codex_homes(home, &[], &[], false);
    let default_home = normalize(&home.join(".codex"));
    let mut bases: Vec<(PathBuf, String)> = Vec::new();
    for item in homes.iter().filter(|item| item.exists) {
        let home_path = item.path.clone();
        // codex_home is only meaningful for isolated (non-default) homes.
        let codex_home = if normalize(Path::new(&home_path)) == default_home {
            String::new()
        } else {
            home_path.clone()
        };
        for sub in ["sessions", "archived_sessions"] {
            bases.push((PathBuf::from(&home_path).join(sub), codex_home.clone()));
        }
    }
    if bases.is_empty() {
        bases.push((home.join(".codex").join("sessions"), String::new()));
        bases.push((home.join(".codex").join("archived_sessions"), String::new()));
    }

    for (base, codex_home) in bases {
        if !base.exists() {
            continue;
        }
        let index_home = if codex_home.is_empty() {
            home.join(".codex")
        } else {
            PathBuf::from(&codex_home)
        };
        let title_index = load_codex_title_index(&index_home.join("session_index.jsonl"));
        let pattern = format!("{}/**/rollout-*.jsonl", base.display());
        for file in glob_files(&pattern) {
            let Some(raw_id) = extract_last_uuid(&file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()) else {
                continue;
            };
            let path_str = file.to_string_lossy().to_string();
            all_ids.push(format!("codex:{raw_id}"));
            if !cursor.changed(&path_str) {
                continue;
            }
            let indexed_title = title_index.get(&raw_id).map(String::as_str).unwrap_or("");
            if let Some(session) = parse_codex_session(&file, &raw_id, &codex_home, indexed_title) {
                out.push(session);
            }
        }
    }
}

fn load_codex_title_index(path: &Path) -> HashMap<String, String> {
    let mut titles = HashMap::new();
    let Ok(reader) = OffsetLineReader::new(path, 0) else { return titles };
    for (line, _) in reader {
        let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
        let Some(id) = value.get("id").and_then(Value::as_str) else { continue };
        let Some(title) = value.get("thread_name").and_then(Value::as_str) else { continue };
        if let Some(cleaned) = clean_user_message(title) {
            titles.insert(id.to_string(), cleaned);
        }
    }
    titles
}

fn parse_codex_session(
    file: &Path,
    raw_id: &str,
    codex_home: &str,
    indexed_title: &str,
) -> Option<Session> {
    let reader = OffsetLineReader::new(file, 0).ok()?;
    let mut cwd = String::new();
    let mut agent_title = indexed_title.to_string();
    let mut first_user = String::new();
    let mut started_at = String::new();
    let mut metrics = SessionMetrics::default();

    for (line, _) in reader {
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        metrics.observe_timestamp(v.get("timestamp").and_then(Value::as_str));
        let event_type = v.get("type").and_then(Value::as_str).unwrap_or("");
        let payload = v.get("payload").unwrap_or(&Value::Null);

        match event_type {
            "thread_name_updated" => {
                if let Some(name) = v.get("thread_name").and_then(Value::as_str) {
                    let cleaned = clean_user_message(name).unwrap_or_else(|| name.trim().to_string());
                    if !cleaned.is_empty() {
                        agent_title = cleaned;
                    }
                }
            }
            "message" if v.get("role").and_then(Value::as_str) == Some("user") && first_user.is_empty() => {
                if let Some(text) = codex_user_text(&v) {
                    if let Some(cleaned) = clean_user_message(&text) {
                        first_user = cleaned;
                    }
                }
            }
            "event_msg" if payload.get("type").and_then(Value::as_str) == Some("user_message") => {
                if let Some(text) = codex_event_user_text(payload) {
                    metrics.turn_count = metrics.turn_count.saturating_add(1);
                    if first_user.is_empty() {
                        if let Some(cleaned) = clean_user_message(&text) {
                            first_user = cleaned;
                        }
                    }
                }
            }
            "event_msg" if payload.get("type").and_then(Value::as_str) == Some("token_count") => {
                collect_codex_token_metrics(payload, &mut metrics);
            }
            "turn_context" => {
                if let Some(model) = payload.get("model").and_then(Value::as_str) {
                    if !model.trim().is_empty() {
                        metrics.model = model.trim().to_string();
                    }
                }
            }
            "response_item"
                if payload.get("type").and_then(Value::as_str) == Some("message")
                    && payload.get("role").and_then(Value::as_str) == Some("user") =>
            {
                if let Some(text) = codex_user_text(payload) {
                    if let Some(cleaned) = clean_user_message(&text) {
                        metrics.turn_count = metrics.turn_count.saturating_add(1);
                        if first_user.is_empty() {
                            first_user = cleaned;
                        }
                    }
                }
            }
            "response_item" => collect_codex_edit_metrics(payload, &mut metrics),
            _ => {}
        }

        if cwd.is_empty() {
            cwd = payload.get("cwd").and_then(Value::as_str)
                .or_else(|| v.get("cwd").and_then(Value::as_str))
                .unwrap_or("")
                .to_string();
        }
        if started_at.is_empty() {
            started_at = payload
                .get("timestamp")
                .and_then(Value::as_str)
                .or_else(|| v.get("timestamp").and_then(Value::as_str))
                .unwrap_or("")
                .to_string();
        }
    }

    let project = project_label(&cwd);
    let title = derive_title(&agent_title, &first_user, &project, &started_at);
    let last_active_at = mtime_iso(file);
    Some(Session {
        id: format!("codex:{raw_id}"),
        source: "codex".to_string(),
        cwd,
        project,
        title,
        custom_title: None,
        first_user_message: first_user,
        started_at,
        last_active_at,
        file_path: file.to_string_lossy().to_string(),
        codex_home: codex_home.to_string(),
        model: metrics.model.clone(),
        total_tokens: metrics.total_tokens(),
        total_cost_usd: metrics.cost("codex"),
        turn_count: metrics.turn_count,
        edit_count: metrics.edit_count,
        duration_seconds: metrics.duration_seconds,
    })
}

fn codex_event_user_text(payload: &Value) -> Option<String> {
    if let Some(message) = payload.get("message").and_then(Value::as_str) {
        let trimmed = message.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    let elements = payload.get("text_elements").and_then(Value::as_array)?;
    let text = elements
        .iter()
        .filter_map(|item| {
            item.as_str()
                .or_else(|| item.get("text").and_then(Value::as_str))
                .or_else(|| item.get("value").and_then(Value::as_str))
        })
        .collect::<Vec<_>>()
        .join("\n");
    if text.trim().is_empty() { None } else { Some(text) }
}

fn collect_codex_token_metrics(payload: &Value, metrics: &mut SessionMetrics) {
    let Some(usage) = payload
        .pointer("/info/total_token_usage")
        .or_else(|| payload.pointer("/info/last_token_usage"))
    else {
        return;
    };
    let raw_input = usage.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
    let cached = usage
        .get("cached_input_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    metrics.input_tokens = raw_input.saturating_sub(cached);
    metrics.cached_input_tokens = cached;
    metrics.cache_creation_input_tokens = usage
        .get("cache_creation_input_tokens")
        .or_else(|| usage.get("cache_write_input_tokens"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    metrics.output_tokens = usage.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
    metrics.reasoning_output_tokens = usage
        .get("reasoning_output_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
}

fn collect_codex_edit_metrics(payload: &Value, metrics: &mut SessionMetrics) {
    let kind = payload.get("type").and_then(Value::as_str).unwrap_or("");
    if !matches!(kind, "function_call" | "custom_tool_call") {
        return;
    }
    let name = payload.get("name").and_then(Value::as_str).unwrap_or("");
    if is_edit_tool(name) {
        metrics.edit_count = metrics.edit_count.saturating_add(1);
        return;
    }
    if name == "exec" {
        let input = payload.get("input").and_then(Value::as_str).unwrap_or("");
        if ["apply_patch", "edit", "write", "multiedit", "notebookedit"]
            .iter()
            .any(|tool| input.contains(&format!("tools.{tool}(")))
        {
            metrics.edit_count = metrics.edit_count.saturating_add(1);
        }
    }
}

/// Extract the plain-text content of a Codex user `message` line.
fn codex_user_text(v: &Value) -> Option<String> {
    let content = v.get("content")?;
    let blocks = content.as_array()?;
    let mut parts: Vec<&str> = Vec::new();
    for block in blocks {
        let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
        if matches!(block_type, "input_text" | "text" | "output_text") {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                parts.push(text);
            }
        }
    }
    let joined = parts.join("\n");
    if joined.trim().is_empty() { None } else { Some(joined) }
}

// --- Grok ------------------------------------------------------------------

fn scan_grok(home: &Path, cursor: &mut ScanCursor, out: &mut Vec<Session>, all_ids: &mut Vec<String>) {
    let base = home.join(".grok").join("sessions");
    if !base.exists() {
        return;
    }
    let pattern = format!("{}/**/summary.json", base.display());
    for file in glob_files(&pattern) {
        let Some(dir) = file.parent() else { continue };
        let raw_id = dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if raw_id.is_empty() {
            continue;
        }
        let path_str = file.to_string_lossy().to_string();
        all_ids.push(format!("grok:{raw_id}"));
        if !cursor.changed(&path_str) {
            continue;
        }
        if let Some(session) = parse_grok_session(&file, &raw_id) {
            out.push(session);
        }
    }
}

fn parse_grok_session(file: &Path, raw_id: &str) -> Option<Session> {
    let data = crate::parsers::utils::read_to_string_capped(file)?;
    let v: Value = serde_json::from_str(&data).ok()?;
    let info = v.get("info");
    let cwd = info.and_then(|i| i.get("cwd")).and_then(Value::as_str).unwrap_or("").to_string();
    let agent_title = v.get("generated_title").and_then(Value::as_str)
        .or_else(|| v.get("session_summary").and_then(Value::as_str))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| clean_user_message(s).unwrap_or_else(|| s.to_string()))
        .unwrap_or_default();
    let started_at = info.and_then(|i| i.get("created_at")).and_then(Value::as_str)
        .or_else(|| v.get("last_active_at").and_then(Value::as_str))
        .unwrap_or("")
        .to_string();
    let last_active_at = v.get("last_active_at").and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| mtime_iso(file));

    let project = project_label(&cwd);
    let title = derive_title(&agent_title, "", &project, &started_at);
    Some(Session {
        id: format!("grok:{raw_id}"),
        source: "grok".to_string(),
        cwd,
        project,
        title,
        custom_title: None,
        first_user_message: String::new(),
        started_at,
        last_active_at,
        file_path: file.to_string_lossy().to_string(),
        codex_home: String::new(),
        model: String::new(),
        total_tokens: 0,
        total_cost_usd: 0.0,
        turn_count: 0,
        edit_count: 0,
        duration_seconds: 0,
    })
}

// --- Additional agents ------------------------------------------------------
//
// These reuse the same path discovery as the token parsers (see
// `core/src/parsers/*.rs` and the shared `utils` path helpers) so a session
// source and its usage source can never drift apart. Only the *output* differs:
// the token parsers aggregate into 30-minute buckets and discard session
// identity, whereas these emit one row per conversation.

fn open_sqlite_readonly(path: &Path) -> Option<rusqlite::Connection> {
    rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
}

fn epoch_secs_to_iso(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

fn epoch_ms_to_iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

/// Build a `Session` with the metrics fields that the lighter scanners don't
/// yet collect left at zero.
#[allow(clippy::too_many_arguments)]
fn empty_session(
    id: String,
    source: &str,
    cwd: String,
    project: String,
    title: String,
    first_user_message: String,
    started_at: String,
    last_active_at: String,
    file_path: String,
    model: String,
    total_tokens: u64,
    turn_count: u32,
) -> Session {
    Session {
        id,
        source: source.to_string(),
        cwd,
        project,
        title,
        custom_title: None,
        first_user_message,
        started_at,
        last_active_at,
        file_path,
        codex_home: String::new(),
        model,
        total_tokens,
        total_cost_usd: 0.0,
        turn_count,
        edit_count: 0,
        duration_seconds: 0,
    }
}

// --- OpenCode (SQLite `session` table) --------------------------------------

fn scan_opencode(home: &Path, out: &mut Vec<Session>, all_ids: &mut Vec<String>) {
    let db_path = crate::parsers::utils::resolve_local_data_path(home, "opencode/opencode.db");
    if !db_path.exists() {
        return;
    }
    let Some(conn) = open_sqlite_readonly(&db_path) else { return };
    let Ok(mut stmt) = conn.prepare(
        "SELECT id, directory, title, time_created, time_updated, model,
                COALESCE(tokens_input,0) + COALESCE(tokens_output,0) + COALESCE(tokens_reasoning,0)
                + COALESCE(tokens_cache_read,0) + COALESCE(tokens_cache_write,0)
         FROM session WHERE time_archived IS NULL",
    ) else {
        return;
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, i64>(6)?,
        ))
    }) else {
        return;
    };
    for row in rows.flatten() {
        let (id, directory, title, created, updated, model, total) = row;
        all_ids.push(format!("opencode:{id}"));
        let project = project_label(&directory);
        let started_at = epoch_ms_to_iso(created);
        let last_active_at = epoch_ms_to_iso(updated);
        let title = derive_title(title.trim(), "", &project, &started_at);
        out.push(empty_session(
            format!("opencode:{id}"),
            "opencode",
            directory,
            project,
            title,
            String::new(),
            started_at,
            last_active_at,
            db_path.to_string_lossy().to_string(),
            model.unwrap_or_default(),
            total.max(0) as u64,
            0,
        ));
    }
}

// --- Kiro CLI (legacy JSON + v3 session.json) -------------------------------

fn scan_kiro(home: &Path, cursor: &mut ScanCursor, out: &mut Vec<Session>, all_ids: &mut Vec<String>) {
    let sessions_root = home.join(".kiro").join("sessions");
    if !sessions_root.exists() {
        return;
    }

    let legacy_pattern = format!("{}/cli/*.json", sessions_root.display());
    for file in glob_files(&legacy_pattern) {
        let raw_id = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        if raw_id.is_empty() {
            continue;
        }
        let path_str = file.to_string_lossy().to_string();
        all_ids.push(format!("kiro:{raw_id}"));
        if !cursor.changed(&path_str) {
            continue;
        }
        if let Some(session) = parse_kiro_legacy(&file, &raw_id) {
            out.push(session);
        }
    }

    let v3_pattern = format!("{}/*/sess_*/session.json", sessions_root.display());
    for file in glob_files(&v3_pattern) {
        let Some(dir_name) = file.parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string()) else {
            continue;
        };
        let raw_id = dir_name.strip_prefix("sess_").unwrap_or(&dir_name).to_string();
        let path_str = file.to_string_lossy().to_string();
        all_ids.push(format!("kiro:{raw_id}"));
        if !cursor.changed(&path_str) {
            continue;
        }
        if let Some(session) = parse_kiro_v3(&file, &raw_id) {
            out.push(session);
        }
    }
}

fn parse_kiro_legacy(file: &Path, raw_id: &str) -> Option<Session> {
    let data = crate::parsers::utils::read_to_string_capped(file)?;
    let v: Value = serde_json::from_str(&data).ok()?;
    let session_id = v.get("session_id").and_then(Value::as_str).unwrap_or(raw_id);
    let cwd = v.get("cwd").and_then(Value::as_str).unwrap_or("").to_string();
    let agent_title = v.get("title").and_then(Value::as_str).map(str::trim).unwrap_or("");
    let started_at = v.get("created_at").and_then(Value::as_str).unwrap_or("").to_string();
    let last_active_at = v.get("updated_at").and_then(Value::as_str).unwrap_or("").to_string();
    let model = v
        .pointer("/session_state/rts_model_state/model_info/model_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let project = project_label(&cwd);
    let title = derive_title(agent_title, "", &project, &started_at);
    let last_active_at = if last_active_at.is_empty() { mtime_iso(file) } else { last_active_at };
    Some(empty_session(
        format!("kiro:{session_id}"),
        "kiro",
        cwd,
        project,
        title,
        String::new(),
        started_at,
        last_active_at,
        file.to_string_lossy().to_string(),
        model,
        0,
        0,
    ))
}

fn parse_kiro_v3(file: &Path, raw_id: &str) -> Option<Session> {
    let data = crate::parsers::utils::read_to_string_capped(file)?;
    let v: Value = serde_json::from_str(&data).ok()?;
    let cwd = v
        .get("workspacePaths")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let agent_title = v.get("title").and_then(Value::as_str).map(str::trim).unwrap_or("");
    let started_at = v.get("createdAt").and_then(Value::as_str).unwrap_or("").to_string();
    let last_active_at = v.get("lastModifiedAt").and_then(Value::as_str).unwrap_or("").to_string();
    let model = v.get("modelId").and_then(Value::as_str).unwrap_or("").to_string();
    let project = project_label(&cwd);
    let title = derive_title(agent_title, "", &project, &started_at);
    let last_active_at = if last_active_at.is_empty() { mtime_iso(file) } else { last_active_at };
    Some(empty_session(
        format!("kiro:{raw_id}"),
        "kiro",
        cwd,
        project,
        title,
        String::new(),
        started_at,
        last_active_at,
        file.to_string_lossy().to_string(),
        model,
        0,
        0,
    ))
}

// --- GitHub Copilot (session-store.db) --------------------------------------

fn scan_copilot(home: &Path, out: &mut Vec<Session>, all_ids: &mut Vec<String>) {
    let db_path = crate::parsers::utils::vscode_global_storage(home)
        .join("github.copilot-chat")
        .join("session-store.db");
    if !db_path.exists() {
        return;
    }
    let Some(conn) = open_sqlite_readonly(&db_path) else { return };
    let Ok(mut stmt) = conn.prepare(
        "SELECT id, cwd, summary, created_at, updated_at FROM sessions",
    ) else {
        return;
    };
    let Ok(mut turn_stmt) = conn.prepare(
        "SELECT user_message FROM turns WHERE session_id = ?1 AND user_message IS NOT NULL AND user_message != '' ORDER BY turn_index LIMIT 1",
    ) else {
        return;
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    }) else {
        return;
    };
    for row in rows.flatten() {
        let (id, cwd, summary, created, updated) = row;
        all_ids.push(format!("copilot:{id}"));
        let cwd = cwd.unwrap_or_default();
        let project = project_label(&cwd);
        let agent_title = summary.unwrap_or_default();
        let started_at = created.unwrap_or_default();
        let last_active_at = updated.unwrap_or_default();
        let first_user = turn_stmt
            .query_row(params![id], |r| r.get::<_, Option<String>>(0))
            .ok()
            .flatten()
            .unwrap_or_default();
        let first_user_clean = clean_user_message(&first_user).unwrap_or_default();
        let title = derive_title(agent_title.trim(), &first_user_clean, &project, &started_at);
        out.push(empty_session(
            format!("copilot:{id}"),
            "copilot",
            cwd,
            project,
            title,
            first_user_clean,
            started_at,
            last_active_at,
            db_path.to_string_lossy().to_string(),
            String::new(),
            0,
            0,
        ));
    }
}

// --- Gemini CLI (chats/session-*.json) --------------------------------------

fn scan_gemini(home: &Path, cursor: &mut ScanCursor, out: &mut Vec<Session>, all_ids: &mut Vec<String>) {
    let base = home.join(".gemini").join("tmp");
    if !base.exists() {
        return;
    }
    let pattern = format!("{}/*/chats/session-*.json", base.display());
    for file in glob_files(&pattern) {
        let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let raw_id = stem.strip_prefix("session-").unwrap_or(&stem).to_string();
        let path_str = file.to_string_lossy().to_string();
        all_ids.push(format!("gemini:{raw_id}"));
        if !cursor.changed(&path_str) {
            continue;
        }
        if let Some(session) = parse_gemini_session(&file, &raw_id) {
            out.push(session);
        }
    }
}

fn parse_gemini_session(file: &Path, raw_id: &str) -> Option<Session> {
    let data = crate::parsers::utils::read_to_string_capped(file)?;
    let v: Value = serde_json::from_str(&data).ok()?;
    let messages = v.get("messages").and_then(Value::as_array)?;
    let mut model = String::new();
    let mut started_at = String::new();
    let mut last_active_at = String::new();
    let mut total_tokens = 0u64;
    for msg in messages {
        if let Some(tokens) = msg.get("tokens") {
            for key in ["input", "output", "tool", "cached", "thoughts"] {
                total_tokens += tokens.get(key).and_then(Value::as_u64).unwrap_or(0);
            }
        }
        if let Some(m) = msg.get("model").and_then(Value::as_str) {
            if !m.is_empty() {
                model = m.to_string();
            }
        }
        if let Some(ts) = msg.get("timestamp").and_then(Value::as_str) {
            if !ts.is_empty() {
                if started_at.is_empty() {
                    started_at = ts.to_string();
                }
                last_active_at = ts.to_string();
            }
        }
    }
    let title = derive_title("", "", "", &started_at);
    Some(empty_session(
        format!("gemini:{raw_id}"),
        "gemini",
        String::new(),
        String::new(),
        title,
        String::new(),
        started_at,
        last_active_at,
        file.to_string_lossy().to_string(),
        model,
        total_tokens,
        0,
    ))
}

// --- Hermes (state.db `sessions` table) -------------------------------------

fn scan_hermes(home: &Path, out: &mut Vec<Session>, all_ids: &mut Vec<String>) {
    let db_path = home.join(".hermes").join("state.db");
    if !db_path.exists() {
        return;
    }
    let Some(conn) = open_sqlite_readonly(&db_path) else { return };
    let Ok(mut stmt) = conn.prepare(
        "SELECT id, model, started_at, ended_at, input_tokens, output_tokens, \
         cache_read_tokens, cache_write_tokens, reasoning_tokens FROM sessions",
    ) else {
        return;
    };
    let Ok(rows) = stmt.query_map([], |row| {
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
    }) else {
        return;
    };
    for row in rows.flatten() {
        let (id, model, started_at, ended_at, input, output, cache_read, cache_write, reasoning) = row;
        all_ids.push(format!("hermes:{id}"));
        let total = (input + output + cache_read + cache_write + reasoning).max(0) as u64;
        let started = epoch_secs_to_iso(started_at);
        let ended = ended_at.map(epoch_secs_to_iso).unwrap_or_else(|| started.clone());
        let title = derive_title("", "", "", &started);
        out.push(empty_session(
            format!("hermes:{id}"),
            "hermes",
            String::new(),
            String::new(),
            title,
            String::new(),
            started,
            ended,
            db_path.to_string_lossy().to_string(),
            model.unwrap_or_default(),
            total,
            0,
        ));
    }
}

// --- Title derivation ------------------------------------------------------

/// Derive a display title by precedence: agent title → first valid user task →
/// local project + time.
fn derive_title(agent_title: &str, first_user: &str, project: &str, started_at: &str) -> String {
    let agent_title = agent_title.trim();
    if !agent_title.is_empty() {
        return truncate(agent_title, 120);
    }
    let first_user = first_user.trim();
    if !first_user.is_empty() {
        return truncate(first_user, 120);
    }
    let project = project.trim();
    let time = local_time_label(started_at);
    match (project.is_empty(), time.is_empty()) {
        (false, false) => format!("{} · {}", project, time),
        (false, true) => project.to_string(),
        (true, false) => time,
        (true, true) => "Session".to_string(),
    }
}

/// Clean a raw user message into a title-usable string. Returns `None` when the
/// result is empty, system/AGENTS boilerplate, or a generic short phrase.
fn clean_user_message(raw: &str) -> Option<String> {
    let mut s = strip_embedded_block(raw, "<image", "</image>");
    s = strip_embedded_block(&s, "[image", "]");
    s = strip_uuids(&s);
    s = strip_noise_tokens(&s);
    // Collapse whitespace.
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_string();
    if collapsed.is_empty() {
        return None;
    }
    if is_system_or_agents(&collapsed) {
        return None;
    }
    if is_generic_phrase(&collapsed) {
        return None;
    }
    Some(truncate(&collapsed, 120))
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// True if the message is injected system/AGENTS boilerplate, not a real task.
fn is_system_or_agents(s: &str) -> bool {
    let lower = s.to_lowercase();
    [
        "you are a coding agent",
        "you are claude code",
        "you are codex",
        "you are chatgpt",
        "you are an ai",
        "you are a software",
        "you are an expert",
        "you are a senior",
        "<system-reminder>",
        "<extremely_important>",
        "<subagent-stop>",
        "claude.md",
        "agents.md",
        "gemini.md",
        "superpowers",
        "skill tool",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// True for generic short prompts that make poor titles.
fn is_generic_phrase(s: &str) -> bool {
    let lower = s.to_lowercase();
    let trimmed = lower
        .trim()
        .trim_matches(|c: char| c.is_ascii_punctuation() || "。，！？；：、…".contains(c));
    if trimmed.chars().count() <= 2 {
        return true;
    }
    if trimmed.chars().all(|c| c.is_ascii_digit() || c.is_whitespace()) {
        return true;
    }
    let generics = [
        "hi", "hello", "hey", "ok", "okay", "yes", "no", "thanks", "thank you", "thx",
        "continue", "go on", "done", "test", "help", "why", "what", "great", "good",
        "继续", "你好", "好的", "谢谢", "测试", "在吗", "嗯", "好", "发布新版本",
        "release a new version", "publish a new version",
    ];
    generics.contains(&trimmed)
}

/// Remove an embedded attachment block while keeping the user's text that
/// follows it. Codex serializes pasted images as `<image ...>…</image>`.
fn strip_embedded_block(input: &str, opening: &str, closing: &str) -> String {
    let mut result = input.to_string();
    loop {
        let lower = result.to_ascii_lowercase();
        let Some(start) = lower.find(opening) else { break };
        let end = if let Some(relative_end) = lower[start..].find(closing) {
            start + relative_end + closing.len()
        } else if let Some(relative_end) = lower[start..].find('>') {
            start + relative_end + 1
        } else {
            result.truncate(start);
            break;
        };
        result.replace_range(start..end, " ");
    }
    result
}

/// Remove UUID-shaped (`8-4-4-4-12` hex) tokens from a string.
fn strip_uuids(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0usize;
    while i < s.len() {
        if let Some(end) = uuid_end_at(bytes, i) {
            i = end;
            // Trim a trailing separator so "fix <uuid> and x" doesn't double-space.
            continue;
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn uuid_end_at(b: &[u8], start: usize) -> Option<usize> {
    // Boundary: not immediately preceded by a hex digit.
    if start > 0 && (b[start - 1] as char).is_ascii_hexdigit() {
        return None;
    }
    let segs = [8usize, 4, 4, 4, 12];
    let mut pos = start;
    for (k, &len) in segs.iter().enumerate() {
        for _ in 0..len {
            if pos >= b.len() || !(b[pos] as char).is_ascii_hexdigit() {
                return None;
            }
            pos += 1;
        }
        if k < segs.len() - 1 {
            if pos >= b.len() || b[pos] != b'-' {
                return None;
            }
            pos += 1;
        }
    }
    // Boundary: not immediately followed by a hex digit.
    if pos < b.len() && (b[pos] as char).is_ascii_hexdigit() {
        return None;
    }
    Some(pos)
}

/// Extract the last UUID-shaped token from an arbitrary string (used for Codex
/// rollout filenames).
fn extract_last_uuid(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut found: Option<String> = None;
    let mut i = 0usize;
    while i < s.len() {
        if let Some(end) = uuid_end_at(bytes, i) {
            found = Some(s[i..end].to_string());
            i = end;
        } else {
            let ch = s[i..].chars().next().unwrap();
            i += ch.len_utf8();
        }
    }
    found
}

/// Remove image blobs, data URLs, paths, and URLs token-by-token.
fn strip_noise_tokens(s: &str) -> String {
    s.split_whitespace()
        .filter(|tok| !noise_token(tok))
        .collect::<Vec<_>>()
        .join(" ")
}

fn noise_token(tok: &str) -> bool {
    if tok.is_empty() {
        return true;
    }
    let lower = tok.to_lowercase();
    lower.starts_with("data:image/")
        || lower.contains("base64")
        || lower.starts_with("<image")
        || lower.starts_with("![")
        || tok.starts_with('/')
        || tok.starts_with("~/")
        || tok.starts_with("./")
        || tok.starts_with("../")
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("file://")
        || (tok.len() >= 2
            && tok.as_bytes()[0].is_ascii_alphabetic()
            && tok.as_bytes()[1] == b':'
            && (tok.as_bytes().get(2) == Some(&b'\\') || tok.as_bytes().get(2) == Some(&b'/')))
}

/// Project label from a cwd: its directory name, or empty.
fn project_label(cwd: &str) -> String {
    let cwd = cwd.trim().trim_end_matches('/');
    if cwd.is_empty() {
        return String::new();
    }
    Path::new(cwd)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Format an ISO timestamp as a local `MM/DD HH:mm` label.
fn local_time_label(iso: &str) -> String {
    use chrono::{DateTime, Datelike, Local, Timelike};
    let dt = match DateTime::parse_from_rfc3339(iso) {
        Ok(dt) => dt.with_timezone(&Local),
        Err(_) => return String::new(),
    };
    format!("{:02}/{:02} {:02}:{:02}", dt.month(), dt.day(), dt.hour(), dt.minute())
}

fn mtime_iso(path: &Path) -> String {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .map(|t| {
            let dt: chrono::DateTime<chrono::Utc> = t.into();
            // Fixed format (no fractional seconds) so it parses cleanly in Swift.
            dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
        })
        .unwrap_or_default()
}

fn normalize(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_uuids_paths_images_and_generic_phrases() {
        assert_eq!(
            clean_user_message("fix the bug in /Users/wangsw/src/app 019dddf4-b360-7161-8d51-e36bb9c7e1a6"),
            Some("fix the bug in".to_string())
        );
        assert_eq!(
            clean_user_message("rename this <image> data:image/png;base64,AAAA and ~/foo/bar.png"),
            Some("rename this and".to_string())
        );
        assert_eq!(clean_user_message("hi"), None);
        assert_eq!(clean_user_message("继续"), None);
        assert_eq!(clean_user_message("   "), None);
    }

    #[test]
    fn rejects_system_and_agents_boilerplate() {
        assert_eq!(clean_user_message("You are a coding agent that helps..."), None);
        assert_eq!(clean_user_message("<system-reminder> do something"), None);
        assert_eq!(clean_user_message("read CLAUDE.md and AGENTS.md"), None);
    }

    #[test]
    fn derive_title_precedence() {
        assert_eq!(
            derive_title("Add login", "add a login page", "myapp", "2026-08-10T12:00:00Z"),
            "Add login"
        );
        assert_eq!(
            derive_title("", "add a login page", "myapp", "2026-08-10T12:00:00Z"),
            "add a login page"
        );
        let fallback = derive_title("", "", "myapp", "2026-08-10T12:00:00Z");
        assert!(fallback.starts_with("myapp · "), "fallback was {fallback}");
    }

    #[test]
    fn extracts_last_uuid_from_rollout_filename() {
        let name = "rollout-2026-04-30T18-34-54-019dddf4-b360-7161-8d51-e36bb9c7e1a6";
        assert_eq!(
            extract_last_uuid(name).as_deref(),
            Some("019dddf4-b360-7161-8d51-e36bb9c7e1a6")
        );
        assert_eq!(extract_last_uuid("no-uuid-here"), None);
    }
}
