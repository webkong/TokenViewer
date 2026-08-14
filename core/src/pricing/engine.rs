use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

use chrono::{DateTime, FixedOffset, Timelike};

use super::data::{PricingEntry, PEAK_PRICING_DATA, PRICING_DATA};
use crate::models::{ModelPricing, UsageRecord};

const ZERO_PRICING: ModelPricing = ModelPricing {
    input: 0.0,
    output: 0.0,
    cache_read: 0.0,
    cache_write: 0.0,
};

// ---------------------------------------------------------------------------
// Curated overrides (embedded from TokenTracker's curated-overrides.json).
// Resolution order mirrors the reference JS `lookupPricing` in
// TokenTracker/src/lib/pricing/matcher.js:
//   1. CURATED exact (incl. dot-restored input)
//   2. LITELLM exact (incl. dot-restored input)
//   3. CURATED alias (e.g. cursor "auto" -> composer-1)
//   4. CURATED fuzzy substring (e.g. "kiro-xyz" -> kiro-cli-agent)
//   5. LITELLM suffix-strip (gpt-5.6-solhigh -> gpt-5.6-sol)
//   6. LITELLM provider-prefix strip ("/model" suffix, lexicographically smallest)
//   7. LITELLM dot-qualified suffix (us./eu./au./global./anthropic. prefixes,
//      least-qualified key wins -> base price, not the +10% regional rate)
//   8. LITELLM reverse substring (model is a superset of the key, longest first)
//   9. Builtin PRICING_DATA (exact, then longest-prefix) as offline fallback.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
struct CuratedTable {
    exact: HashMap<String, ModelPricing>,
    alias: HashMap<String, String>,
    fuzzy: Vec<(String, String)>,
}

fn curated() -> &'static CuratedTable {
    static CURATED: OnceLock<CuratedTable> = OnceLock::new();
    CURATED.get_or_init(load_curated)
}

fn load_curated() -> CuratedTable {
    #[derive(serde::Deserialize, Default)]
    struct CuratedFile {
        #[serde(default)]
        exact: HashMap<String, CuratedPrice>,
        #[serde(default)]
        alias: HashMap<String, String>,
        #[serde(default)]
        fuzzy: Vec<CuratedFuzzy>,
    }
    #[derive(serde::Deserialize)]
    struct CuratedPrice {
        #[serde(default)]
        input: f64,
        #[serde(default)]
        output: f64,
        #[serde(default)]
        cache_read: f64,
        #[serde(default)]
        cache_write: f64,
    }
    #[derive(serde::Deserialize)]
    struct CuratedFuzzy {
        #[serde(rename = "match")]
        m: String,
        #[serde(rename = "ref")]
        r: String,
    }

    let raw = include_str!("curated-overrides.json");
    let parsed: CuratedFile = serde_json::from_str(raw).unwrap_or_default();
    let exact = parsed
        .exact
        .into_iter()
        .map(|(k, v)| {
            (
                k.to_lowercase(),
                ModelPricing {
                    input: v.input,
                    output: v.output,
                    cache_read: v.cache_read,
                    cache_write: v.cache_write,
                },
            )
        })
        .collect();
    let alias = parsed
        .alias
        .into_iter()
        .map(|(k, v)| (k.to_lowercase(), v.to_lowercase()))
        .collect();
    let fuzzy = parsed
        .fuzzy
        .into_iter()
        .map(|f| (f.m.to_lowercase(), f.r.to_lowercase()))
        .collect();
    CuratedTable {
        exact,
        alias,
        fuzzy,
    }
}

// ---------------------------------------------------------------------------
// Runtime LiteLLM override table, installed via `set_pricing_override` (FFI:
// tt_set_pricing). Per-million USD, keyed by raw LiteLLM key.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default)]
struct LiteLLMPricing {
    input: Option<f64>,
    output: Option<f64>,
    cache_read: Option<f64>,
    cache_write: Option<f64>,
}

fn litellm() -> &'static RwLock<HashMap<String, LiteLLMPricing>> {
    static LITELLM: OnceLock<RwLock<HashMap<String, LiteLLMPricing>>> = OnceLock::new();
    LITELLM.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Replace the runtime LiteLLM table from a raw (or slimmed) LiteLLM JSON map.
