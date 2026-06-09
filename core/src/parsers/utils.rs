use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime, TimeZone, Timelike, Utc};
use serde_json::Value;

use crate::models::UsageRecord;

/// Round a UTC timestamp down to the nearest 30-minute bucket.
pub fn bucket_30min(ts: DateTime<Utc>) -> String {
    let minute = if ts.minute() < 30 { 0 } else { 30 };
    let bucketed = ts
        .date_naive()
        .and_hms_opt(ts.hour(), minute, 0)
        .expect("valid hour/minute for 30-min bucketing");
    format!("{}Z", bucketed.format("%Y-%m-%dT%H:%M:%S"))
}

/// Get current UTC time bucketed to 30 min.
pub fn now_bucket() -> String {
    bucket_30min(Utc::now())
}

/// Sanity bounds for epoch seconds: 2000-01-01 .. 2100-01-01.
const MIN_EPOCH_SECS: i64 = 946_684_800;
const MAX_EPOCH_SECS: i64 = 4_102_444_800;

/// Timestamp from epoch seconds to bucket string. Returns None for out-of-range values.
pub fn epoch_secs_to_bucket(secs: i64) -> Option<String> {
    if !(MIN_EPOCH_SECS..=MAX_EPOCH_SECS).contains(&secs) {
        return None;
    }
    Utc.timestamp_opt(secs, 0).single().map(bucket_30min)
}

/// Timestamp from epoch millis to bucket string. Returns None for out-of-range values.
pub fn epoch_millis_to_bucket(ms: i64) -> Option<String> {
    epoch_secs_to_bucket(ms / 1000)
}

/// Maximum size (bytes) for a single JSON/JSONL file we'll read fully into memory.
pub const MAX_FILE_SIZE: u64 = 100 * 1024 * 1024;

/// Read a file to string only if it is under MAX_FILE_SIZE. Returns None if too large or unreadable.
pub fn read_to_string_capped(path: &Path) -> Option<String> {
    let len = fs::metadata(path).ok()?.len();
    if len > MAX_FILE_SIZE {
        eprintln!("tokenviewer: skipping oversized file ({} bytes): {}", len, path.display());
        return None;
    }
    fs::read_to_string(path).ok()
}

/// Try to parse an ISO 8601 string to a bucket.
pub fn iso_to_bucket(s: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        bucket_30min(dt.with_timezone(&Utc))
    } else if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        bucket_30min(Utc.from_utc_datetime(&dt))
    } else {
        now_bucket()
    }
}

/// File modification time as bucket.
pub fn file_mtime_bucket(path: &Path) -> String {
    if let Ok(meta) = fs::metadata(path) {
        if let Ok(mtime) = meta.modified() {
            let dt: DateTime<Utc> = mtime.into();
            return bucket_30min(dt);
        }
    }
    now_bucket()
}

/// File modification time as epoch seconds.
pub fn file_mtime_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// File inode number (unix). Returns 0 if unavailable / non-unix.
pub fn file_inode(path: &Path) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(path).map(|m| m.ino()).unwrap_or(0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        0
    }
}

/// Cursor state for tracking file offsets.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct FileCursor {
    pub offsets: HashMap<String, u64>,
    #[serde(default)]
    pub last_timestamp: Option<String>,
    /// Per-key cumulative snapshots [input, output, cache_read, cache_write, reasoning].
    /// Used by cumulative-total sources to emit only the delta each sync.
    #[serde(default)]
    pub snapshots: HashMap<String, [u64; 5]>,
    /// Per-key seen IDs for dedup (capped).
    #[serde(default)]
    pub seen_ids: Vec<String>,
    /// Per-file last mtime (epoch secs) for skip-if-unchanged optimization.
    #[serde(default)]
    pub mtimes: HashMap<String, u64>,
    /// Per-file inode at the time `offset` was recorded. If the inode changes
    /// (file truncated/recreated/rotated), the stored offset is invalidated.
    #[serde(default)]
    pub inodes: HashMap<String, u64>,
    /// Per-file last known model context for parsers that need stateful
    /// metadata across incremental reads.
    #[serde(default)]
    pub last_models: HashMap<String, String>,
    /// Per-file last known provider context for parsers that need stateful
    /// metadata across incremental reads.
    #[serde(default)]
    pub last_providers: HashMap<String, String>,
    /// Per-directory mtime (epoch secs) — skip re-glob if dir unchanged.
    #[serde(default)]
    pub dir_mtimes: HashMap<String, u64>,
    /// Cached file lists per directory pattern.
    #[serde(default)]
    pub dir_files: HashMap<String, Vec<String>>,
}

