use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use super::utils::*;
use crate::models::UsageRecord;

/// Hard cap on decompressed session bytes — the same limit as `MAX_FILE_SIZE`
/// — enforced as a **per-sync budget shared across every frame** of a session,
/// so a compressed artifact with many individually-small frames cannot expand
/// to gigabytes in one pass.
const MAX_DECOMPRESSED_BYTES: u64 = MAX_FILE_SIZE;
/// Zstandard frame magic (little-endian `0xFD2FB528`), used to locate complete
/// frame boundaries without decompressing them.
const ZSTD_MAGIC: u32 = 0xFD2F_B528;

/// Byte range of one structurally complete zstd frame.
#[derive(Debug, Clone, Copy)]
struct FrameRange {
    start: usize,
    end: usize,
}

/// Parse DeepSeek Harness (`dsh`) session logs.
///
/// DSH persists each session as an append-only logical JSONL log under
/// `~/.dsh/sessions/<normalized-cwd>/<session-id>/session.jsonl.zstd`
/// (zstd-compressed concatenated frames by default) or `session.jsonl`
/// (`compression: 'none'`). The first logical line is the `session` header;
/// every following line is a `SessionEvent`.
///
/// Token accounting lives on the `assistant/message` event's `data.usage` and
/// follows DSH's `TokenUsage` semantics — the buckets are DISJOINT:
///   * `inputTokens`     → uncached input (`input_tokens`)
///   * `cacheReadTokens` → cached input (`cached_input_tokens`)
///   * `cacheWriteTokens`→ cache creation (`cache_creation_input_tokens`)
///   * `outputTokens`    → output (`output_tokens`, already includes reasoning)
///   * `reasoningTokens` → informational only (subset of output; never added)
/// `total = input + cacheRead + cacheWrite + output`. The model comes from
/// `data.message.source.model`, falling back to the latest `request/header`.
///
/// Incremental state is a per-file **compressed byte offset** (persisted in
/// `FileCursor::offsets`, whose inode guard resets it if a session file is
/// recreated). Because DSH appends independent zstd frames and never rewrites a
/// flushed prefix, every byte before the offset is stable, so each sync reads
/// and decompresses only the newly appended suffix. Decompression is further
/// bounded by a per-sync budget shared across frames: when the budget runs out
/// mid-file, the offset before the unconsumed frame is persisted (without
/// committing the mtime) so the next sync resumes the backlog.
/// The latest `request/header` model is persisted in `FileCursor::last_models`.
/// Forked sessions declare `parentSession` and `seedLength` in their `session`
/// header. DSH copies all parent events through that sequence into the child
/// file, so usage at `seq <= seedLength` is inherited history and is skipped;
/// only the child's continuation is counted.
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

            // Skip unchanged files; commit the mtime only after the file is
            // fully drained (or definitively oversized) so transient failures
            // and budget-limited passes retry on the next sync.
            let mtime = file_mtime_secs(&file);
            let last = cursor.mtimes.get(&key).copied().unwrap_or(0);
            if mtime <= last {
                continue;
            }

            let offset = cursor.get_offset(&key);
            if offset == 0 {
                // A new/recreated file must rediscover its fork boundary from
                // the session header instead of inheriting stale cursor state.
                cursor.dsh_fork_seed_lengths.remove(&key);
            }
            let header_model = cursor.last_models.get(&key).cloned();
            let fork_seed_length = cursor.dsh_fork_seed_lengths.get(&key).copied();

            match stream_file(
                &file,
                offset,
                header_model,
                fork_seed_length,
                MAX_DECOMPRESSED_BYTES,
            ) {
                Ok(ScanOutcome::Done(scan) | ScanOutcome::BudgetExhausted(scan)) => {
                    let budget_exhausted = scan.budget_exhausted;
                    cursor.set_offset(&key, scan.new_offset);
                    if let Some(m) = scan.model {
                        cursor.last_models.insert(key.clone(), m);
                    }
                    if let Some(seed_length) = scan.fork_seed_length {
                        cursor
                            .dsh_fork_seed_lengths
                            .insert(key.clone(), seed_length);
                    } else {
                        cursor.dsh_fork_seed_lengths.remove(&key);
                    }
                    all_records.extend(scan.records);
                    // BudgetExhausted must NOT commit the mtime: the next sync
                    // resumes the backlog from `new_offset`.
                    if !budget_exhausted {
                        cursor.mtimes.insert(key.clone(), mtime);
                    }
                }
                Ok(ScanOutcome::Oversized) => {
                    // Oversized artifact: skip definitively for this revision.
                    cursor.mtimes.insert(key.clone(), mtime);
                }
                Err(_) => {
                    // Transient read failure: retry on the next sync.
                }
            }
        }
    }

    Ok((aggregate_records(all_records), cursor.to_json()))
}

