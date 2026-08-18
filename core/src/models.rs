use serde::{Deserialize, Serialize};

/// 一条 token 用量记录（最小聚合单位：30 分钟桶）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: Option<i64>,
    pub hour_start: String,
    pub source: String,
    pub model: String,
    /// Local project identity. Empty for agents whose logs do not expose a cwd/repository.
    #[serde(default)]
    pub project_key: String,
    /// Local cwd or git remote used to derive `project_key`; never leaves the device.
    #[serde(default)]
    pub project_ref: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
    pub conversation_count: u32,
}

/// A resumable coding-agent session (conversation) discovered on disk.
///
/// `id` is the stable composite key `"<source>:<raw_session_id>"` so that
/// sessions from different agents can never collide. The raw per-agent session
/// id is recovered by stripping the `"<source>:"` prefix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    /// Agent source id, e.g. `"claude"`, `"codex"`, `"grok"`.
    pub source: String,
    /// Working directory the session was started in (may be empty).
    pub cwd: String,
    /// Human-friendly project label derived from `cwd` (its directory name).
    pub project: String,
    /// Auto-derived display title (agent title → first user task → project+time).
    pub title: String,
    /// User-assigned override; when present it wins over `title`.
    pub custom_title: Option<String>,
    /// First valid user prompt, cleaned for display/title fallback.
    pub first_user_message: String,
    /// UTC ISO-8601 timestamp of the first activity seen.
    pub started_at: String,
    /// UTC ISO-8601 timestamp of the last activity (derived from file mtime).
    pub last_active_at: String,
    /// Source file the session was parsed from (may be empty for archived rows).
    pub file_path: String,
    /// Codex home root this session belongs to; empty for the default `~/.codex`.
    pub codex_home: String,
    /// Model observed in the session log, when available.
    #[serde(default)]
    pub model: String,
    /// Total tokens consumed by this session.
    #[serde(default)]
    pub total_tokens: u64,
    /// Locally computed USD cost using TokenViewer pricing.
    #[serde(default)]
    pub total_cost_usd: f64,
    /// Number of user turns observed in the session.
    #[serde(default)]
    pub turn_count: u32,
    /// Number of edit/write tool calls observed in the session.
    #[serde(default)]
    pub edit_count: u32,
    /// Active duration in seconds, excluding gaps longer than 30 minutes.
    #[serde(default)]
    pub duration_seconds: u64,
}

/// Usage aggregated by project across all dates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectUsageEntry {
    pub project_key: String,
    pub project_ref: String,
    pub total_tokens: u64,
    pub sources: Vec<String>,
}

/// 定价条目（USD per million tokens）
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ModelPricing {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
}

/// 查询结果：汇总
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub conversation_count: u32,
    pub active_days: u32,
}

/// 每日用量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyUsage {
    pub date: String,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub conversation_count: u32,
}

/// 模型分布条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelBreakdownEntry {
    pub model: String,
    pub source: String,
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_tokens: u64,
    pub total_cost_usd: f64,
    pub percentage: f64,
}

/// 热力图数据点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatmapPoint {
    pub date: String,
    pub count: u64,
    pub level: u8, // 0-4
}

/// Agent 状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub source: String,
    pub installed: bool,
    pub last_sync: Option<String>,
    pub record_count: u64,
}

/// 同步游标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCursor {
    pub source: String,
    pub cursor_data: String, // JSON: 文件路径+偏移量等
    pub updated_at: String,
}

/// 设置项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
}

impl UsageRecord {
    /// 计算单条记录的费用
    pub fn compute_cost(&self, pricing: &ModelPricing) -> f64 {
        (self.input_tokens as f64 * pricing.input
            + self.output_tokens as f64 * pricing.output
            + self.cached_input_tokens as f64 * pricing.cache_read
            + self.cache_creation_input_tokens as f64 * pricing.cache_write
            + self.reasoning_output_tokens as f64 * pricing.output)
            / 1_000_000.0
    }
}
