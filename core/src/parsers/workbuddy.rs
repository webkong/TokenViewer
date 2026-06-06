use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::models::UsageRecord;
use super::utils::*;

pub fn parse(
    home_dir: &Path,
    cursor_data: Option<&str>,
) -> Result<(Vec<UsageRecord>, String), Box<dyn std::error::Error>> {
    let mut cursor = FileCursor::from_json(cursor_data);
    let Some((account, file)) = read_account_snapshot(home_dir) else {
        return Ok((vec![], cursor.to_json()));
    };

    let key = file.to_string_lossy().to_string();
    if !cursor.file_changed(&key) {
        return Ok((vec![], cursor.to_json()));
    }

    let Some(used_units) = extract_used_units(&account) else {
        return Ok((vec![], cursor.to_json()));
    };

    let dedup_key = account
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("workbuddy");
    let delta = cursor.delta(dedup_key, [used_units, 0, 0, 0, 0]);
    let used_delta = delta[0];
    if used_delta == 0 {
        return Ok((vec![], cursor.to_json()));
    }

    let bucket = account
        .get("usage_updated_at")
        .and_then(json_i64_value)
        .and_then(epoch_to_bucket)
        .or_else(|| account.get("last_used").and_then(json_i64_value).and_then(epoch_to_bucket))
        .unwrap_or_else(|| file_mtime_bucket(&file));

    let record = UsageRecord {
        id: None,
        hour_start: bucket,
        source: "workbuddy".to_string(),
        model: "workbuddy-quota".to_string(),
        input_tokens: used_delta,
        output_tokens: 0,
        cached_input_tokens: 0,
        cache_creation_input_tokens: 0,
        reasoning_output_tokens: 0,
        total_tokens: used_delta,
        conversation_count: 1,
    };

    Ok((vec![record], cursor.to_json()))
}

fn read_account_snapshot(home_dir: &Path) -> Option<(Value, PathBuf)> {
    let base = home_dir.join(".antigravity_cockpit");
    let index_path = base.join("workbuddy_accounts.json");
    let accounts_dir = base.join("workbuddy_accounts");

    if let Some(index) = read_json_value(&index_path) {
        if let Some(id) = selected_account_id(&index) {
            let detail = accounts_dir.join(format!("{id}.json"));
            if let Some(account) = read_json_value(&detail) {
                return Some((account, detail));
            }
        }
    }

    let mut candidates: Vec<PathBuf> = fs::read_dir(&accounts_dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    candidates.sort_by_key(|path| std::cmp::Reverse(file_mtime_secs(path)));

    for file in candidates {
        if let Some(account) = read_json_value(&file) {
            return Some((account, file));
        }
    }
    None
}

fn read_json_value(path: &Path) -> Option<Value> {
    read_to_string_capped(path).and_then(|content| serde_json::from_str(&content).ok())
}

fn selected_account_id(index: &Value) -> Option<String> {
    let accounts = index.get("accounts")?.as_array()?;
    let selected = accounts.iter().max_by(|left, right| {
        account_sort_score(left)
            .cmp(&account_sort_score(right))
    })?;
    json_string(selected.get("id"))
        .or_else(|| json_string(selected.get("account_id")))
        .or_else(|| json_string(index.get("current_account_id")))
}

fn account_sort_score(value: &Value) -> i64 {
    json_i64(value.get("last_used_at"))
        .or_else(|| json_i64(value.get("last_used")))
        .or_else(|| json_i64(value.get("updated_at")))
        .unwrap_or(0)
}

fn extract_used_units(account: &Value) -> Option<u64> {
    let roots = [
        account.get("usage_raw"),
        account.pointer("/quota_raw/userResource"),
        account.get("quota_raw"),
    ];

    for root in roots.into_iter().flatten() {
        let accounts = root.pointer("/data/Response/Data/Accounts")?.as_array()?;
        let mut used_total = 0.0_f64;
        let mut found = false;
        for item in accounts {
            let status = item.get("Status").and_then(json_i64_value).unwrap_or(0);
            if status != 0 && status != 3 {
                continue;
            }
            let total = json_f64(item.get("CycleCapacitySizePrecise"))
                .or_else(|| json_f64(item.get("CycleCapacitySize")))
                .or_else(|| json_f64(item.get("CapacitySizePrecise")))
                .or_else(|| json_f64(item.get("CapacitySize")))
                .unwrap_or(0.0);
            let remain = json_f64(item.get("CycleCapacityRemainPrecise"))
                .or_else(|| json_f64(item.get("CycleCapacityRemain")))
                .or_else(|| json_f64(item.get("CapacityRemainPrecise")))
                .or_else(|| json_f64(item.get("CapacityRemain")))
                .unwrap_or(0.0);
            if total <= 0.0 {
                continue;
            }
            used_total += (total - remain).max(0.0);
            found = true;
        }
        if found {
            return Some(used_total.round().max(0.0) as u64);
        }
    }
    None
}

fn json_string(value: Option<&Value>) -> Option<String> {
    value.and_then(|raw| raw.as_str()).map(|s| s.to_string())
}

fn json_i64(value: Option<&Value>) -> Option<i64> {
    let value = value?;
    json_i64_value(value)
}

fn json_i64_value(value: &Value) -> Option<i64> {
    if let Some(v) = value.as_i64() {
        return Some(v);
    }
    if let Some(v) = value.as_u64() {
        return i64::try_from(v).ok();
    }
    value.as_str()?.trim().parse().ok()
}

fn json_f64(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    if let Some(v) = value.as_f64() {
        return Some(v);
    }
    if let Some(v) = value.as_i64() {
        return Some(v as f64);
    }
    if let Some(v) = value.as_u64() {
        return Some(v as f64);
    }
    value.as_str()?.trim().parse().ok()
}

fn epoch_to_bucket(value: i64) -> Option<String> {
    if value > 1_000_000_000_000 {
        epoch_millis_to_bucket(value)
    } else {
        epoch_secs_to_bucket(value)
    }
}