/// Result of scanning one session artifact's new suffix.
#[derive(Debug)]
struct DshScan {
    records: Vec<UsageRecord>,
    /// Compressed byte offset just past the last fully-processed frame/line.
    new_offset: u64,
    /// Latest `request/header` model (persisted in `FileCursor::last_models`).
    model: Option<String>,
    /// Highest event sequence copied from a fork's parent session.
    fork_seed_length: Option<u64>,
    /// True when the per-sync decompression budget ran out before EOF.
    budget_exhausted: bool,
}

/// Outcome of one `stream_file` pass.
enum ScanOutcome {
    /// Processed to the current end of the stream.
    Done(DshScan),
    /// Per-sync decompression budget exhausted; resume from `scan.new_offset`.
    BudgetExhausted(DshScan),
    /// The compressed artifact exceeds the cap; skip this revision.
    Oversized,
}

/// Read and parse only the byte suffix of `file` at/after `offset`, decoding at
/// most `budget` decompressed bytes across all frames in this pass.
///
/// Returns:
/// * `Ok(ScanOutcome::Done(_))` — processed up to the current end of the stream.
/// * `Ok(ScanOutcome::BudgetExhausted(_))` — stopped before EOF because the
///   shared budget ran out; the scan's `new_offset` points at the unconsumed
///   frame so a later pass resumes exactly there.
/// * `Ok(ScanOutcome::Oversized)` — the artifact exceeds the compressed cap.
/// * `Err(_)` — transient read failure (retry on the next sync).
fn stream_file(
    file: &Path,
    offset: u64,
    header_model: Option<String>,
    initial_fork_seed_length: Option<u64>,
    budget: u64,
) -> std::io::Result<ScanOutcome> {
    let meta = std::fs::metadata(file)?;
    if meta.len() > MAX_FILE_SIZE {
        eprintln!(
            "tokenviewer: skipping oversized dsh session ({} bytes): {}",
            meta.len(),
            file.display()
        );
        return Ok(ScanOutcome::Oversized);
    }

    let start = offset.min(meta.len());
    let mut handle = std::fs::File::open(file)?;
    handle.seek(SeekFrom::Start(start))?;
    let mut suffix = Vec::new();
    handle.read_to_end(&mut suffix)?;

    if suffix.is_empty() {
        return Ok(ScanOutcome::Done(DshScan {
            records: Vec::new(),
            new_offset: start,
            model: header_model,
            fork_seed_length: initial_fork_seed_length,
            budget_exhausted: false,
        }));
    }

    let mut records = Vec::new();
    let mut model = header_model;
    let mut fork_seed_length = initial_fork_seed_length;
    let mut new_offset = start;

    if file.to_string_lossy().ends_with(".zstd") {
        let (frames, _torn) = scan_zstd_frames(&suffix);
        // Remaining decompression budget, shared across ALL frames of this
        // pass (not reset per frame).
        let mut remaining = budget;

        for frame in frames {
            let content = match decode_frame_bounded(&suffix[frame.start..frame.end], remaining) {
                Ok(c) => c,
                Err(e) => {
                    // Corrupt frame: skip it and keep going.
                    eprintln!(
                        "tokenviewer: skipping corrupt dsh frame in {}: {}",
                        file.display(),
                        e
                    );
                    new_offset = start + frame.end as u64;
                    continue;
                }
            };

            if content.len() as u64 > remaining {
                if remaining == budget {
                    // Fresh budget: the frame itself exceeds the absolute
                    // decompression cap. Skip it definitively and charge the
                    // decode so one pass never decodes many oversized frames.
                    eprintln!(
                        "tokenviewer: skipping oversized dsh frame (>{} bytes) in {}",
                        MAX_DECOMPRESSED_BYTES,
                        file.display()
                    );
                    remaining = 0;
                    new_offset = start + frame.end as u64;
                    continue;
                }
                // Budget exhausted mid-backlog: stop, keeping this frame's
                // offset so the next sync resumes from it.
                return Ok(ScanOutcome::BudgetExhausted(DshScan {
                    records,
                    new_offset: start + frame.start as u64,
                    model,
                    fork_seed_length,
                    budget_exhausted: true,
                }));
            }

            remaining -= content.len() as u64;
            parse_bytes(
                &content,
                file,
                &mut records,
                &mut model,
                &mut fork_seed_length,
            );
            new_offset = start + frame.end as u64;
        }
    } else {
        // Uncompressed: the file-size cap already bounds the total bytes, so
        // the shared budget is trivially satisfied.
        let consumed = parse_bytes(
            &suffix,
            file,
            &mut records,
            &mut model,
            &mut fork_seed_length,
        );
        new_offset = start + consumed;
    }

    Ok(ScanOutcome::Done(DshScan {
        records,
        new_offset,
        model,
        fork_seed_length,
        budget_exhausted: false,
    }))
}

