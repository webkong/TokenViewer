use std::collections::HashMap;
use std::path::Path;

use crate::parsers;
use crate::storage::Database;

const SOURCES: &[&str] = &[
    "claude", "codex", "cursor", "gemini", "kiro", "opencode", "openclaw",
    "everycode", "hermes", "copilot", "kimi", "grok", "antigravity",
    "roocode", "kilocode", "kilocli", "zed", "goose", "ohmypi", "pi",
    "craft", "codebuddy",
];

pub struct SyncResult {
    pub providers_synced: u32,
    pub records_added: u32,
    pub errors: Vec<String>,
}

pub fn sync_all(db: &Database, home_dir: &Path) -> SyncResult {
    let mut cursors = HashMap::new();
    for &source in SOURCES {
        if let Ok(Some(c)) = db.get_cursor(source) {
            cursors.insert(source.to_string(), c.cursor_data);
        }
    }

    let results = parsers::parse_all(home_dir, &cursors);

    let mut providers_synced = 0u32;
    let mut records_added = 0u32;
    let mut errors = Vec::new();

    for result in results {
        if result.records.is_empty() {
            continue;
        }

        // All-or-nothing transaction per provider; on failure the cursor is not
        // advanced so the next sync retries (UPSERT makes retries idempotent).
        match db.upsert_usage_batch(&result.records) {
            Ok(()) => {
                records_added = records_added.saturating_add(result.records.len() as u32);
                if let Err(e) = db.set_cursor(&result.source, &result.new_cursor) {
                    errors.push(format!("{}: cursor update failed: {}", result.source, e));
                } else {
                    providers_synced += 1;
                }
            }
            Err(e) => {
                errors.push(format!("{}: {}", result.source, e));
            }
        }
    }

    SyncResult { providers_synced, records_added, errors }
}
