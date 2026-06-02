use serde::{Deserialize, Serialize};

/// 一条 token 用量记录（最小聚合单位：30 分钟桶）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: Option<i64>,
    pub hour_start: String,
    pub source: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub reasoning_output_tokens: u64,
    pub total_tokens: u64,
    pub conversation_count: u32,
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

/// Provider 状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
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