/// Decode one complete zstd frame, bounding the decompressed output to
/// `limit + 1` bytes so callers can distinguish "fits the limit" (len <= limit)
/// from "exceeds the limit" (len == limit + 1) without unbounded allocation.
fn decode_frame_bounded(data: &[u8], limit: u64) -> std::io::Result<Vec<u8>> {
    let decoder = zstd::stream::read::Decoder::new(data)?;
    let mut content = Vec::new();
    decoder.take(limit + 1).read_to_end(&mut content)?;
    Ok(content)
}

/// Parse complete JSONL lines from `bytes` (each line terminated by `\n`),
/// emitting `assistant/message` usage records and tracking the latest
/// `request/header` model. Returns the number of bytes consumed — complete
/// lines only; a torn final line without a trailing newline is left unconsumed
/// for the next sync.
fn parse_bytes(
    bytes: &[u8],
    file: &Path,
    records: &mut Vec<UsageRecord>,
    header_model: &mut Option<String>,
    fork_seed_length: &mut Option<u64>,
) -> u64 {
    let mut start = 0usize;
    while let Some(rel) = bytes[start..].iter().position(|b| *b == b'\n') {
        let end = start + rel;
        let line = std::str::from_utf8(&bytes[start..end]).unwrap_or("");
        if !line.trim().is_empty() {
            if let Ok(v) = serde_json::from_str::<Value>(line.trim()) {
                parse_event(&v, file, records, header_model, fork_seed_length);
            }
        }
        start = end + 1;
    }
    start as u64
}

