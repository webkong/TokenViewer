use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;

use crate::storage::Database;
use crate::sync;

pub struct CoreHandle {
    pub db: Database,
    pub db_path: PathBuf,
    pub home_dir: PathBuf,
}

/// Initialize the core with a database path. Returns null on failure.
/// home_dir is inferred as db_path's grandparent (e.g. ~/.tokenviewer/data.db → ~).
///
/// # Safety
/// `db_path` must be a valid, non-null, NUL-terminated C string pointer.
#[no_mangle]
pub extern "C" fn tt_init(db_path: *const c_char) -> *mut CoreHandle {
    if db_path.is_null() {
        return std::ptr::null_mut();
    }
    let path_str = match unsafe { CStr::from_ptr(db_path) }.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let path = PathBuf::from(path_str);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let home_dir = path
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".to_string()))
        });

    match Database::open(&path) {
        Ok(db) => Box::into_raw(Box::new(CoreHandle { db, db_path: path, home_dir })),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Sync all providers. Returns JSON: {"providers_synced": N, "records_added": N, "errors": [...]}
///
/// # Safety
/// `handle` must be a valid pointer returned by `tt_init`, or null (returns null).
#[no_mangle]
pub extern "C" fn tt_sync_all(handle: *mut CoreHandle) -> *mut c_char {
    let handle = match unsafe { handle.as_ref() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    let result = sync::sync_all(&handle.db, &handle.home_dir);
    let json = serde_json::json!({
        "providers_synced": result.providers_synced,
        "records_added": result.records_added,
        "errors": result.errors,
    });
    to_json_cstring(&json)
}

/// Clear processed data and immediately resync from raw sources.
/// Returns the same JSON shape as `tt_sync_all`.
///
/// # Safety
/// `handle` must be a valid pointer returned by `tt_init`, or null (returns null).
#[no_mangle]
pub extern "C" fn tt_rebuild_all(handle: *mut CoreHandle) -> *mut c_char {
    let handle = match unsafe { handle.as_ref() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };

    if let Err(e) = handle.db.clear_processed_data() {
        return to_json_cstring(&serde_json::json!({
            "providers_synced": 0,
            "records_added": 0,
            "errors": [format!("reset failed: {}", e)],
        }));
    }

    let result = sync::sync_all(&handle.db, &handle.home_dir);
    let json = serde_json::json!({
        "providers_synced": result.providers_synced,
        "records_added": result.records_added,
        "errors": result.errors,
    });
    to_json_cstring(&json)
}

/// Get provider status. Returns JSON array of provider info.
///
/// # Safety
/// `handle` must be a valid pointer returned by `tt_init`, or null (returns null).
#[no_mangle]
pub extern "C" fn tt_get_provider_status(handle: *mut CoreHandle) -> *mut c_char {
    let handle = match unsafe { handle.as_ref() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    let sources = [
        "claude", "codex", "cursor", "gemini", "kiro", "opencode", "openclaw",
        "everycode", "hermes", "copilot", "kimi", "grok", "antigravity",
        "roocode", "kilocode", "kilocli", "zed", "goose", "ohmypi", "pi",
        "craft", "codebuddy",
    ];

    let statuses: Vec<serde_json::Value> = sources
        .iter()
        .map(|&source| {
            let cursor = handle.db.get_cursor(source).ok().flatten();
            let last_sync = cursor.as_ref().map(|c| c.updated_at.clone());
            let record_count = handle.db.count_records_by_source(source).unwrap_or(0);
            serde_json::json!({
                "source": source,
                "installed": last_sync.is_some() || record_count > 0,
                "last_sync": last_sync,
                "record_count": record_count,
            })
        })
        .collect();

    to_json_cstring(&statuses)
}

/// Query usage summary for a date range. Returns JSON string (caller must free with tt_free_string).
///
/// # Safety
/// `handle` must be valid (from `tt_init`); `from`/`to` must be valid NUL-terminated C strings.
/// Returns null if any pointer is null or the query fails.
#[no_mangle]
pub extern "C" fn tt_query_summary(
    handle: *mut CoreHandle,
    from: *const c_char,
    to: *const c_char,
) -> *mut c_char {
    let (handle, from, to) = match unsafe { unpack_query_args(handle, from, to) } {
        Some(v) => v,
        None => return std::ptr::null_mut(),
    };
    match handle.db.query_summary(&from, &to) {
        Ok(mut summary) => {
            // Compute total cost from per-model aggregates.
            if let Ok(rows) = handle.db.aggregate_by_model(&from, &to) {
                summary.total_cost_usd = rows.iter().map(crate::pricing::compute_row_cost).sum();
            }
            to_json_cstring(&summary)
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Query daily usage. Returns JSON string.
///
/// # Safety
/// See `tt_query_summary`.
#[no_mangle]
pub extern "C" fn tt_query_daily(
    handle: *mut CoreHandle,
    from: *const c_char,
    to: *const c_char,
) -> *mut c_char {
    let (handle, from, to) = match unsafe { unpack_query_args(handle, from, to) } {
        Some(v) => v,
        None => return std::ptr::null_mut(),
    };
    match handle.db.query_daily(&from, &to) {
        Ok(mut data) => {
            // Compute per-day cost from (date, model) aggregates.
            if let Ok(rows) = handle.db.aggregate_by_day_model(&from, &to) {
                use std::collections::HashMap;
                let mut cost_by_date: HashMap<String, f64> = HashMap::new();
                for r in &rows {
                    *cost_by_date.entry(r.hour_start.clone()).or_insert(0.0) +=
                        crate::pricing::compute_row_cost(r);
                }
                for d in &mut data {
                    d.total_cost_usd = cost_by_date.get(&d.date).copied().unwrap_or(0.0);
                }
            }
            to_json_cstring(&data)
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Query hourly breakdown for a single day. Returns JSON string.
///
/// # Safety
/// See `tt_query_summary`.
#[no_mangle]
pub extern "C" fn tt_query_hourly(
    handle: *mut CoreHandle,
    from: *const c_char,
    to: *const c_char,
) -> *mut c_char {
    let (handle, from, to) = match unsafe { unpack_query_args(handle, from, to) } {
        Some(v) => v,
        None => return std::ptr::null_mut(),
    };
    match handle.db.query_hourly(&from, &to) {
        Ok(mut data) => {
            if let Ok(rows) = handle.db.aggregate_by_hour_model(&from, &to) {
                use std::collections::HashMap;
                let mut cost_by_hour: HashMap<String, f64> = HashMap::new();
                for r in &rows {
                    *cost_by_hour.entry(r.hour_start.clone()).or_insert(0.0) +=
                        crate::pricing::compute_row_cost(r);
                }
                for d in &mut data {
                    d.total_cost_usd = cost_by_hour.get(&d.date).copied().unwrap_or(0.0);
                }
            }
            to_json_cstring(&data)
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Query model breakdown. Returns JSON string.
///
/// # Safety
/// See `tt_query_summary`.
#[no_mangle]
pub extern "C" fn tt_query_model_breakdown(
    handle: *mut CoreHandle,
    from: *const c_char,
    to: *const c_char,
) -> *mut c_char {
    let (handle, from, to) = match unsafe { unpack_query_args(handle, from, to) } {
        Some(v) => v,
        None => return std::ptr::null_mut(),
    };
    match handle.db.aggregate_by_model(&from, &to) {
        Ok(rows) => {
            let grand_total: u64 = rows.iter().map(|r| r.total_tokens).sum();
            let entries: Vec<crate::models::ModelBreakdownEntry> = rows
                .iter()
                .map(|r| crate::models::ModelBreakdownEntry {
                    model: r.model.clone(),
                    source: r.source.clone(),
                    total_tokens: r.total_tokens,
                    total_cost_usd: crate::pricing::compute_row_cost(r),
                    percentage: if grand_total > 0 {
                        r.total_tokens as f64 / grand_total as f64 * 100.0
                    } else {
                        0.0
                    },
                })
                .collect();
            to_json_cstring(&entries)
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Query heatmap data. Returns JSON string.
///
/// # Safety
/// `handle` must be valid (from `tt_init`), or null (returns null).
#[no_mangle]
pub extern "C" fn tt_query_heatmap(handle: *mut CoreHandle, weeks: i32) -> *mut c_char {
    let handle = match unsafe { handle.as_ref() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    match handle.db.query_heatmap(weeks.max(1)) {
        Ok(data) => to_json_cstring(&data),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a string returned by any tt_* function.
///
/// # Safety
/// `ptr` must be a pointer previously returned by a tt_* function, or null.
#[no_mangle]
pub extern "C" fn tt_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe { drop(CString::from_raw(ptr)) };
    }
}

/// Destroy the core handle.
///
/// # Safety
/// `handle` must be a pointer returned by `tt_init`, or null. Must not be used afterward.
#[no_mangle]
pub extern "C" fn tt_destroy(handle: *mut CoreHandle) {
    if !handle.is_null() {
        unsafe { drop(Box::from_raw(handle)) };
    }
}

// --- Helpers ---

/// Validate and unpack the common (handle, from, to) query arguments.
/// Returns None if any pointer is null.
///
/// # Safety
/// Pointers must be valid or null.
unsafe fn unpack_query_args<'a>(
    handle: *mut CoreHandle,
    from: *const c_char,
    to: *const c_char,
) -> Option<(&'a CoreHandle, String, String)> {
    if from.is_null() || to.is_null() {
        return None;
    }
    let handle = handle.as_ref()?;
    let from = CStr::from_ptr(from).to_str().ok()?.to_string();
    let to = CStr::from_ptr(to).to_str().ok()?.to_string();
    Some((handle, from, to))
}

fn to_json_cstring<T: serde::Serialize>(value: &T) -> *mut c_char {
    match serde_json::to_string(value) {
        Ok(json) => CString::new(json).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut()),
        Err(_) => std::ptr::null_mut(),
    }
}
