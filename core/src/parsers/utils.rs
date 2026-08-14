use std::collections::{HashMap, HashSet};
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
        eprintln!(
            "tokenviewer: skipping oversized file ({} bytes): {}",
            len,
            path.display()
        );
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

/// File modification time as a nanosecond-resolution monotonic-ish stamp.
/// The value is still stored in `u64` so existing cursor state remains valid,
/// but it distinguishes multiple writes inside the same second.
pub fn file_mtime_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs().saturating_mul(1_000_000_000) + u64::from(d.subsec_nanos()))
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
    /// Per-key seen IDs for dedup (capped). Kept as a Vec to preserve insertion
    /// order for oldest-first eviction; `seen_set` mirrors it for O(1) lookup.
    #[serde(default)]
    pub seen_ids: Vec<String>,
    /// O(1) membership mirror of `seen_ids`. Not serialized — rebuilt lazily
    /// from `seen_ids` on first `mark_seen` after deserialization.
    #[serde(skip)]
    #[serde(default)]
    seen_set: HashSet<String>,
    /// Per-file last mtime stamp for skip-if-unchanged optimization.
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
    /// Claude user-prompt buckets waiting for the following assistant usage
    /// record to provide the actual model. Persisted across incremental syncs.
    #[serde(default)]
    pub pending_conversation_buckets: HashMap<String, Vec<String>>,
    /// Codex fork rollouts replay the parent session before the first new turn.
    /// Maps rollout logical key -> child session UUID while that replay prefix
    /// is still being consumed.
    #[serde(default)]
    pub codex_fork_replay_pending: HashMap<String, String>,
    /// Per-directory mtime stamp — skip re-glob if dir unchanged.
    #[serde(default)]
    pub dir_mtimes: HashMap<String, u64>,
    /// Cached file lists per directory pattern.
    #[serde(default)]
    pub dir_files: HashMap<String, Vec<String>>,
    /// kiro-cli/data.sqlite3 incremental state: max `updated_at` (epoch ms) seen,
    /// used as the SQL-level filter to skip unchanged conversations.
    #[serde(default)]
    pub kiro_cli_updated_at: i64,
    /// kiro-cli per-conversation watermark: conversation_id -> max processed
    /// request timestamp (epoch ms). Turns at/below this are already counted.
    #[serde(default)]
    pub kiro_cli_conv_ts: HashMap<String, i64>,
    /// zcode incremental watermark: max processed started_at (epoch ms) and
    /// its tie-breaker id. Kept separate so other parsers do not pollute it.
    #[serde(default)]
    pub zcode_last_started_at: i64,
    #[serde(default)]
    pub zcode_last_id: Option<String>,
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
            out[i] = if cur[i] >= prev[i] {
                cur[i] - prev[i]
            } else {
                cur[i]
            };
        }
        self.snapshots.insert(key.to_string(), cur);
        out
    }

    /// Returns true if id was newly inserted (not seen before). Caps at 50k.
    /// Membership is O(1) via a lazily-rebuilt `HashSet`; insertion order is
    /// kept in `seen_ids` so eviction still drops the oldest entries.
    pub fn mark_seen(&mut self, id: &str) -> bool {
        if self.seen_set.len() != self.seen_ids.len() {
            // Freshly deserialized cursor: rebuild the set from the Vec.
            self.seen_set = self.seen_ids.iter().cloned().collect();
        }
        if !self.seen_set.insert(id.to_string()) {
            return false;
        }
        self.seen_ids.push(id.to_string());
        if self.seen_ids.len() > 50_000 {
            let drop = self.seen_ids.len() - 50_000;
            for evicted in self.seen_ids.drain(0..drop) {
                self.seen_set.remove(&evicted);
            }
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
            files
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect(),
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

/// Streaming line reader starting at a byte offset. Yields `(line, new_offset)`
/// pairs without materializing the whole file in memory — callers keep only
/// their aggregated state plus the final offset. Empty lines are skipped.
pub struct OffsetLineReader {
    reader: BufReader<File>,
    offset: u64,
    line: String,
}

impl OffsetLineReader {
    pub fn new(path: &Path, offset: u64) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(offset))?;
        Ok(Self {
            reader,
            offset,
            line: String::new(),
        })
    }
}

impl Iterator for OffsetLineReader {
    type Item = (String, u64);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.line.clear();
            let bytes_read = match self.reader.read_line(&mut self.line) {
                Ok(0) => return None,
                Ok(n) => n,
                Err(_) => return None,
            };
            self.offset += bytes_read as u64;
            let trimmed = self.line.trim();
            if !trimmed.is_empty() {
                return Some((trimmed.to_string(), self.offset));
            }
        }
    }
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