impl FileCursor {
    pub fn from_json(data: Option<&str>) -> Self {
        data.and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    pub fn get_offset(&self, path: &str) -> u64 {
        let stored = self.offsets.get(path).copied().unwrap_or(0);
        if stored == 0 {
            return 0;
        }
        // If the file's inode changed since we recorded the offset, the file was
        // truncated/recreated/rotated — reset to read from the start.
        let cur = file_inode(Path::new(path));
        match self.inodes.get(path) {
            Some(&recorded) if cur != 0 && recorded != 0 && recorded != cur => 0,
            _ => stored,
        }
    }

    pub fn set_offset(&mut self, path: &str, offset: u64) {
        self.offsets.insert(path.to_string(), offset);
        let ino = file_inode(Path::new(path));
        if ino != 0 {
            self.inodes.insert(path.to_string(), ino);
        }
    }

    /// Given a key and the current cumulative totals, return the delta vs. the
    /// stored snapshot and update the snapshot. Resets (current < snapshot) emit
    /// the full current value.
    pub fn delta(&mut self, key: &str, cur: [u64; 5]) -> [u64; 5] {
        let prev = self.snapshots.get(key).copied().unwrap_or([0; 5]);
        let mut out = [0u64; 5];
        for i in 0..5 {
            out[i] = if cur[i] >= prev[i] { cur[i] - prev[i] } else { cur[i] };
        }
        self.snapshots.insert(key.to_string(), cur);
        out
    }

    /// Returns true if id was newly inserted (not seen before). Caps at 50k.
    pub fn mark_seen(&mut self, id: &str) -> bool {
        if self.seen_ids.iter().any(|s| s == id) {
            return false;
        }
        self.seen_ids.push(id.to_string());
        if self.seen_ids.len() > 50_000 {
            let drop = self.seen_ids.len() - 50_000;
            self.seen_ids.drain(0..drop);
        }
        true
    }

    /// Returns true if the file has been modified since last recorded mtime.
    /// Also updates the stored mtime. If file cannot be stat'd, returns true (assume changed).
    pub fn file_changed(&mut self, path: &str) -> bool {
        let mtime = file_mtime_secs(Path::new(path));
        let last = self.mtimes.get(path).copied().unwrap_or(0);
        if mtime > last {
            self.mtimes.insert(path.to_string(), mtime);
            true
        } else {
            false
        }
    }

    /// Glob files with directory-level caching. If no subdirectory mtime
    /// has changed, return the cached file list instead of re-globbing.
    pub fn glob_cached(&mut self, pattern: &str, dir: &Path) -> Vec<std::path::PathBuf> {
        let dir_key = dir.to_string_lossy().to_string();
        let max_mtime = max_subtree_mtime(dir);
        let cached_mtime = self.dir_mtimes.get(&dir_key).copied().unwrap_or(0);

        if max_mtime <= cached_mtime {
            // Dir tree unchanged — return cached list
            if let Some(cached) = self.dir_files.get(pattern) {
                return cached.iter().map(std::path::PathBuf::from).collect();
            }
        }

        // Re-glob
        let files = glob_files(pattern);
        self.dir_mtimes.insert(dir_key, max_mtime);
        self.dir_files.insert(
            pattern.to_string(),
            files.iter().map(|p| p.to_string_lossy().to_string()).collect(),
        );
        files
    }
}

/// Read new lines from a file starting at the given byte offset.
/// Returns (lines, new_offset).
pub fn read_lines_from_offset(path: &Path, offset: u64) -> std::io::Result<(Vec<String>, u64)> {
    let file = File::open(path)?;
    let file_len = file.metadata()?.len();
    if offset >= file_len {
        return Ok((vec![], offset));
    }
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(offset))?;
    let mut lines = Vec::new();
    let mut current_offset = offset;
    let mut line = String::new();
    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line)?;
        if bytes_read == 0 {
            break;
        }
        current_offset += bytes_read as u64;
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            lines.push(trimmed.to_string());
        }
    }
    Ok((lines, current_offset))
}

/// Aggregate records by (hour_start, source, model) key.
pub fn aggregate_records(records: Vec<UsageRecord>) -> Vec<UsageRecord> {
    let mut map: HashMap<(String, String, String), UsageRecord> = HashMap::new();
    for r in records {
        let key = (r.hour_start.clone(), r.source.clone(), r.model.clone());
        let entry = map.entry(key).or_insert_with(|| UsageRecord {
            id: None,
            hour_start: r.hour_start.clone(),
            source: r.source.clone(),
            model: r.model.clone(),
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            cache_creation_input_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 0,
            conversation_count: 0,
        });
        entry.input_tokens += r.input_tokens;
        entry.output_tokens += r.output_tokens;
        entry.cached_input_tokens += r.cached_input_tokens;
        entry.cache_creation_input_tokens += r.cache_creation_input_tokens;
        entry.reasoning_output_tokens += r.reasoning_output_tokens;
        entry.total_tokens += r.total_tokens;
        entry.conversation_count += r.conversation_count;
    }
    map.into_values().collect()
}

/// Walk a directory tree and return the maximum mtime (epoch secs) of any entry.
fn max_subtree_mtime(dir: &Path) -> u64 {
    let mut max_mt = file_mtime_secs(dir);
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return max_mt,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let mt = if path.is_dir() {
            max_subtree_mtime(&path)
        } else {
            file_mtime_secs(&path)
        };
        if mt > max_mt {
            max_mt = mt;
        }
    }
    max_mt
}

/// Glob for files matching a pattern relative to a base directory.
pub fn glob_files(pattern: &str) -> Vec<PathBuf> {
    glob::glob(pattern)
        .map(|paths| paths.filter_map(|p| p.ok()).collect())
        .unwrap_or_default()
}

/// Parse a JSONL file for records using a custom line parser function.
/// Returns records and the new file offset.
pub fn parse_jsonl_file<F>(
    path: &Path,
    offset: u64,
    source: &str,
    line_parser: F,
) -> (Vec<UsageRecord>, u64)
where
    F: Fn(&Value, &str) -> Option<UsageRecord>,
{
    match read_lines_from_offset(path, offset) {
        Ok((lines, new_offset)) => {
            let records: Vec<UsageRecord> = lines
                .iter()
                .filter_map(|line| {
                    let v: Value = serde_json::from_str(line).ok()?;
                    line_parser(&v, source)
                })
                .collect();
            (records, new_offset)
        }
        Err(_) => (vec![], offset),
    }
}

/// Get the VS Code extensions globalStorage path.
pub fn vscode_global_storage(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home.join("Library/Application Support/Code/User/globalStorage")
    }
    #[cfg(target_os = "linux")]
    {
        home.join(".config/Code/User/globalStorage")
    }
    #[cfg(target_os = "windows")]
    {
        home.join("AppData/Roaming/Code/User/globalStorage")
    }
}
