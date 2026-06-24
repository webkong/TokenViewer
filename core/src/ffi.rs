use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;

use crate::storage::Database;
use crate::sync;

pub struct CoreHandle {
    pub db: Database,
    pub db_path: PathBuf,
    pub home_dir: PathBuf,
    pub skills: crate::skills::SkillsCore,
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
        Ok(db) => {
            let source_root = std::env::var("TOKENVIEWER_SKILLS_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home_dir.join(".agents").join("skills"));

            let skills = match crate::skills::SkillsCore::new(&db, source_root) {
                Ok(skills) => skills,
                Err(_) => return std::ptr::null_mut(),
            };

            Box::into_raw(Box::new(CoreHandle { db, db_path: path, home_dir, skills }))
        }
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

// --- Skills FFI ---

/// List all scanned skills. Returns JSON array.
///
/// # Safety
/// `handle` must be a valid pointer from `tt_init`, or null (returns null).
#[no_mangle]
pub extern "C" fn tt_skills_list(handle: *mut CoreHandle) -> *mut c_char {
    let handle = match unsafe { handle.as_ref() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    match handle.skills.scanner.scan_all() {
        Ok(skills) => to_json_cstring(&skills),
        Err(e) => to_json_cstring(&serde_json::json!({ "error": e.to_string() })),
    }
}

/// List all registered agents (builtin + custom). Returns JSON array.
///
/// # Safety
/// `handle` must be a valid pointer from `tt_init`, or null (returns null).
#[no_mangle]
pub extern "C" fn tt_skills_list_agents(handle: *mut CoreHandle) -> *mut c_char {
    let handle = match unsafe { handle.as_ref() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    let agents = handle.skills.registry.all();
    to_json_cstring(&agents)
}

/// Get git status of the skill source root. Returns JSON object.
///
/// # Safety
/// `handle` must be a valid pointer from `tt_init`, or null (returns null).
#[no_mangle]
pub extern "C" fn tt_skills_git_status(handle: *mut CoreHandle) -> *mut c_char {
    let handle = match unsafe { handle.as_ref() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    match &handle.skills.git {
        Some(git) => match git.get_status() {
            Ok(status) => to_json_cstring(&status),
            Err(e) => to_json_cstring(&serde_json::json!({ "error": e.to_string() })),
        },
        None => to_json_cstring(&crate::skills::models::GitStatusInfo::error("No git repository")),
    }
}

/// Organize a skill. Takes JSON: {"skill_id": "..."}. Returns JSON: {"ok": true/false, "error": "..."}
///
/// # Safety
/// `handle` must be valid; `json` must be a valid NUL-terminated C string.
#[no_mangle]
pub extern "C" fn tt_skills_organize(handle: *mut CoreHandle, json: *const c_char) -> *mut c_char {
    let handle = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return to_json_cstring(&crate::skills::models::SkillCommandResult::error("Null handle")),
    };

    #[derive(serde::Deserialize)]
    struct SkillIdReq { skill_id: String }

    let req: SkillIdReq = match unsafe { from_cstring_json(json) } {
        Ok(r) => r,
        Err(e) => return to_json_cstring(&crate::skills::models::SkillCommandResult::error(e)),
    };

    // Require an agent_id for organize; defaults to first compatible agent
    let agent_id = handle.skills.registry.all().first()
        .map(|a| a.id.clone())
        .unwrap_or_default();

    match handle.skills.organize_skill(&req.skill_id, &agent_id) {
        Ok(()) => to_json_cstring(&crate::skills::models::SkillCommandResult::ok()),
        Err(e) => to_json_cstring(&crate::skills::models::SkillCommandResult::error(e)),
    }
}

/// Delete a skill directory from source_root. Takes JSON: {"skill_id": "..."}. Returns JSON: {"ok": true/false, "error": "..."}
///
/// # Safety
/// `handle` must be valid; `json` must be a valid NUL-terminated C string.
#[no_mangle]
pub extern "C" fn tt_skills_delete(handle: *mut CoreHandle, json: *const c_char) -> *mut c_char {
    let handle = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return to_json_cstring(&crate::skills::models::SkillCommandResult::error("Null handle")),
    };

    #[derive(serde::Deserialize)]
    struct SkillIdReq { skill_id: String }

    let req: SkillIdReq = match unsafe { from_cstring_json(json) } {
        Ok(r) => r,
        Err(e) => return to_json_cstring(&crate::skills::models::SkillCommandResult::error(e)),
    };

    match handle.skills.delete_skill(&req.skill_id) {
        Ok(()) => to_json_cstring(&crate::skills::models::SkillCommandResult::ok()),
        Err(e) => to_json_cstring(&crate::skills::models::SkillCommandResult::error(e)),
    }
}

/// Restore a skill back to its original agent location. Takes JSON: {"skill_id": "..."}. Returns JSON: {"ok": true/false, "error": "..."}
///
/// # Safety
/// `handle` must be valid; `json` must be a valid NUL-terminated C string.
#[no_mangle]
pub extern "C" fn tt_skills_restore(handle: *mut CoreHandle, json: *const c_char) -> *mut c_char {
    let handle = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return to_json_cstring(&crate::skills::models::SkillCommandResult::error("Null handle")),
    };

    #[derive(serde::Deserialize)]
    struct SkillIdReq { skill_id: String }

    let req: SkillIdReq = match unsafe { from_cstring_json(json) } {
        Ok(r) => r,
        Err(e) => return to_json_cstring(&crate::skills::models::SkillCommandResult::error(e)),
    };

    // Default to first agent for restore
    let agent_id = handle.skills.registry.all().first()
        .map(|a| a.id.clone())
        .unwrap_or_default();

    match handle.skills.restore_skill(&req.skill_id, &agent_id) {
        Ok(()) => to_json_cstring(&crate::skills::models::SkillCommandResult::ok()),
        Err(e) => to_json_cstring(&crate::skills::models::SkillCommandResult::error(e)),
    }
}

/// Add a custom agent. Takes JSON: {"name":"...", "skills_path":"...", "link_type":"Directory"|"SingleFile"|"Overlay"}
/// Returns JSON of AgentConfig on success, or SkillCommandResult error.
///
/// # Safety
/// `handle` must be valid; `json` must be a valid NUL-terminated C string.
#[no_mangle]
pub extern "C" fn tt_skills_add_custom_agent(handle: *mut CoreHandle, json: *const c_char) -> *mut c_char {
    let handle = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };

    let input: crate::skills::models::CustomAgentInput = match unsafe { from_cstring_json(json) } {
        Ok(i) => i,
        Err(e) => return to_json_cstring(&serde_json::json!({ "error": e })),
    };

    match handle.skills.registry.add_custom(input) {
        Ok(agent) => to_json_cstring(&agent),
        Err(e) => to_json_cstring(&serde_json::json!({ "error": e })),
    }
}

/// Remove a custom agent. Takes JSON: {"agent_id":"..."}
/// Returns SkillCommandResult: {"ok":true/false, "error":"..."}
///
/// # Safety
/// `handle` must be valid; `json` must be a valid NUL-terminated C string.
#[no_mangle]
pub extern "C" fn tt_skills_remove_custom_agent(handle: *mut CoreHandle, json: *const c_char) -> *mut c_char {
    let handle = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return to_json_cstring(&crate::skills::models::SkillCommandResult::error("Null handle")),
    };