/// Per-token cost fields are converted to USD per million tokens and rounded to
/// 10 significant decimals, mirroring TokenTracker's `buildLitellmPerMillionMap`.
/// Meta keys (`_*`) and entries without any cost fields are skipped.
/// Returns the number of models installed.
pub fn set_pricing_override(json: &str) -> Result<usize, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("Invalid pricing JSON: {e}"))?;
    let obj = value
        .as_object()
        .ok_or_else(|| "Pricing JSON must be an object".to_string())?;

    let mut map = HashMap::new();
    for (name, entry) in obj {
        if name.starts_with('_') {
            continue;
        }
        let Some(entry) = entry.as_object() else {
            continue;
        };
        let cost = |field: &str| {
            entry
                .get(field)
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite())
                .map(convert_per_token)
        };
        let input = cost("input_cost_per_token");
        let output = cost("output_cost_per_token");
        let cache_read = cost("cache_read_input_token_cost");
        let cache_write = cost("cache_creation_input_token_cost");
        if input.is_none() && output.is_none() && cache_read.is_none() && cache_write.is_none() {
            continue;
        }
        map.insert(
            name.to_lowercase(),
            LiteLLMPricing {
                input,
                output,
                cache_read,
                cache_write,
            },
        );
    }

    if let Ok(mut guard) = litellm().write() {
        *guard = map;
    }
    Ok(litellm().read().map(|g| g.len()).unwrap_or(0))
}

/// Reset runtime state (used by tests for determinism).
#[cfg(test)]
fn reset_runtime() -> std::sync::MutexGuard<'static, ()> {
    static TEST_RUNTIME_LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
    let guard = TEST_RUNTIME_LOCK
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Ok(mut guard) = litellm().write() {
        guard.clear();
    }
    guard
}

/// Convert a per-token cost (as in LiteLLM JSON) to USD per million tokens,
/// rounded to 10 significant decimals (same as TokenTracker's roundToTenDecimals).
fn convert_per_token(v: f64) -> f64 {
    let per_million = v * 1_000_000.0;
    (per_million * 1e10).round() / 1e10
}

// ---------------------------------------------------------------------------
// Per-source model-name normalizers (lookup-time only; the raw model name is
// preserved for storage/display). Ported from TokenTracker matcher.js.
// ---------------------------------------------------------------------------

const TIERS: [&str; 3] = ["sonnet", "opus", "haiku"];