/// Handle one parsed session event.
fn parse_event(
    v: &Value,
    file: &Path,
    records: &mut Vec<UsageRecord>,
    header_model: &mut Option<String>,
    fork_seed_length: &mut Option<u64>,
) {
    let Some(ty) = v.get("type").and_then(|t| t.as_str()) else {
        return;
    };
    match ty {
        "session" => {
            let is_fork = v
                .get("parentSession")
                .and_then(Value::as_str)
                .is_some_and(|parent| !parent.is_empty());
            *fork_seed_length = if is_fork {
                v.get("seedLength").and_then(Value::as_u64)
            } else {
                None
            };
        }
        "request/header" => {
            if let Some(m) = v
                .pointer("/data/header/config/model")
                .and_then(|x| x.as_str())
            {
                *header_model = Some(m.to_string());
            }
        }
        "assistant/message" => {
            if fork_seed_length.is_some_and(|seed| {
                v.get("seq")
                    .and_then(Value::as_u64)
                    .is_some_and(|seq| seq <= seed)
            }) {
                return;
            }
            let Some(usage) = v.pointer("/data/usage").and_then(|u| u.as_object()) else {
                return;
            };

            let input = usage.get("inputTokens").and_then(|x| x.as_u64()).unwrap_or(0);
            let output = usage.get("outputTokens").and_then(|x| x.as_u64()).unwrap_or(0);
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

            let total_tokens = input + output + cache_read + cache_write;
            if total_tokens == 0 {
                return;
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
                .unwrap_or_else(|| file_mtime_bucket(file));

            records.push(UsageRecord {
                id: None,
                hour_start,
                source: "dsh".to_string(),
                model,
                project_key: String::new(),
                project_ref: String::new(),
                input_tokens: input,
                output_tokens: output,
                cached_input_tokens: cache_read,
                cache_creation_input_tokens: cache_write,
                reasoning_output_tokens: reasoning,
                total_tokens,
                conversation_count: 1,
            });
        }
        _ => {}
    }
}

/// Locate structurally complete zstd frames in `data` without decompressing
/// them. Mirrors DSH's `scanZstdFrames` (zstd-jsonl-session-logs). Returns the
/// complete frames and, when the final frame is torn, its start offset.
fn scan_zstd_frames(data: &[u8]) -> (Vec<FrameRange>, Option<usize>) {
    let mut frames = Vec::new();
    let mut offset = 0usize;

    while offset < data.len() {
        let start = offset;
        if data.len() - offset < 4 {
            return (frames, Some(start));
        }
        let magic = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        if magic != ZSTD_MAGIC {
            return (frames, Some(start));
        }
        offset += 4;

        if offset >= data.len() {
            return (frames, Some(start));
        }
        let descriptor = data[offset];
        offset += 1;
        if descriptor & 0x18 != 0 {
            // Reserved frame-header bits set → corrupt.
            return (frames, Some(start));
        }

        let content_size_flag = descriptor >> 6;
        let single_segment = (descriptor & 0x20) != 0;
        let checksum = (descriptor & 0x04) != 0;
        let dictionary_flag = descriptor & 0x03;
        let dictionary_bytes = if dictionary_flag == 3 {
            4usize
        } else {
            dictionary_flag as usize
        };
        let content_size_bytes = if content_size_flag == 0 {
            if single_segment {
                1
            } else {
                0
            }
        } else {
            1usize << content_size_flag
        };
        let remaining_header =
            (if single_segment { 0 } else { 1 }) + dictionary_bytes + content_size_bytes;
        if data.len() - offset < remaining_header {
            return (frames, Some(start));
        }
        offset += remaining_header;

        loop {
            if data.len() - offset < 3 {
                return (frames, Some(start));
            }
            let block_header =
                u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], 0]);
            offset += 3;
            let last_block = (block_header & 1) != 0;
            let block_type = (block_header >> 1) & 0x03;
            let block_size = block_header >> 3;
            if block_type == 0x03 {
                // Reserved block type → corrupt.
                return (frames, Some(start));
            }
            let payload_bytes = if block_type == 0x01 {
                1usize
            } else {
                block_size as usize
            };
            if data.len() - offset < payload_bytes {
                return (frames, Some(start));
            }
            offset += payload_bytes;
            if last_block {
                break;
            }
        }

        if checksum {
            if data.len() - offset < 4 {
                return (frames, Some(start));
            }
            offset += 4;
        }
        frames.push(FrameRange { start, end: offset });
    }

    (frames, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    fn usage_json(
        seq: u64,
        input: u64,
        output: u64,
        cache_read: u64,
        cache_write: u64,
        reasoning: u64,
    ) -> String {
        format!(
            r#"{{"type":"assistant/message","seq":{seq},"time":1786678145601,"data":{{"turn":1,"step":1,"message":{{"role":"assistant","content":[{{"type":"text","text":"hi"}}],"source":{{"kind":"model","provider":"deepseek-official","model":"deepseek-v4-pro"}}}},"usage":{{"inputTokens":{input},"outputTokens":{output},"cacheReadTokens":{cache_read},"cacheWriteTokens":{cache_write},"reasoningTokens":{reasoning}}}}}}}"#
        )
    }

    fn header_json(id: &str) -> String {
        format!(r#"{{"type":"session","version":0,"id":"{id}","createdAt":1786678123061,"delegationDepth":0}}"#)
    }

    fn fork_header_json(id: &str, parent_id: &str, seed_length: u64) -> String {
        format!(
            r#"{{"type":"session","version":0,"id":"{id}","createdAt":1786678123061,"parentSession":"{parent_id}","seedLength":{seed_length},"delegationDepth":0}}"#
        )
    }

    fn req_header_json(seq: u64, model: &str) -> String {
        format!(
            r#"{{"type":"request/header","seq":{seq},"time":1786678139519,"data":{{"header":{{"config":{{"provider":"deepseek-official","model":"{model}"}}}}}}}}"#
        )
    }

    /// Encode `data` as one checksummed zstd frame — DSH enables the frame
    /// content checksum (`ZSTD_c_checksumFlag`), so fixtures mirror the real
    /// on-disk format.
    fn encode_zstd_frame(data: &[u8]) -> Vec<u8> {
        let mut encoder = zstd::stream::Encoder::new(Vec::new(), 0).unwrap();
        encoder.include_checksum(true).unwrap();
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }

    /// Encode one logical line as its own zstd frame and append it — mirrors
    /// DSH's one-frame-per-append-batch concatenated layout.
    fn append_zstd_frame(path: &Path, line: &str) {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        let frame = encode_zstd_frame(format!("{line}\n").as_bytes());
        f.write_all(&frame).unwrap();
    }

    #[test]
    fn parse_zstd_session_extracts_usage() {
        let dir = TempDir::new().unwrap();
        let home = dir.path();
        let sessions = home.join(".dsh/sessions/proj/sess-1");
        fs::create_dir_all(&sessions).unwrap();

        let path = sessions.join("session.jsonl.zstd");
        append_zstd_frame(&path, &header_json("session-abc"));
        append_zstd_frame(&path, &req_header_json(0, "deepseek-v4-flash"));
        append_zstd_frame(&path, &usage_json(1, 100, 50, 20, 5, 10));

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
    fn incremental_append_counts_only_new_events() {
        let dir = TempDir::new().unwrap();
        let home = dir.path();
        let sessions = home.join(".dsh/sessions/proj/sess-incr");
        fs::create_dir_all(&sessions).unwrap();

        let path = sessions.join("session.jsonl.zstd");
        append_zstd_frame(&path, &header_json("session-incr"));
        append_zstd_frame(&path, &req_header_json(0, "deepseek-v4-pro"));
        append_zstd_frame(&path, &usage_json(1, 10, 10, 0, 0, 0));
        append_zstd_frame(&path, &usage_json(2, 10, 10, 0, 0, 0));

        let (records, cursor) = parse(home, None).unwrap();
        // Both messages share a bucket + model, so they aggregate into one row.
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].total_tokens, 40);

        // Append a third assistant message (new frame → new mtime).
        append_zstd_frame(&path, &usage_json(3, 10, 10, 0, 0, 0));

        let (records2, _) = parse(home, Some(&cursor)).unwrap();
        assert_eq!(records2.len(), 1);
        // Only the appended suffix is parsed — events 1 and 2 must not be
        // re-emitted (a full-history rescan would total 60 here).
        assert_eq!(records2[0].total_tokens, 20);
    }

    #[test]
    fn decompression_budget_is_shared_across_frames() {
        let dir = TempDir::new().unwrap();
        let sessions = dir.path().join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        let path = sessions.join("session.jsonl.zstd");

        let header = header_json("sess-budget");
        let msg1 = usage_json(1, 10, 10, 0, 0, 0);
        let msg2 = usage_json(2, 10, 10, 0, 0, 0);
        let msg3 = usage_json(3, 10, 10, 0, 0, 0);
        append_zstd_frame(&path, &header);
        append_zstd_frame(&path, &msg1);
        append_zstd_frame(&path, &msg2);
        append_zstd_frame(&path, &msg3);

        // Budget fits header + msg1 (+ their newlines) but is one byte short of
        // msg2, so the shared budget must span frame boundaries across passes.
        let budget = (header.len() + msg1.len() + 2) as u64;

        let mut offset = 0u64;
        let mut grand_total = 0u64;
        let mut exhausted_passes = 0usize;
        let mut done = false;

        for _ in 0..6 {
            match stream_file(&path, offset, None, None, budget).unwrap() {
                ScanOutcome::Done(scan) => {
                    grand_total += scan.records.iter().map(|r| r.total_tokens).sum::<u64>();
                    done = true;
                    break;
                }
                ScanOutcome::BudgetExhausted(scan) => {
                    exhausted_passes += 1;
                    grand_total += scan.records.iter().map(|r| r.total_tokens).sum::<u64>();
                    offset = scan.new_offset;
                }
                ScanOutcome::Oversized => panic!("unexpected oversize"),
            }
        }

        assert!(done, "backlog should drain within a few passes");
        assert!(exhausted_passes >= 1, "budget should be exhausted mid-backlog");
        // Every message counted exactly once despite the mid-file stops.
        assert_eq!(grand_total, 60);
    }

    #[test]
    fn torn_frame_is_not_consumed_and_repair_counts_once() {
        let dir = TempDir::new().unwrap();
        let home = dir.path();
        let sessions = home.join(".dsh/sessions/proj/sess-torn");
        fs::create_dir_all(&sessions).unwrap();

        let path = sessions.join("session.jsonl.zstd");
        append_zstd_frame(&path, &header_json("session-torn"));
        append_zstd_frame(&path, &usage_json(1, 10, 10, 0, 0, 0));

        let (records, cursor) = parse(home, None).unwrap();
        assert_eq!(records.len(), 1);

        // Simulate a torn final frame: append a full frame, then truncate the
        // file so only its 4-byte magic + 1-byte descriptor remain (clearly an
        // incomplete frame with no complete boundary).
        let frame_start = fs::metadata(&path).unwrap().len();
        append_zstd_frame(&path, &usage_json(2, 5, 5, 0, 0, 0));
        {
            let f = fs::OpenOptions::new().write(true).open(&path).unwrap();
            f.set_len(frame_start + 5).unwrap();
        }

        // The torn frame must be left unconsumed (no new records, offset stays
        // at the last complete frame boundary).
        let (records2, _) = parse(home, Some(&cursor)).unwrap();
        assert!(records2.is_empty());

        // Simulate DSH repair: truncate the torn tail, then append the repaired
        // batch as a complete frame.
        {
            let f = fs::OpenOptions::new().write(true).open(&path).unwrap();
            f.set_len(frame_start).unwrap();
        }
        append_zstd_frame(&path, &usage_json(2, 5, 5, 0, 0, 0));

        let (records3, _) = parse(home, Some(&cursor)).unwrap();
        assert_eq!(records3.len(), 1);
        assert_eq!(records3[0].total_tokens, 10);
    }

    #[test]
    fn parse_uncompressed_session() {
        let dir = TempDir::new().unwrap();
        let home = dir.path();
        let sessions = home.join(".dsh/sessions/proj/sess-2");
        fs::create_dir_all(&sessions).unwrap();

        let path = sessions.join("session.jsonl");
        fs::write(
            &path,
            format!(
                "{}\n{}\n",
                header_json("session-def"),
                usage_json(1, 10, 20, 0, 0, 0)
            ),
        )
        .unwrap();

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

        let path = sessions.join("session.jsonl");
        // No model on the message source → falls back to header model.
        let msg = r#"{"type":"assistant/message","seq":1,"time":1786678145601,"data":{"turn":1,"step":1,"message":{"role":"assistant","content":[]},"usage":{"inputTokens":5,"outputTokens":5}}}"#;
        fs::write(
            &path,
            format!(
                "{}\n{}\n{}\n",
                header_json("session-ghi"),
                req_header_json(0, "deepseek-v4-flash"),
                msg
            ),
        )
        .unwrap();

        let (records, _) = parse(home, None).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].model, "deepseek-v4-flash");
    }

    #[test]
    fn forked_session_skips_copied_parent_usage_and_counts_continuation() {
        let dir = TempDir::new().unwrap();
        let home = dir.path();
        let sessions = home.join(".dsh/sessions/proj");
        let parent_dir = sessions.join("session-parent");
        let child_dir = sessions.join("session-child");
        fs::create_dir_all(&parent_dir).unwrap();
        fs::create_dir_all(&child_dir).unwrap();

        let parent_path = parent_dir.join("session.jsonl.zstd");
        append_zstd_frame(&parent_path, &header_json("session-parent"));
        append_zstd_frame(&parent_path, &usage_json(1, 10, 10, 0, 0, 0));
        append_zstd_frame(&parent_path, &usage_json(2, 10, 10, 0, 0, 0));

        let child_path = child_dir.join("session.jsonl.zstd");
        append_zstd_frame(
            &child_path,
            &fork_header_json("session-child", "session-parent", 2),
        );
        // DSH physically copies the parent's prefix into the child file.
        append_zstd_frame(&child_path, &usage_json(1, 10, 10, 0, 0, 0));
        append_zstd_frame(&child_path, &usage_json(2, 10, 10, 0, 0, 0));
        // Only this post-fork event belongs to the child session.
        append_zstd_frame(&child_path, &usage_json(3, 5, 5, 0, 0, 0));

        let (records, cursor_json) = parse(home, None).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].total_tokens, 50); // parent 40 + child continuation 10
        assert_eq!(records[0].conversation_count, 3);

        let cursor = FileCursor::from_json(Some(&cursor_json));
        assert_eq!(
            cursor
                .dsh_fork_seed_lengths
                .get(&child_path.to_string_lossy().to_string()),
            Some(&2)
        );

        let (records2, _) = parse(home, Some(&cursor_json)).unwrap();
        assert!(records2.is_empty());
    }

    #[test]
    fn fork_boundary_survives_incremental_cursor() {
        let dir = TempDir::new().unwrap();
        let home = dir.path();
        let sessions = home.join(".dsh/sessions/proj/session-child");
        fs::create_dir_all(&sessions).unwrap();

        let path = sessions.join("session.jsonl.zstd");
        append_zstd_frame(
            &path,
            &fork_header_json("session-child", "session-parent", 2),
        );
        append_zstd_frame(&path, &usage_json(1, 10, 10, 0, 0, 0));
        append_zstd_frame(&path, &usage_json(2, 10, 10, 0, 0, 0));

        let (records, cursor_json) = parse(home, None).unwrap();
        assert!(records.is_empty(), "copied fork prefix must be skipped");

        append_zstd_frame(&path, &usage_json(3, 5, 5, 0, 0, 0));
        let (records2, _) = parse(home, Some(&cursor_json)).unwrap();
        assert_eq!(records2.len(), 1);
        assert_eq!(records2[0].total_tokens, 10);
    }

    #[test]
    fn parse_missing_dir_is_empty() {
        let dir = TempDir::new().unwrap();
        let (records, cursor) = parse(dir.path(), None).unwrap();
        assert!(records.is_empty());
        assert_eq!(cursor, "{}");
    }

    #[test]
    fn scan_frames_roundtrips_concatenated_frames() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&encode_zstd_frame(b"a\n"));
        buf.extend_from_slice(&encode_zstd_frame(b"bb\n"));
        buf.extend_from_slice(&encode_zstd_frame(b"ccc\n"));

        let (frames, torn) = scan_zstd_frames(&buf);
        assert_eq!(frames.len(), 3);
        assert!(torn.is_none());
        assert_eq!(frames[0].start, 0);
        assert_eq!(frames[2].end, buf.len());

        // Each frame decodes independently.
        for frame in &frames {
            let out = zstd::stream::decode_all(&buf[frame.start..frame.end]).unwrap();
            assert!(out.ends_with(b"\n"));
        }
    }
}