    #[derive(serde::Deserialize)]
    struct AgentIdReq { agent_id: String }

    let req: AgentIdReq = match unsafe { from_cstring_json(json) } {
        Ok(r) => r,
        Err(e) => return to_json_cstring(&crate::skills::models::SkillCommandResult::error(e)),
    };

    match handle.skills.registry.remove_custom(&req.agent_id) {
        Ok(()) => to_json_cstring(&crate::skills::models::SkillCommandResult::ok()),
        Err(e) => to_json_cstring(&crate::skills::models::SkillCommandResult::error(e)),
    }
}

/// Pull (fetch + rebase) from git remote. Returns JSON GitStatusInfo.
///
/// # Safety
/// `handle` must be a valid pointer from `tt_init`, or null (returns null).
#[no_mangle]
pub extern "C" fn tt_skills_git_pull(handle: *mut CoreHandle) -> *mut c_char {
    let handle = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    match &mut handle.skills.git {
        Some(git) => match git.pull(None) {
            Ok(status) => to_json_cstring(&status),
            Err(e) => to_json_cstring(&crate::skills::models::GitStatusInfo::error(&e)),
        },
        None => to_json_cstring(&crate::skills::models::GitStatusInfo::error("No git repository")),
    }
}

/// Push to git remote. Returns JSON GitStatusInfo.
///
/// # Safety
/// `handle` must be a valid pointer from `tt_init`, or null (returns null).
#[no_mangle]
pub extern "C" fn tt_skills_git_push(handle: *mut CoreHandle) -> *mut c_char {
    let handle = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    match &mut handle.skills.git {
        Some(git) => match git.stage_and_push("skill: sync", None) {
            Ok(status) => to_json_cstring(&status),
            Err(e) => to_json_cstring(&crate::skills::models::GitStatusInfo::error(&e)),
        },
        None => to_json_cstring(&crate::skills::models::GitStatusInfo::error("No git repository")),
    }
}