/// Shared cleaning step: strip parenthesized qualifiers, lowercase, collapse
/// non-alphanumeric runs to `-`, trim leading/trailing `-`. `keep_slash`
/// preserves `/` (Zed provider paths).
fn clean(s: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_parens = 0usize;
    let mut prev_dash = true;
    for ch in s.chars() {
        match ch {
            '(' => in_parens += 1,
            ')' if in_parens > 0 => in_parens -= 1,
            c if in_parens > 0 => {
                let _ = c;
            }
            c => {
                let keep = c.is_ascii_alphanumeric() || c == '.' || (keep_slash && c == '/');
                if keep {
                    out.push(c.to_ascii_lowercase());
                    prev_dash = false;
                } else if !prev_dash {
                    out.push('-');
                    prev_dash = true;
                }
            }
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn strip_reasoning_suffix(m: &str) -> String {
    const PATTERNS: [&str; 9] = [
        "-xhigh-fast",
        "-high-fast",
        "-medium-fast",
        "-low-fast",
        "-xhigh",
        "-high",
        "-medium",
        "-low",
        "-fast",
    ];
    for p in PATTERNS {
        if let Some(stripped) = m.strip_suffix(p) {
            return stripped.to_string();
        }
    }
    m.to_string()
}

/// Split a `major.minor` (or `major-minor`) token into (major, minor).
fn split_dotted(token: &str) -> Option<(&str, &str)> {
    let (a, b) = token.split_once('.')?;
    if !a.chars().all(|c| c.is_ascii_digit()) || !b.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((a, b))
}

fn is_tier(t: &str) -> bool {
    TIERS.contains(&t)
}

/// Re-join the remaining split parts (after `from`) back into `-foo-bar`.
fn rest_from(parts: &[&str], from: usize) -> String {
    if from < parts.len() {
        format!("-{}", parts[from..].join("-"))
    } else {
        String::new()
    }
}

/// Claude desktop/CLI/gateway names. Strips a provider path prefix
/// (`anthropic/...`, `openrouter/anthropic/...`), hyphenates dotted Claude
/// minors, and restores version-first ids for major >= 4
/// (`claude-4.6-opus-20260205` -> `claude-opus-4-6-20260205`).
fn normalize_claude_model(model: &str) -> String {
    let base = model.rsplit('/').next().unwrap_or(model);
    let m = clean(base, false);
    let parts: Vec<&str> = m.split('-').collect();

    // claude-{tier}-{major}.{minor}[...] -> claude-{tier}-{major}-{minor}[...]
    if parts.len() >= 3 && parts[0] == "claude" && is_tier(parts[1]) {
        if let Some((maj, min)) = split_dotted(parts[2]) {
            return format!(
                "claude-{}-{}-{}{}",
                parts[1],
                maj,
                min,
                rest_from(&parts, 3)
            );
        }
    }
    // {tier}-{major}[.-]{minor}[...] -> claude-{tier}-{major}-{minor}[...]
    if parts.len() >= 2 && is_tier(parts[0]) {
        if let Some((maj, min)) = split_dotted(parts[1]) {
            return format!(
                "claude-{}-{}-{}{}",
                parts[0],
                maj,
                min,
                rest_from(&parts, 2)
            );
        }
        if parts[1].chars().all(|c| c.is_ascii_digit())
            && parts.len() >= 3
            && parts[2]
                .split('.')
                .next()
                .map_or(false, |d| d.chars().all(|c| c.is_ascii_digit()))
        {
            return format!("claude-{}-{}{}", parts[0], parts[1], rest_from(&parts, 2));
        }
    }
    // version-first, major >= 4 only (Claude 3.x is genuinely version-first).
    if parts.len() >= 3 && parts[0] == "claude" {
        if let Some((maj, min)) = split_dotted(parts[1]) {
            if is_tier(parts[2]) {
                if let Ok(major) = maj.parse::<u32>() {
                    if major >= 4 {
                        return format!(
                            "claude-{}-{}-{}{}",
                            parts[2],
                            maj,
                            min,
                            rest_from(&parts, 3)
                        );
                    }
                }
            }
        }
    }
    m
}

/// Cursor decorates model ids with reasoning effort. Preserves Grok 4.5's
/// distinct Fast SKU; otherwise drops `thinking/xhigh/high/medium/low/fast`
/// decorations and delegates Claude ids to `normalize_claude_model`.
fn normalize_cursor_model(model: &str) -> String {
    let m = clean(model, false);
    if grok_4_5(&m) {
        return if m.contains("fast") {
            "cursor-grok-4.5-fast".to_string()
        } else {
            "cursor-grok-4.5".to_string()
        };
    }
    let decorations = ["thinking", "xhigh", "high", "medium", "low", "fast"];
    let filtered: Vec<&str> = m
        .split('-')
        .filter(|part| !decorations.contains(part))
        .collect();
    let joined = filtered.join("-");
    if joined.starts_with("claude-") {
        normalize_claude_model(&joined)
    } else {
        joined
    }
}

fn grok_4_5(m: &str) -> bool {
    let s = m.strip_prefix("cursor-").unwrap_or(m);
    let s = s.strip_prefix("grok-4").unwrap_or(s);
    s == ".5" || s == "-5" || s.starts_with(".5-") || s.starts_with("-5-")
}

/// Zed stores both canonical ids and human display names. Hyphenate Claude
/// dotted minors (`claude-opus-4.8` -> `claude-opus-4-8`) while keeping dotted
/// GPT minors (`gpt-5.2`) untouched.
fn normalize_zed_model(model: &str) -> String {
    let m = clean(model, true);
    let parts: Vec<&str> = m.split('-').collect();
    if parts.len() >= 3 && parts[0] == "claude" && is_tier(parts[1]) {
        if let Some((maj, min)) = split_dotted(parts[2]) {
            return format!(
                "claude-{}-{}-{}{}",
                parts[1],
                maj,
                min,
                rest_from(&parts, 3)
            );
        }
    }
    m
}

/// WorkBuddy's auto-router records the literal "auto", which collides with
/// Cursor's curated alias ("auto" -> composer-1). Map it to WorkBuddy's own
/// default model (hy3-preview-agent).
fn normalize_workbuddy_model(model: &str) -> String {
    if model.trim().eq_ignore_ascii_case("auto") {
        "hy3-preview-agent".to_string()
    } else {
        model.to_string()
    }
}

/// Antigravity bridges a mix of Gemini/Claude/GPT routes with display-style
/// names and reasoning-effort suffixes.
fn normalize_antigravity_model(model: &str) -> String {
    let mut m = clean(model, false);
    // Drop reasoning-effort words (word-boundary semantics approximated by
    // removing whole `-`-delimited tokens).
    let words = ["thinking", "xhigh", "high", "medium", "low", "fast"];
    let filtered: Vec<&str> = m.split('-').filter(|part| !words.contains(part)).collect();
    m = filtered.join("-");
    m = strip_reasoning_suffix(&m);

    if let Some(stripped) = m.strip_prefix("gemini-") {
        m = stripped.to_string();
    }
    if m.starts_with("gemini-3.") {
        let rest = &m["gemini-3.".len()..];
        let digits = rest.bytes().take_while(|b| b.is_ascii_digit()).count();
        let after = &rest[digits..];
        if after.starts_with("-flash-lite") {
            return "gemini-2.5-flash-lite".to_string();
        }
        if after.starts_with("-flash") {
            return "gemini-2.5-flash".to_string();
        }
        if after.starts_with("-pro") {
            return "gemini-2.5-pro".to_string();
        }
    }
    if let Some(c) = antigravity_claude(&m) {
        return c;
    }
    if m.starts_with("gpt-oss-120b") {
        return "antigravity-gpt-oss-120b".to_string();
    }
    m
}

fn antigravity_claude(m: &str) -> Option<String> {
    let parts: Vec<&str> = m.split('-').collect();
    if parts.len() >= 3 && parts[0] == "claude" && is_tier(parts[1]) {
        if let Some((maj, min)) = split_dotted(parts[2]) {
            if maj == "4" {
                return Some(format!(
                    "claude-{}-4-{}{}",
                    parts[1],
                    min,
                    rest_from(&parts, 3)
                ));
            }
        }
    }
    None
}

fn normalize_for_source(model: &str, source: &str) -> String {
    match source {
        "claude" | "pi" | "pi-anthropic" => normalize_claude_model(model),
        "cursor" => normalize_cursor_model(model),
        "zed" => normalize_zed_model(model),
        "workbuddy" => normalize_workbuddy_model(model),
        "antigravity" => normalize_antigravity_model(model),
        _ => model.to_string(),
    }
}

/// Restore dot-separated minor versions from dash-separated ones
/// (`glm-5-1-0` -> `glm-5.1.0`, `claude-opus-4-6` -> `claude-opus-4.6`).
/// Returns an empty string when the input has no digit-dash-digit pair.
fn build_dot_restored(lower: &str) -> String {
    let b = lower.as_bytes();
    let mut out = String::with_capacity(lower.len());
    let mut i = 0;
    let mut changed = false;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            out.push_str(&lower[start..i]);
            if i < b.len() && b[i] == b'-' && i + 1 < b.len() && b[i + 1].is_ascii_digit() {
                out.push('.');
                changed = true;
                i += 1;
            }
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    if changed {
        out
    } else {
        String::new()
    }
}

// ---------------------------------------------------------------------------
// Layered lookup
// ---------------------------------------------------------------------------

fn curated_contained_exact(curated: &CuratedTable, dot_form: &str) -> Option<ModelPricing> {
    if dot_form.is_empty() {
        return None;
    }
    let mut keys: Vec<&String> = curated.exact.keys().collect();
    keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
    for key in keys {
        if dot_form.contains(key.as_str()) {
            if let Some(p) = curated.exact.get(key) {
                return Some(*p);
            }
        }
    }
    None
}

/// Pure matcher (no global state) so unit tests can feed synthetic tables.
fn lookup_pricing_inner(
    model: &str,
    source: &str,
    curated: &CuratedTable,
    litellm: &HashMap<String, LiteLLMPricing>,
) -> Option<ModelPricing> {
    let normalized = normalize_for_source(model, source);
    let lower = normalized.to_lowercase();
    let dot_form = build_dot_restored(&lower);

    // 1. CURATED exact (case-insensitive), then dot-restored variants.
    if let Some(p) = curated.exact.get(&lower) {
        return Some(*p);
    }
    if !dot_form.is_empty() {
        if let Some(p) = curated.exact.get(&dot_form) {
            return Some(*p);
        }
    }
    if let Some(p) = curated_contained_exact(curated, &dot_form) {
        return Some(p);
    }

    // 2. LITELLM exact.
    if let Some(p) = litellm.get(&lower) {
        return Some(complete_litellm(*p, builtin_fallback(&lower, &dot_form)));
    }
    if !dot_form.is_empty() {
        if let Some(p) = litellm.get(&dot_form) {
            return Some(complete_litellm(*p, builtin_fallback(&lower, &dot_form)));
        }
    }

    // 3. CURATED alias (e.g. cursor "auto" -> composer-1).
    if let Some(target) = curated.alias.get(&lower) {
        if let Some(p) = curated.exact.get(target) {
            return Some(*p);
        }
    }

    // 4. CURATED fuzzy substring.
    if !curated.fuzzy.is_empty() {
        for (needle, ref_model) in &curated.fuzzy {
            if lower.contains(needle.as_str())
                || (!dot_form.is_empty() && dot_form.contains(needle.as_str()))
            {
                if let Some(p) = curated.exact.get(ref_model) {
                    return Some(*p);
                }
            }
        }
    }

    // 5. LITELLM suffix-strip (gpt-5.6-solhigh -> gpt-5.6-sol).
    let stripped = strip_reasoning_suffix(&lower);
    if stripped != lower {
        if let Some(p) = litellm.get(&stripped) {
            return Some(complete_litellm(*p, builtin_fallback(&lower, &dot_form)));
        }
    }

    // 6. LITELLM provider-prefix strip: key ends with "/<model>", pick the
    // lexicographically smallest key for determinism.
    if !litellm.is_empty() {
        let suffix = format!("/{lower}");
        let mut best: Option<(&String, LiteLLMPricing)> = None;
        for (key, p) in litellm {
            if key.len() > suffix.len() && key.to_ascii_lowercase().ends_with(&suffix) {
                if best.map_or(true, |(bk, _)| key < bk) {
                    best = Some((key, *p));
                }
            }
        }
        if let Some((_, p)) = best {
            return Some(complete_litellm(p, builtin_fallback(&lower, &dot_form)));
        }
    }

    // 7. LITELLM dot-qualified suffix: keys with a provider/region prefix
    // (`us./eu./au./global./anthropic./...`). Prefer the least-qualified key so
    // the base (global) price wins over the +10% regional rates.
    if !litellm.is_empty() {
        let suffix = format!(".{lower}");
        let mut best: Option<(usize, String, LiteLLMPricing)> = None;
        for (key, p) in litellm {
            if key.len() > suffix.len() && key.to_ascii_lowercase().ends_with(&suffix) {
                let depth = key.bytes().filter(|b| *b == b'.').count();
                if best.as_ref().map_or(true, |(bd, bk, _)| {
                    depth < *bd || (depth == *bd && key < bk)
                }) {
                    best = Some((depth, key.clone(), *p));
                }
            }
        }
        if let Some((_, _, p)) = best {
            return Some(complete_litellm(p, builtin_fallback(&lower, &dot_form)));
        }
    }

    // 8. LITELLM reverse substring (longest key first; model is a superset).
    if !litellm.is_empty() {
        let mut keys: Vec<&String> = litellm.keys().collect();
        keys.sort_by_key(|k| std::cmp::Reverse(k.len()));
        for key in keys {
            let k = key.to_ascii_lowercase();
            if lower.contains(&k) || (!dot_form.is_empty() && dot_form.contains(&k)) {
                if let Some(p) = litellm.get(key) {
                    return Some(complete_litellm(*p, builtin_fallback(&lower, &dot_form)));
                }
            }
        }
    }

    // 9. Builtin PRICING_DATA fallback (offline): exact, then longest-prefix.
    builtin_lookup(&lower)
}

fn builtin_fallback(lower: &str, dot_form: &str) -> Option<ModelPricing> {
    builtin_lookup(lower)
        .or_else(|| lower.rsplit('/').next().and_then(builtin_lookup))
        .or_else(|| {
            if dot_form.is_empty() {
                None
            } else {
                builtin_lookup(dot_form)
                    .or_else(|| dot_form.rsplit('/').next().and_then(builtin_lookup))
            }
        })
}

fn complete_litellm(pricing: LiteLLMPricing, fallback: Option<ModelPricing>) -> ModelPricing {
    let fallback = fallback.unwrap_or(ZERO_PRICING);
    ModelPricing {
        input: pricing.input.unwrap_or(fallback.input),
        output: pricing.output.unwrap_or(fallback.output),
        cache_read: pricing.cache_read.unwrap_or(fallback.cache_read),
        cache_write: pricing.cache_write.unwrap_or(fallback.cache_write),
    }
}

fn builtin_lookup(lower: &str) -> Option<ModelPricing> {
    for entry in PRICING_DATA {
        if entry.model == lower {
            return Some(entry.pricing);
        }
    }
    let mut best: Option<&PricingEntry> = None;
    for entry in PRICING_DATA {
        if lower.starts_with(entry.model)
            && best.map_or(true, |b| entry.model.len() > b.model.len())
        {
            best = Some(entry);
        }
    }
    best.map(|e| e.pricing)
}

/// Look up pricing, returning `None` when the model is unknown so callers can
/// distinguish "unpriced/unknown" from a genuine zero price.
pub fn lookup_model_pricing(model: &str, source: &str) -> Option<ModelPricing> {
    let litellm = litellm().read().unwrap_or_else(|p| p.into_inner());
    lookup_pricing_inner(model, source, curated(), &litellm)
}

/// Look up pricing for a model. Unknown models fall back to zero pricing, but
/// are logged once each so the cost is not silently dropped to $0 without a
/// trace.
pub fn get_model_pricing(model: &str, source: &str) -> ModelPricing {
    match lookup_model_pricing(model, source) {
        Some(p) => p,
        None => {
            warn_unpriced_once(model);
            ZERO_PRICING
        }
    }
}

/// Emit a single stderr warning per unknown model name (deduplicated across the
/// process). `auto` and empty names are intentionally ignored.
fn warn_unpriced_once(model: &str) {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    if model.is_empty() || model == "auto" {
        return;
    }
    static WARNED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let set = WARNED.get_or_init(|| Mutex::new(HashSet::new()));
    if let Ok(mut guard) = set.lock() {
        if guard.insert(model.to_string()) {
            eprintln!("tokenviewer: no pricing match for model '{model}' — cost counted as $0");
        }
    }
}

/// DeepSeek-style peak/off-peak window (Beijing time, UTC+8): peak hours are
/// 09:00-12:00 and 14:00-18:00. `hour_start` is a 30-min UTC bucket
/// (`YYYY-MM-DDTHH:MM:SSZ`); since a bucket is keyed by its start hour, hour
/// membership in {9,10,11,14,15,16,17} covers every 30-min bucket inside the
/// peak windows (e.g. 11:30 is peak, 12:00 is off-peak). Unparseable/empty
/// timestamps fall back to off-peak.
fn is_peak_hour(hour_start: &str) -> bool {
    let Ok(ts) = DateTime::parse_from_rfc3339(hour_start) else {
        return false;
    };
    let beijing = ts.with_timezone(&FixedOffset::east_opt(8 * 3600).expect("valid UTC+8 offset"));
    matches!(beijing.hour(), 9 | 10 | 11 | 14 | 15 | 16 | 17)
}

/// Peak-rate overlay lookup: exact, then longest-prefix, then last `/`-segment
/// (provider-prefixed names like `deepseek/deepseek-v4-pro`). Mirrors
/// `builtin_lookup` + `builtin_fallback`. `lower` is the normalized model name.
fn lookup_peak_pricing(lower: &str) -> Option<ModelPricing> {
    peak_lookup(lower).or_else(|| lower.rsplit('/').next().and_then(peak_lookup))
}

fn peak_lookup(lower: &str) -> Option<ModelPricing> {
    for entry in PEAK_PRICING_DATA {
        if entry.model == lower {
            return Some(entry.pricing);
        }
    }
    let mut best: Option<&PricingEntry> = None;
    for entry in PEAK_PRICING_DATA {
        if lower.starts_with(entry.model)
            && best.map_or(true, |b| entry.model.len() > b.model.len())
        {
            best = Some(entry);
        }
    }
    best.map(|e| e.pricing)
}

/// Compute USD cost for a single usage record (30-min bucket). DeepSeek models
/// use peak/off-peak rates keyed off the bucket's Beijing hour.
pub fn compute_row_cost(record: &UsageRecord) -> f64 {
    let mut pricing = get_model_pricing(&record.model, &record.source);

    if is_peak_hour(&record.hour_start) {
        let normalized = normalize_for_source(&record.model, &record.source).to_lowercase();
        if let Some(peak) = lookup_peak_pricing(&normalized) {
            pricing = peak;
        }
    }

    let reasoning = if record.source == "codex" || record.source == "every-code" {
        0
    } else {
        record.reasoning_output_tokens
    };

    (record.input_tokens as f64 * pricing.input
        + record.output_tokens as f64 * pricing.output
        + record.cached_input_tokens as f64 * pricing.cache_read
        + record.cache_creation_input_tokens as f64 * pricing.cache_write
        + reasoning as f64 * pricing.output)
        / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pricing(input: f64, output: f64) -> LiteLLMPricing {
        LiteLLMPricing {
            input: Some(input),
            output: Some(output),
            cache_read: Some(0.0),
            cache_write: Some(0.0),
        }
    }

    #[test]
    fn builtin_exact_and_prefix() {
        let _runtime_guard = reset_runtime();
        let p = lookup_model_pricing("claude-sonnet-4-20250514", "claude").unwrap();
        assert_eq!(p.input, 3.0);
        // Family prefix fallback for undated names.
        let p = lookup_model_pricing("claude-sonnet-4-6", "claude").unwrap();
        assert_eq!(p.input, 3.0);
    }

    #[test]
    fn claude_normalizer_handles_gateway_and_version_first() {
        let _runtime_guard = reset_runtime();
        // Provider path prefix stripped.
        let p = lookup_model_pricing("anthropic/claude-opus-4-8", "claude").unwrap();
        assert_eq!(p.input, 5.0);
        // Version-first relay name restored (claude-4.6-opus-20260205 -> claude-opus-4-6).
        let p = lookup_model_pricing("claude-4.6-opus-20260205", "claude").unwrap();
        assert_eq!(p.input, 5.0);
        assert_eq!(p.output, 25.0);
        // Dotted minor hyphenated.
        let p = lookup_model_pricing("claude-opus-4.8", "claude").unwrap();
        assert_eq!(p.input, 5.0);
    }

    #[test]
    fn zed_display_name_normalized() {
        let _runtime_guard = reset_runtime();
        let p = lookup_model_pricing("Claude Sonnet 4", "zed").unwrap();
        assert_eq!(p.input, 3.0);
        assert_eq!(p.output, 15.0);
    }

    #[test]
    fn antigravity_gateway_prefix_preserves_model_family() {
        let _runtime_guard = reset_runtime();
        let p = lookup_model_pricing("gemini-claude-opus-4.6-high", "antigravity").unwrap();
        assert_eq!(p.input, 5.0);
        assert_eq!(p.output, 25.0);
        assert_eq!(
            normalize_antigravity_model("gemini-gpt-oss-120b-medium"),
            "antigravity-gpt-oss-120b"
        );
    }

    #[test]
    fn workbuddy_auto_maps_to_hy3_not_composer() {
        let _runtime_guard = reset_runtime();
        // Curated alias would map "auto" -> composer-1 (Cursor); WorkBuddy's
        // normalizer pre-empts that with its own default model.
        let p = lookup_model_pricing("auto", "workbuddy").unwrap();
        assert_eq!(p.input, 0.167);
        // Cursor's "auto" still resolves to composer-1.
        let p = lookup_model_pricing("auto", "cursor").unwrap();
        assert_eq!(p.input, 1.25);
    }

    #[test]
    fn litellm_exact_and_fuzzy() {
        let _runtime_guard = reset_runtime();
        let mut litellm = HashMap::new();
        litellm.insert("openai/gpt-5.2".to_string(), pricing(1.75, 14.0));
        litellm.insert("deepseek/deepseek-chat".to_string(), pricing(0.14, 0.28));
        let curated = curated().clone();

        // Exact hit.
        assert_eq!(
            lookup_pricing_inner("gpt-5.2", "codex", &curated, &litellm)
                .unwrap()
                .input,
            1.75
        );
        // Provider-prefix strip: bare model resolves via "/<model>" suffix.
        assert_eq!(
            lookup_pricing_inner("deepseek-chat", "opencode", &curated, &litellm)
                .unwrap()
                .input,
            0.14
        );
    }

    #[test]
    fn litellm_region_prefixed_keys_resolve_to_base_price() {
        let _runtime_guard = reset_runtime();
        let mut litellm = HashMap::new();
        litellm.insert(
            "anthropic.claude-opus-4-6-v1".to_string(),
            pricing(5.0, 25.0),
        );
        litellm.insert(
            "us.anthropic.claude-opus-4-6-v1".to_string(),
            pricing(5.5, 27.5),
        );
        litellm.insert(
            "global.anthropic.claude-opus-4-6-v1".to_string(),
            pricing(5.0, 25.0),
        );
        let curated = curated().clone();

        // The +10% us./eu./au. regional rate must NOT win; base price does.
        let p = lookup_pricing_inner("claude-opus-4-6-v1", "claude", &curated, &litellm).unwrap();
        assert_eq!(p.input, 5.0);
        assert_eq!(p.output, 25.0);
    }

    #[test]
    fn litellm_missing_cache_fields_fall_back_to_builtin_pricing() {
        let _runtime_guard = reset_runtime();
        let mut litellm = HashMap::new();
        litellm.insert(
            "claude-sonnet-4-20250514".to_string(),
            LiteLLMPricing {
                input: Some(3.0),
                output: Some(15.0),
                cache_read: None,
                cache_write: None,
            },
        );
        let curated = curated().clone();

        let p =
            lookup_pricing_inner("claude-sonnet-4-20250514", "claude", &curated, &litellm).unwrap();
        assert_eq!(p.cache_read, 0.3);
        assert_eq!(p.cache_write, 3.75);
    }

    #[test]
    fn set_pricing_override_converts_per_token_to_per_million() {
        let _runtime_guard = reset_runtime();
        let json = r#"{
            "_meta": {"note": "x"},
            "acme.chat-v9": {
                "input_cost_per_token": 0.000002,
                "output_cost_per_token": 0.00001,
                "cache_read_input_token_cost": 0.0000002,
                "cache_creation_input_token_cost": 0.0000025
            },
            "no-cost-entry": {"litellm_provider": "x"}
        }"#;
        let n = set_pricing_override(json).unwrap();
        assert_eq!(n, 1);
        let p = lookup_model_pricing("chat-v9", "opencode").unwrap();
        assert_eq!(p.input, 2.0);
        assert_eq!(p.output, 10.0);
        assert_eq!(p.cache_read, 0.2);
        assert_eq!(p.cache_write, 2.5);
    }

    #[test]
    fn curated_wins_over_litellm() {
        let _runtime_guard = reset_runtime();
        // kiro-agent is pinned in curated; a (wrong) litellm price must not win.
        let _ = set_pricing_override(
            r#"{"kiro-agent":{"input_cost_per_token":0.0000999,"output_cost_per_token":0.0000999}}"#,
        );
        let p = lookup_model_pricing("kiro-agent", "kiro").unwrap();
        assert_eq!(p.input, 3.0);
        assert_eq!(p.output, 15.0);
    }

    #[test]
    fn reasoning_suffix_strip() {
        assert_eq!(strip_reasoning_suffix("gpt-5.6-sol-high"), "gpt-5.6-sol");
        assert_eq!(strip_reasoning_suffix("gpt-5.6-solhigh"), "gpt-5.6-solhigh");
        assert_eq!(strip_reasoning_suffix("gpt-5.6-sol"), "gpt-5.6-sol");
    }

    #[test]
    fn reasoning_suffix_attached_resolves_via_curated_fuzzy() {
        let _runtime_guard = reset_runtime();
        // `gpt-5.6-solhigh` has no exact key, but the curated fuzzy needle
        // `gpt-5.6-sol` matches as a substring and wins with the pinned price.
        let mut litellm = HashMap::new();
        litellm.insert("openai/gpt-5.6-sol".to_string(), pricing(1.75, 14.0));
        let curated = curated().clone();
        let p = lookup_pricing_inner("gpt-5.6-solhigh", "opencode", &curated, &litellm).unwrap();
        assert_eq!(p.input, 5.0);
    }

    #[test]
    fn dot_restore() {
        assert_eq!(build_dot_restored("glm-5-1-0"), "glm-5.1.0");
        assert_eq!(build_dot_restored("claude-opus-4-6"), "claude-opus-4.6");
        assert_eq!(build_dot_restored("gpt-5"), "");
    }

    #[test]
    fn fuzzy_substring_matches_kiro() {
        let _runtime_guard = reset_runtime();
        // Curated fuzzy "kiro" -> kiro-cli-agent.
        let p = lookup_model_pricing("kiro-future-xyz", "kiro").unwrap();
        assert_eq!(p.input, 3.0);
    }

    #[test]
    fn peak_hour_windows() {
        // Beijing peak windows: 09:00-12:00 and 14:00-18:00 (UTC+8).
        assert!(is_peak_hour("2026-08-17T01:00:00Z")); // Beijing 09:00
        assert!(is_peak_hour("2026-08-17T01:30:00Z")); // Beijing 09:30
        assert!(is_peak_hour("2026-08-17T03:30:00Z")); // Beijing 11:30
        assert!(!is_peak_hour("2026-08-17T04:00:00Z")); // Beijing 12:00 (lunch)
        assert!(!is_peak_hour("2026-08-17T05:00:00Z")); // Beijing 13:00
        assert!(is_peak_hour("2026-08-17T06:00:00Z")); // Beijing 14:00
        assert!(is_peak_hour("2026-08-17T09:30:00Z")); // Beijing 17:30
        assert!(!is_peak_hour("2026-08-17T10:00:00Z")); // Beijing 18:00
        assert!(!is_peak_hour("2026-08-17T16:00:00Z")); // Beijing 00:00
        assert!(!is_peak_hour("")); // unparseable -> off-peak
        assert!(!is_peak_hour("not-a-timestamp"));
    }

    #[test]
    fn peak_pricing_lookup() {
        let pro = lookup_peak_pricing("deepseek-v4-pro").unwrap();
        assert_eq!(pro.input, 1.25);
        assert_eq!(pro.output, 3.75);

        // Provider-prefixed names strip the last `/` segment.
        let pro = lookup_peak_pricing("deepseek/deepseek-v4-pro").unwrap();
        assert_eq!(pro.input, 1.25);

        let flash = lookup_peak_pricing("deepseek-v4-flash").unwrap();
        assert_eq!(flash.input, 0.416667);

        // Bare family prefix resolves to the v4-pro peak rate.
        let v4 = lookup_peak_pricing("deepseek-v4").unwrap();
        assert_eq!(v4.input, 1.25);

        // Non-DeepSeek models have no peak overlay.
        assert!(lookup_peak_pricing("claude-sonnet-4").is_none());
    }

    #[test]
    fn deepseek_cost_uses_peak_and_offpeak_rates() {
        let _runtime_guard = reset_runtime();
        let mut record = UsageRecord {
            id: None,
            hour_start: String::new(),
            source: "opencode".to_string(),
            model: "deepseek-v4-pro".to_string(),
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cached_input_tokens: 0,
            cache_creation_input_tokens: 0,
            reasoning_output_tokens: 0,
            total_tokens: 2_000_000,
            conversation_count: 1,
        };

        // Off-peak (Beijing 20:00): input 0.625 + output 1.875 = 2.5.
        record.hour_start = "2026-08-17T12:00:00Z".to_string();
        let offpeak = compute_row_cost(&record);
        assert!((offpeak - 2.5).abs() < 1e-9, "offpeak={offpeak}");

        // Peak (Beijing 10:00): input 1.25 + output 3.75 = 5.0.
        record.hour_start = "2026-08-17T02:00:00Z".to_string();
        let peak = compute_row_cost(&record);
        assert!((peak - 5.0).abs() < 1e-9, "peak={peak}");
    }
}
