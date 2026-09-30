use std::collections::HashMap;
use std::path::Path;

use crate::parsers;
use crate::storage::Database;

pub struct SyncResult {
    pub agents_synced: u32,
    pub records_added: u32,
    pub errors: Vec<String>,
}

pub fn sync_all(db: &Database, home_dir: &Path) -> SyncResult {
    sync(db, home_dir, false)
}

pub fn rebuild_all(db: &Database, home_dir: &Path) -> SyncResult {
    sync(db, home_dir, true)
}

fn sync(db: &Database, home_dir: &Path, rebuild: bool) -> SyncResult {
    let mut summary = SyncResult {
        agents_synced: 0,
        records_added: 0,
        errors: Vec::new(),
    };
    match db.has_pending_usage_replace() {
        Ok(false) => {}
        result => {
            summary.errors.push(match result {
                Err(error) => format!("usage recovery check failed: {error}"),
                _ => "usage recovery pending; complete Device Sync recovery before syncing".into(),
            });
            return summary;
        }
    }
    let mut cursors = HashMap::new();
    if !rebuild {
        for source in parsers::all_parser_sources() {
            match db.get_cursor(source) {
                Ok(Some(cursor)) => {
                    cursors.insert(source.to_string(), cursor.cursor_data);
                }
                Ok(None) => {}
                Err(error) => summary
                    .errors
                    .push(format!("{source}: cursor read failed: {error}")),
            }
        }
        // A failed checkpoint read must never become an initial scan.
        if !summary.errors.is_empty() {
            return summary;
        }
    }

    let codex_homes = crate::codex_home::discover_with_database(db, home_dir, false);
    let mut results = parsers::parse_all_with_codex_homes(home_dir, &cursors, &codex_homes);
    let empty_cursor = parsers::utils::FileCursor::default().to_json();
    // A missing Agent is not an installation. Preserve meaningful empty
    // checkpoints without creating status entries for every registered parser.
    results.retain(|result| {
        result.error.is_some()
            || !result.records.is_empty()
            || cursors.contains_key(&result.source)
            || (result.new_cursor != "{}" && result.new_cursor != empty_cursor)
    });

    for result in &results {
        if let Some(error) = &result.error {
            summary.errors.push(format!("{}: {}", result.source, error));
        }
    }

    if rebuild {
        if !summary.errors.is_empty() {
            return summary;
        }
        let batches: Vec<_> = results
            .iter()
            .map(|result| {
                (
                    result.source.as_str(),
                    result.records.as_slice(),
                    result.new_cursor.as_str(),
                )
            })
            .collect();
        if let Err(error) = db.replace_processed_data(&batches) {
            summary.errors.push(format!("rebuild failed: {error}"));
            return summary;
        }
    }

    for result in results {
        if result.error.is_some() {
            continue;
        }
        // Empty scans may still carry offsets and pending metadata.
        if !rebuild {
            if let Err(error) =
                db.commit_usage_sync(&result.source, &result.records, &result.new_cursor)
            {
                summary.errors.push(format!("{}: {}", result.source, error));
                continue;
            }
        }
        if !result.records.is_empty() {
            summary.agents_synced += 1;
            summary.records_added = summary
                .records_added
                .saturating_add(result.records.len() as u32);
        }
    }
    summary
}