/// Check git remote connectivity. Returns JSON GitConnectivity.
///
/// # Safety
/// `handle` must be a valid pointer from `tt_init`, or null (returns null).
#[no_mangle]
pub extern "C" fn tt_skills_git_connectivity(handle: *mut CoreHandle) -> *mut c_char {
    let handle = match unsafe { handle.as_ref() } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    match &handle.skills.git {
        Some(git) => match git.check_connectivity(None) {
            Ok(conn) => to_json_cstring(&conn),
            Err(e) => to_json_cstring(&crate::skills::models::GitConnectivity {
                status: "disconnected".into(),
                message: Some(e),
            }),
        },
        None => to_json_cstring(&crate::skills::models::GitConnectivity {
            status: "disconnected".into(),
            message: Some("No git repository".into()),
        }),
    }
}

/// Start file watcher on the skills source root.
/// Returns SkillCommandResult.
///
/// # Safety
/// `handle` must be a valid pointer from `tt_init`, or null (returns null).
#[no_mangle]
pub extern "C" fn tt_skills_watch_start(handle: *mut CoreHandle) -> *mut c_char {
    let handle = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return to_json_cstring(&crate::skills::models::SkillCommandResult::error("Null handle")),
    };

    let source_root = handle.skills.source_root.clone();
    let mut watcher = crate::skills::watcher::SkillWatcher::new(source_root);
    match watcher.start(|_event| {}) {
        Ok(()) => {
            handle.skills.watcher = Some(watcher);
            to_json_cstring(&crate::skills::models::SkillCommandResult::ok())
        }
        Err(e) => to_json_cstring(&crate::skills::models::SkillCommandResult::error(e)),
    }
}

/// Stop file watcher.
///
/// # Safety
/// `handle` must be a valid pointer from `tt_init`, or null.
#[no_mangle]
pub extern "C" fn tt_skills_watch_stop(handle: *mut CoreHandle) {
    let handle = match unsafe { handle.as_mut() } {
        Some(h) => h,
        None => return,
    };
    handle.skills.watcher = None;
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

/// Deserialize a JSON value from a C string pointer.
///
/// # Safety
/// `ptr` must be a valid, non-null, NUL-terminated C string.
unsafe fn from_cstring_json<T: serde::de::DeserializeOwned>(ptr: *const c_char) -> Result<T, String> {
    let s = CStr::from_ptr(ptr).to_str().map_err(|e| format!("Invalid UTF-8: {}", e))?;
    serde_json::from_str(s).map_err(|e| format!("Failed to parse JSON: {}", e))
}
