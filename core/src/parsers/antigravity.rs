use serde_json::Value;
use std::path::Path;

use super::utils::*;
use crate::models::UsageRecord;

pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let gemini_dir = home_dir.join(".gemini");
    if !gemini_dir.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let mut cursor = FileCursor::from_json(cursor_data);
    let mut all_records = Vec::new();

    // Scan multiple antigravity directories
    let dirs = ["antigravity", "antigravity-ide", "antigravity-cli"];
    for dir in &dirs {
        let pattern = format!("{}/{}/brain/**/transcript.jsonl", gemini_dir.display(), dir);
        let base = gemini_dir.join(dir);
        let files = cursor.glob_cached(&pattern, &base);
        for file in files {
            let key = file.to_string_lossy().to_string();
            // Older versions advanced the byte offset past these previously
            // unrecognized records. Rescan them once, without replaying legacy
            // nested usage records that were already counted.
            let scan_key = format!("antigravity:transcript-v1:{key}");
            let needs_backfill = !cursor.snapshots.contains_key(&scan_key);
            let previous_mtime = cursor.mtimes.get(&key).copied();
            if !cursor.file_changed(&key) && !needs_backfill {
                continue;
            }
            let reader = match OffsetLineReader::new(&file, 0) {
                Ok(reader) => reader,
                Err(_) => {
                    // A transient read failure must be retried on the next sync.
                    if let Some(mtime) = previous_mtime {
                        cursor.mtimes.insert(key.clone(), mtime);
                    } else {
                        cursor.mtimes.remove(&key);
                    }
                    continue;
                }
            };
            let offset = cursor.get_offset(&key);
            let bucket = file_mtime_bucket(&file);

            // Antigravity can rewrite a transcript while a step is running.
            // Identify each completed model step by its conversation file and
            // step index instead of assuming this format is append-only.
            for (line, _) in reader {
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                let Some((step_index, mut record)) = parse_model_step(&value) else {
                    continue;
                };
                let step_key = format!("antigravity:step-v1:{key}:{step_index}");
                if cursor.snapshots.contains_key(&step_key) {
                    continue;
                }
                if record.hour_start.is_empty() {
                    record.hour_start = bucket.clone();
                }
                cursor.snapshots.insert(step_key, [1, 0, 0, 0, 0]);
                all_records.push(record);
            }
            cursor.snapshots.insert(scan_key, [1, 0, 0, 0, 0]);

            // Preserve the old offset for legacy nested-usage transcripts.
            let (mut records, new_offset) =
                parse_jsonl_file(&file, offset, "antigravity", |v, src| {
                    parse_transcript_line(v, src)
                });
            for r in &mut records {
                if r.hour_start.is_empty() {
                    r.hour_start = bucket.clone();
                }
            }
            all_records.extend(records);
            cursor.set_offset(&key, new_offset);
        }
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}

fn parse_model_step(v: &Value) -> Option<(u64, UsageRecord)> {
    // A nested usage record was handled by the legacy parser; never replay it
    // as a top-level record during migration.
    if v.get("usageMetadata").is_some() || v.get("usage").is_some() {
        return None;
    }
    if v.get("source").and_then(Value::as_str) != Some("MODEL")
        || v.get("type").and_then(Value::as_str) != Some("PLANNER_RESPONSE")
    {
        return None;
    }
    if !matches!(
        v.get("status").and_then(Value::as_str),
        Some("DONE" | "ERROR" | "CANCELED" | "CANCELLED")
    ) {
        return None;
    }

    let step_index = v.get("step_index")?.as_u64()?;
    let input = v.get("input_tokens").and_then(Value::as_u64).unwrap_or(0);
    let output = v.get("output_tokens").and_then(Value::as_u64).unwrap_or(0);
    let cached = v
        .get("cache_read_tokens")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total = input.saturating_add(cached).saturating_add(output);
    if total == 0 {
        return None;
    }

    let hour_start = v
        .get("created_at")
        .and_then(Value::as_str)
        .map(iso_to_bucket)
        .unwrap_or_default();
    let model = v
        .get("model")
        .and_then(Value::as_str)
        .filter(|model| !model.is_empty())
        .unwrap_or("gemini")
        .to_string();
    Some((
        step_index,
        UsageRecord {
            id: None,
            hour_start,
            source: "antigravity".to_string(),
            model,
            project_key: String::new(),
            project_ref: String::new(),
            input_tokens: input,
            output_tokens: output,
            cached_input_tokens: cached,
            cache_creation_input_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: total,
            conversation_count: 1,
        },
    ))
}

