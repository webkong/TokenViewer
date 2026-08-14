use serde_json::Value;
use std::path::Path;

use super::utils::*;
use crate::models::UsageRecord;

/// Parse DeepSeek Harness (`dsh`) session logs.
///
/// DSH persists each session as an append-only logical JSONL log under
/// `~/.dsh/sessions/<normalized-cwd>/<session-id>/session.jsonl.zstd`
/// (zstd-compressed concatenated frames by default) or `session.jsonl`
/// (`compression: 'none'`). The first logical line is the `session` header
/// (carrying the session `id`); every following line is a `SessionEvent`.
///
/// Token accounting lives on the `assistant/message` event's `data.usage` and
/// follows DSH's `TokenUsage` semantics — the buckets are DISJOINT:
///   * `inputTokens`    → uncached input (`input_tokens`)
///   * `cacheReadTokens` → cached input (`cached_input_tokens`)
///   * `cacheWriteTokens`→ cache creation (`cache_creation_input_tokens`)
///   * `outputTokens`   → output (`output_tokens`, already includes reasoning)
///   * `reasoningTokens`→ informational only (subset of output; never added)
/// `total = input + cacheRead + cacheWrite + output`. The model comes from
/// `data.message.source.model`, falling back to the latest `request/header`.
///
/// Dedup is by `session_id + seq` via `mark_seen` (the compressed artifact has
/// no stable byte→line mapping, so offsets are not used; unchanged files are
/// skipped by mtime).
pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let base = home_dir.join(".dsh").join("sessions");
    if !base.exists() {
        return Ok((vec![], cursor_data.unwrap_or("{}").to_string()));
    }

    let mut cursor = FileCursor::from_json(cursor_data);
    let mut all_records = Vec::new();

    for suffix in [".jsonl.zstd", ".jsonl"] {
        let pattern = format!("{}/**/*{}", base.display(), suffix);
        let files = cursor.glob_cached(&pattern, &base);
        for file in files {
            let key = file.to_string_lossy().to_string();

            // Skip unchanged files, but only commit the mtime after a successful
            // read so a torn final frame (mid-write) is retried on the next sync.
            let mtime = file_mtime_secs(&file);
            let last = cursor.mtimes.get(&key).copied().unwrap_or(0);
            if mtime <= last {
                continue;
            }

            let lines = match read_dsh_lines(&file) {
                Ok(l) => l,
                Err(_) => continue,
            };
            cursor.mtimes.insert(key.clone(), mtime);

            let mut session_id = String::new();
            let mut header_model: Option<String> = None;

            for line in &lines {
                let v: Value = match serde_json::from_str(line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let Some(ty) = v.get("type").and_then(|t| t.as_str()) else {
                    continue;
                };

                match ty {
                    "session" => {
                        if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
                            session_id = id.to_string();
                        }
                    }
                    "request/header" => {
                        if let Some(m) = v
                            .pointer("/data/header/config/model")
                            .and_then(|x| x.as_str())
                        {
                            header_model = Some(m.to_string());
                        }
                    }
                    "assistant/message" => {
                        let Some(usage) = v.pointer("/data/usage").and_then(|u| u.as_object())
                        else {
                            continue;
                        };

                        let seq = v.get("seq").and_then(|x| x.as_i64()).unwrap_or(0);
                        let dedup_id = format!("{}#{}", session_id, seq);
                        if !cursor.mark_seen(&dedup_id) {
                            continue;
                        }

                        let input =
                            usage.get("inputTokens").and_then(|x| x.as_u64()).unwrap_or(0);
                        let output =
                            usage.get("outputTokens").and_then(|x| x.as_u64()).unwrap_or(0);
                        let cache_read = usage
                            .get("cacheReadTokens")
                            .and_then(|x| x.as_u64())
                            .unwrap_or(0);
                        let cache_write = usage
                            .get("cacheWriteTokens")
                            .and_then(|x| x.as_u64())
                            .unwrap_or(0);
                        let reasoning = usage
                            .get("reasoningTokens")
                            .and_then(|x| x.as_u64())
                            .unwrap_or(0);

                        let total = input + output + cache_read + cache_write;
                        if total == 0 {
                            continue;
                        }

                        let model = v
                            .pointer("/data/message/source/model")
                            .and_then(|m| m.as_str())
                            .map(|s| s.to_string())
                            .or_else(|| header_model.clone())
                            .unwrap_or_else(|| "dsh-agent".to_string());

                        let hour_start = v
                            .get("time")
                            .and_then(|t| t.as_i64())
                            .and_then(epoch_millis_to_bucket)
                            .unwrap_or_else(|| file_mtime_bucket(&file));

                        all_records.push(UsageRecord {
                            id: None,
                            hour_start,
                            source: "dsh".to_string(),
                            model,
                            input_tokens: input,
                            output_tokens: output,
                            cached_input_tokens: cache_read,
                            cache_creation_input_tokens: cache_write,
                            reasoning_output_tokens: reasoning,
                            total_tokens: total,
                            conversation_count: 1,
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}

/// Read a DSH session artifact into logical lines, transparently decompressing
/// the default zstd (concatenated-frames) representation.
fn read_dsh_lines(path: &Path) -> std::io::Result<Vec<String>> {
    let raw = std::fs::read(path)?;
    if raw.len() as u64 > MAX_FILE_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("oversized file ({} bytes)", raw.len()),
        ));
    }
    let bytes: Vec<u8> = if path.to_string_lossy().ends_with(".zstd") {
        zstd::stream::decode_all(raw.as_slice())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
    } else {
        raw
    };
    let text = String::from_utf8_lossy(&bytes);
    Ok(text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn usage_json(input: u64, output: u64, cache_read: u64, cache_write: u64, reasoning: u64) -> String {
        format!(
            r#"{{"type":"assistant/message","seq":1,"time":1786678145601,"data":{{"turn":1,"step":1,"message":{{"role":"assistant","content":[{{"type":"text","text":"hi"}}],"source":{{"kind":"model","provider":"deepseek-official","model":"deepseek-v4-pro"}}}},"usage":{{"inputTokens":{input},"outputTokens":{output},"cacheReadTokens":{cache_read},"cacheWriteTokens":{cache_write},"reasoningTokens":{reasoning}}}}}}}"#
        )
    }

    fn write_session_artifact(
        dir: &Path,
        name: &str,
        lines: &[&str],
        compress: bool,
    ) -> std::path::PathBuf {
        let path = dir.join(name);
        let text = lines.join("\n") + "\n";
        if compress {
            // DSH writes a concatenation of independent zstd frames (one
            // checksummed frame per append batch). Encode each logical line as
            // its own frame so decode_all must walk multiple concatenated frames.
            let mut encoded = Vec::new();
            for line in lines {
                encoded.extend_from_slice(
                    &zstd::stream::encode_all(format!("{line}\n").as_bytes(), 0).unwrap(),
                );
            }
            fs::write(&path, encoded).unwrap();
        } else {
            fs::write(&path, text.as_bytes()).unwrap();
        }
        path
    }

    #[test]
    fn parse_zstd_session_extracts_usage() {
        let dir = TempDir::new().unwrap();
        let home = dir.path();
        let sessions = home.join(".dsh/sessions/proj/sess-1");
        fs::create_dir_all(&sessions).unwrap();

        let header = r#"{"type":"session","version":0,"id":"session-abc","createdAt":1786678123061,"delegationDepth":0}"#;
        let req_header = r#"{"type":"request/header","seq":0,"time":1786678139519,"data":{"header":{"config":{"provider":"deepseek-official","model":"deepseek-v4-flash"}}}}"#;
        let msg = usage_json(100, 50, 20, 5, 10);

        write_session_artifact(&sessions, "session.jsonl.zstd", &[header, req_header, &msg], true);

        let (records, cursor) = parse(home, None).unwrap();
        assert_eq!(records.len(), 1);
        let r = &records[0];
        assert_eq!(r.source, "dsh");
        assert_eq!(r.model, "deepseek-v4-pro");
        assert_eq!(r.input_tokens, 100);
        assert_eq!(r.output_tokens, 50);
        assert_eq!(r.cached_input_tokens, 20);
        assert_eq!(r.cache_creation_input_tokens, 5);
        assert_eq!(r.reasoning_output_tokens, 10);
        assert_eq!(r.total_tokens, 175); // 100 + 50 + 20 + 5 (reasoning is inside output)

        // Second parse with the persisted cursor must not double-count.
        let (records2, _) = parse(home, Some(&cursor)).unwrap();
        assert!(records2.is_empty());
    }

    #[test]
    fn parse_uncompressed_session() {
        let dir = TempDir::new().unwrap();
        let home = dir.path();
        let sessions = home.join(".dsh/sessions/proj/sess-2");
        fs::create_dir_all(&sessions).unwrap();

        let header = r#"{"type":"session","version":0,"id":"session-def","createdAt":1786678123061,"delegationDepth":0}"#;
        let msg = usage_json(10, 20, 0, 0, 0);

        write_session_artifact(&sessions, "session.jsonl", &[header, &msg], false);

        let (records, _) = parse(home, None).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].model, "deepseek-v4-pro");
        assert_eq!(records[0].total_tokens, 30);
    }

    #[test]
    fn model_falls_back_to_request_header() {
        let dir = TempDir::new().unwrap();
        let home = dir.path();
        let sessions = home.join(".dsh/sessions/proj/sess-3");
        fs::create_dir_all(&sessions).unwrap();

        let header = r#"{"type":"session","version":0,"id":"session-ghi","createdAt":1786678123061,"delegationDepth":0}"#;
        let req_header = r#"{"type":"request/header","seq":0,"time":1786678139519,"data":{"header":{"config":{"provider":"deepseek-official","model":"deepseek-v4-flash"}}}}"#;
        // No model on the message source → falls back to header model.
        let msg = r#"{"type":"assistant/message","seq":1,"time":1786678145601,"data":{"turn":1,"step":1,"message":{"role":"assistant","content":[]},"usage":{"inputTokens":5,"outputTokens":5}}}"#;

        write_session_artifact(&sessions, "session.jsonl", &[header, req_header, msg], false);

        let (records, _) = parse(home, None).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].model, "deepseek-v4-flash");
    }

    #[test]
    fn parse_missing_dir_is_empty() {
        let dir = TempDir::new().unwrap();
        let (records, cursor) = parse(dir.path(), None).unwrap();
        assert!(records.is_empty());
        assert_eq!(cursor, "{}");
    }
}