/// Ordered candidates for a CLI-style local-data DB, `rel` like
/// `opencode/opencode.db`.
///
/// On Windows the CLI keeps its DB under `%LOCALAPPDATA%`; the legacy
/// `~/.local/share/<rel>` layout stays as a fallback so existing installs and
/// injected test roots keep working. On macOS/Linux only the legacy layout is
/// returned, so parser behavior is unchanged.
pub fn local_data_candidates(home: &Path, rel: &str) -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        vec![
            home.join("AppData/Local").join(rel),
            home.join(".local/share").join(rel),
        ]
    }
    #[cfg(not(target_os = "windows"))]
    {
        vec![home.join(".local/share").join(rel)]
    }
}

/// First candidate that already exists on disk, else `None`.
pub fn first_existing(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|p| p.exists()).cloned()
}

/// Resolve the primary path for a CLI local-data DB: the first existing
/// candidate, else the legacy default. Callers keep their existing `!exists()`
/// check, which turns a missing path into an empty result (never an error).
pub fn resolve_local_data_path(home: &Path, rel: &str) -> PathBuf {
    let candidates = local_data_candidates(home, rel);
    first_existing(&candidates).unwrap_or_else(|| candidates.last().cloned().unwrap())
}

/// Resolve the first existing path among ordered relative candidates rooted at
/// `home`, falling back to the first candidate when none exist. Callers rely on
/// the returned path's own `.exists()` semantics to skip gracefully, so a
/// missing result is never an error. An empty candidate list returns `home`.
pub fn resolve_first_existing(home: &Path, rel_candidates: &[&str]) -> PathBuf {
    let paths: Vec<PathBuf> = rel_candidates.iter().map(|rel| home.join(rel)).collect();
    first_existing(&paths).unwrap_or_else(|| paths.first().cloned().unwrap_or_else(|| home.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn first_existing_picks_first_and_returns_none_when_empty() {
        let dir = TempDir::new().unwrap();
        let existing = dir.path().join("a.db");
        let missing = dir.path().join("b.db");
        fs::write(&existing, b"x").unwrap();

        assert_eq!(
            first_existing(&[missing.clone(), existing.clone()]),
            Some(existing)
        );
        assert_eq!(first_existing(&[missing.clone()]), None);
        assert_eq!(first_existing(&[]), None);
    }

    #[test]
    fn local_data_candidates_keep_legacy_layout_on_non_windows() {
        let dir = TempDir::new().unwrap();
        let rel = "opencode/opencode.db";
        let candidates = local_data_candidates(dir.path(), rel);
        #[cfg(not(target_os = "windows"))]
        assert_eq!(candidates, vec![dir.path().join(".local/share").join(rel)]);
        #[cfg(target_os = "windows")]
        assert_eq!(candidates.len(), 2);
    }

    #[test]
    fn resolve_local_data_path_falls_back_to_legacy_when_nothing_exists() {
        let dir = TempDir::new().unwrap();
        let rel = "opencode/opencode.db";
        let resolved = resolve_local_data_path(dir.path(), rel);
        assert_eq!(resolved, dir.path().join(".local/share").join(rel));
    }

    #[test]
    fn resolve_local_data_path_picks_first_existing_candidate() {
        let dir = TempDir::new().unwrap();
        let legacy = dir.path().join(".local/share/opencode/opencode.db");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, b"x").unwrap();

        let resolved = resolve_local_data_path(dir.path(), "opencode/opencode.db");
        assert_eq!(resolved, legacy);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn resolve_local_data_path_prefers_windows_local_appdata() {
        let dir = TempDir::new().unwrap();
        let win = dir.path().join("AppData/Local/opencode/opencode.db");
        fs::create_dir_all(win.parent().unwrap()).unwrap();
        fs::write(&win, b"x").unwrap();

        let resolved = resolve_local_data_path(dir.path(), "opencode/opencode.db");
        assert_eq!(resolved, win);
    }

    #[test]
    fn resolve_first_existing_picks_first_existing_and_falls_back_to_primary() {
        let dir = TempDir::new().unwrap();

        // Nothing exists -> falls back to the first (primary) candidate.
        let resolved = resolve_first_existing(dir.path(), &["roaming/dev", "config/dev"]);
        assert_eq!(resolved, dir.path().join("roaming/dev"));

        // Second candidate exists -> it wins over the primary.
        fs::create_dir_all(dir.path().join("config")).unwrap();
        fs::write(dir.path().join("config/dev"), b"x").unwrap();
        let resolved = resolve_first_existing(dir.path(), &["roaming/dev", "config/dev"]);
        assert_eq!(resolved, dir.path().join("config/dev"));

        // Empty candidate list -> returns the home directory itself.
        assert_eq!(
            resolve_first_existing(dir.path(), &[]),
            dir.path().to_path_buf()
        );
    }
}