fn parse_transcript_line(v: &Value, source: &str) -> Option<UsageRecord> {
    let usage = v.get("usageMetadata").or_else(|| v.get("usage"))?;
    let model = v
        .get("model")
        .and_then(|m| m.as_str())
        .unwrap_or("gemini")
        .to_string();
    let input = usage
        .get("promptTokenCount")
        .or(usage.get("input_tokens"))
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let output = usage
        .get("candidatesTokenCount")
        .or(usage.get("output_tokens"))
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let cached = usage
        .get("cachedContentTokenCount")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let total = usage
        .get("totalTokenCount")
        .and_then(|x| x.as_u64())
        .unwrap_or(input + output);
    if total == 0 {
        return None;
    }
    Some(UsageRecord {
        id: None,
        hour_start: String::new(),
        source: source.to_string(),
        model,
        project_key: String::new(),
        project_ref: String::new(),
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: cached,
        cache_creation_input_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: total,
        conversation_count: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn backfills_top_level_steps_without_replaying_legacy_records() {
        let home = tempfile::tempdir().unwrap();
        let logs = home
            .path()
            .join(".gemini/antigravity/brain/session-a/.system_generated/logs");
        fs::create_dir_all(&logs).unwrap();
        let transcript = logs.join("transcript.jsonl");
        let legacy = r#"{"model":"old","usage":{"input_tokens":50,"output_tokens":10}}"#;
        let done = r#"{"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-11-27T07:43:56Z","model":"gemini-4-neon-high","input_tokens":100,"cache_read_tokens":40,"output_tokens":20}"#;
        fs::write(&transcript, format!("{legacy}\n{done}\n")).unwrap();
        // The full transcript is a duplicate of the shortened transcript.
        fs::write(logs.join("transcript_full.jsonl"), format!("{done}\n")).unwrap();

        // Legacy cursors stored the paths returned by glob, whose separators
        // can differ from the fixture path joined above on Windows.
        let pattern = format!(
            "{}/.gemini/antigravity/brain/**/transcript.jsonl",
            home.path().display()
        );
        let files = glob_files(&pattern);
        assert_eq!(files.len(), 1);
        let key = files[0].to_string_lossy();
        let mut old_cursor = FileCursor::default();
        old_cursor.set_offset(&key, fs::metadata(&transcript).unwrap().len());
        old_cursor.file_changed(&key);

        let (records, cursor) = parse(home.path(), Some(&old_cursor.to_json())).unwrap();
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.hour_start, "2026-11-27T07:30:00Z");
        assert_eq!(record.model, "gemini-4-neon-high");
        assert_eq!(
            (
                record.input_tokens,
                record.cached_input_tokens,
                record.output_tokens
            ),
            (100, 40, 20)
        );
        assert_eq!(record.total_tokens, 160);
        assert!(parse(home.path(), Some(&cursor)).unwrap().0.is_empty());

        // Rewritten files can contain an unfinished earlier step and a later
        // completed step. Completing the former must not replay the latter.
        let running = r#"{"step_index":2,"source":"MODEL","type":"PLANNER_RESPONSE","status":"RUNNING","input_tokens":9,"output_tokens":1}"#;
        let completed = r#"{"step_index":3,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","input_tokens":7,"output_tokens":3}"#;
        sleep(Duration::from_millis(10));
        fs::write(
            &transcript,
            format!("{legacy}\n{done}\n{running}\n{completed}\n"),
        )
        .unwrap();
        let (records, cursor) = parse(home.path(), Some(&cursor)).unwrap();
        assert_eq!(records.iter().map(|r| r.total_tokens).sum::<u64>(), 10);

        let finished = running.replace("RUNNING", "DONE");
        sleep(Duration::from_millis(10));
        fs::write(
            &transcript,
            format!("{legacy}\n{done}\n{finished}\n{completed}\n"),
        )
        .unwrap();
        let (records, cursor) = parse(home.path(), Some(&cursor)).unwrap();
        assert_eq!(records.iter().map(|r| r.total_tokens).sum::<u64>(), 10);
        assert!(parse(home.path(), Some(&cursor)).unwrap().0.is_empty());
    }
}
