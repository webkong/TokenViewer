use crate::models::ModelPricing;

pub struct PricingEntry {
    pub model: &'static str,
    pub pricing: ModelPricing,
}

pub struct HistoricalPricingEntry {
    pub model: &'static str,
    pub effective_from: &'static str,
    pub effective_until: &'static str,
    pub pricing: ModelPricing,
}

pub static PRICING_DATA: &[PricingEntry] = &[
    PricingEntry {
        model: "claude-sonnet-4-20250514",
        pricing: ModelPricing {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
        },
    },
    PricingEntry {
        model: "claude-opus-4-20250414",
        pricing: ModelPricing {
            input: 5.0,
            output: 25.0,
            cache_read: 0.5,
            cache_write: 6.25,
        },
    },
    PricingEntry {
        model: "claude-3-5-sonnet-20241022",
        pricing: ModelPricing {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
        },
    },
    PricingEntry {
        model: "claude-3-5-haiku-20241022",
        pricing: ModelPricing {
            input: 0.8,
            output: 4.0,
            cache_read: 0.08,
            cache_write: 1.0,
        },
    },
    // Canonical Anthropic models use runtime LiteLLM prices when available.
    // These standard rates are offline fallbacks; Sonnet 5's introductory
    // interval is represented separately in HISTORICAL_PRICING_DATA.
    PricingEntry {
        model: "claude-fable-5",
        pricing: ModelPricing {
            input: 10.0,
            output: 50.0,
            cache_read: 1.0,
            cache_write: 12.5,
        },
    },
    PricingEntry {
        model: "claude-opus-5",
        pricing: ModelPricing {
            input: 5.0,
            output: 25.0,
            cache_read: 0.5,
            cache_write: 6.25,
        },
    },
    PricingEntry {
        model: "claude-opus-4-8",
        pricing: ModelPricing {
            input: 5.0,
            output: 25.0,
            cache_read: 0.5,
            cache_write: 6.25,
        },
    },
    PricingEntry {
        model: "claude-sonnet-5",
        pricing: ModelPricing {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
        },
    },
    PricingEntry {
        model: "claude-haiku-4-5",
        pricing: ModelPricing {
            input: 1.0,
            output: 5.0,
            cache_read: 0.1,
            cache_write: 1.25,
        },
    },
    PricingEntry {
        model: "gpt-4o",
        pricing: ModelPricing {
            input: 2.5,
            output: 10.0,
            cache_read: 1.25,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "gpt-4o-mini",
        pricing: ModelPricing {
            input: 0.15,
            output: 0.6,
            cache_read: 0.075,
            cache_write: 0.0,
        },
    },
    // Current OpenAI standard rates. Runtime LiteLLM data takes precedence;
    // these entries keep offline cost estimates available.
    PricingEntry {
        model: "gpt-5.6-sol",
        pricing: ModelPricing {
            input: 4.0,
            output: 20.0,
            cache_read: 0.4,
            cache_write: 5.0,
        },
    },
    PricingEntry {
        model: "gpt-5.6-terra",
        pricing: ModelPricing {
            input: 2.0,
            output: 12.0,
            cache_read: 0.2,
            cache_write: 2.5,
        },
    },
    PricingEntry {
        model: "gpt-5.6-luna",
        pricing: ModelPricing {
            input: 0.2,
            output: 1.2,
            cache_read: 0.02,
            cache_write: 0.25,
        },
    },
    PricingEntry {
        model: "gpt-5.6",
        pricing: ModelPricing {
            input: 4.0,
            output: 20.0,
            cache_read: 0.4,
            cache_write: 5.0,
        },
    },
    PricingEntry {
        model: "o3",
        pricing: ModelPricing {
            input: 2.0,
            output: 8.0,
            cache_read: 0.5,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "o4-mini",
        pricing: ModelPricing {
            input: 1.1,
            output: 4.4,
            cache_read: 0.275,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "gemini-2.5-pro",
        pricing: ModelPricing {
            input: 1.25,
            output: 10.0,
            cache_read: 0.31,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "gemini-2.5-flash",
        pricing: ModelPricing {
            input: 0.15,
            output: 0.6,
            cache_read: 0.0375,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "kiro-agent",
        pricing: ModelPricing {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
        },
    },
    PricingEntry {
        model: "kiro-cli-agent",
        pricing: ModelPricing {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
        },
    },
    PricingEntry {
        model: "deepseek-chat",
        pricing: ModelPricing {
            input: 0.28,
            output: 0.42,
            cache_read: 0.028,
            cache_write: 0.28,
        },
    },
    PricingEntry {
        model: "deepseek-reasoner",
        pricing: ModelPricing {
            input: 0.28,
            output: 0.42,
            cache_read: 0.028,
            cache_write: 0.28,
        },
    },
    PricingEntry {
        model: "grok-4",
        pricing: ModelPricing {
            input: 3.0,
            output: 15.0,
            cache_read: 0.75,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "grok-4-fast",
        pricing: ModelPricing {
            input: 0.2,
            output: 0.5,
            cache_read: 0.05,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "kimi-for-coding",
        pricing: ModelPricing {
            input: 0.6,
            output: 2.0,
            cache_read: 0.15,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "composer-1",
        pricing: ModelPricing {
            input: 1.25,
            output: 10.0,
            cache_read: 0.125,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "composer-2",
        pricing: ModelPricing {
            input: 0.5,
            output: 2.5,
            cache_read: 0.2,
            cache_write: 0.0,
        },
    },
    // Family prefixes (lowercase) for real-world model names like claude-sonnet-4-6.
    PricingEntry {
        model: "claude-opus-4",
        pricing: ModelPricing {
            input: 5.0,
            output: 25.0,
            cache_read: 0.5,
            cache_write: 6.25,
        },
    },
    PricingEntry {
        model: "claude-sonnet-4",
        pricing: ModelPricing {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
        },
    },
    PricingEntry {
        model: "claude-haiku-4",
        pricing: ModelPricing {
            input: 0.8,
            output: 4.0,
            cache_read: 0.08,
            cache_write: 1.0,
        },
    },
    PricingEntry {
        model: "claude-3-5-sonnet",
        pricing: ModelPricing {
            input: 3.0,
            output: 15.0,
            cache_read: 0.3,
            cache_write: 3.75,
        },
    },
    PricingEntry {
        model: "claude-3-5-haiku",
        pricing: ModelPricing {
            input: 0.8,
            output: 4.0,
            cache_read: 0.08,
            cache_write: 1.0,
        },
    },
    // DeepSeek v4 peak/off-peak pricing (effective 2026-08-17, Beijing time).
    // Base prices below are the OFF-PEAK (空闲时段) rates; peak rates live in
    // PEAK_PRICING_DATA at the bottom of this file. RMB -> USD at ~7.2 RMB/USD.
    PricingEntry {
        model: "deepseek-v4-pro",
        pricing: ModelPricing {
            input: 0.625,
            output: 1.875,
            cache_read: 0.020833,
            cache_write: 0.625,
        },
    },
    PricingEntry {
        model: "deepseek-v4-flash",
        pricing: ModelPricing {
            input: 0.208333,
            output: 0.625,
            cache_read: 0.006944,
            cache_write: 0.208333,
        },
    },
    PricingEntry {
        model: "deepseek-v4",
        pricing: ModelPricing {
            input: 0.625,
            output: 1.875,
            cache_read: 0.020833,
            cache_write: 0.625,
        },
    },
    PricingEntry {
        model: "kimi-k2",
        pricing: ModelPricing {
            input: 0.6,
            output: 2.0,
            cache_read: 0.15,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "glm-4",
        pricing: ModelPricing {
            input: 0.6,
            output: 2.2,
            cache_read: 0.11,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "qwen3-coder",
        pricing: ModelPricing {
            input: 0.3,
            output: 1.2,
            cache_read: 0.06,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "gpt-5",
        pricing: ModelPricing {
            input: 1.25,
            output: 10.0,
            cache_read: 0.125,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "gemini-2.5",
        pricing: ModelPricing {
            input: 1.25,
            output: 10.0,
            cache_read: 0.31,
            cache_write: 0.0,
        },
    },
    // CodeBuddy — Tencent Hy3 Preview (16~32K tier, USD @ 1:6.77 CNY)
    PricingEntry {
        model: "hy3-preview",
        pricing: ModelPricing {
            input: 0.2365,
            output: 0.9459,
            cache_read: 0.0887,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "hy3-preview-agent",
        pricing: ModelPricing {
            input: 0.2365,
            output: 0.9459,
            cache_read: 0.0887,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "codebuddy-agent",
        pricing: ModelPricing {
            input: 0.2365,
            output: 0.9459,
            cache_read: 0.0887,
            cache_write: 0.0,
        },
    },
    // GLM-5 family (ZCode / Z.ai / BigModel). Approximate BigModel API rates
    // (USD/M tokens); zcode routes these through its Coding Plan subscription,
    // so real cost to the user is the flat plan fee — these are reference rates.
    PricingEntry {
        model: "glm-5.2",
        pricing: ModelPricing {
            input: 0.07,
            output: 0.21,
            cache_read: 0.014,
            cache_write: 0.0,
        },
    },
    PricingEntry {
        model: "glm-5-turbo",
        pricing: ModelPricing {
            input: 0.04,
            output: 0.12,
            cache_read: 0.008,
            cache_write: 0.0,
        },
    },
];

// Closed documented price intervals. Open-ended current prices continue to come
// from LiteLLM (or PRICING_DATA while offline), so future changes do not require
// guessing an end date in advance. Boundaries are stored as UTC timestamps.
pub static HISTORICAL_PRICING_DATA: &[HistoricalPricingEntry] = &[
    HistoricalPricingEntry {
        model: "claude-sonnet-5",
        effective_from: "2026-06-30T00:00:00Z",
        effective_until: "2026-09-01T00:00:00Z",
        pricing: ModelPricing {
            input: 2.0,
            output: 10.0,
            cache_read: 0.2,
            cache_write: 2.5,
        },
    },
    HistoricalPricingEntry {
        model: "gpt-5.6-sol",
        effective_from: "2026-07-09T00:00:00Z",
        effective_until: "2026-08-21T00:00:00Z",
        pricing: ModelPricing {
            input: 5.0,
            output: 30.0,
            cache_read: 0.5,
            cache_write: 6.25,
        },
    },
    HistoricalPricingEntry {
        model: "gpt-5.6-terra",
        effective_from: "2026-07-09T00:00:00Z",
        effective_until: "2026-07-30T00:00:00Z",
        pricing: ModelPricing {
            input: 2.5,
            output: 15.0,
            cache_read: 0.25,
            cache_write: 3.125,
        },
    },
    HistoricalPricingEntry {
        model: "gpt-5.6-luna",
        effective_from: "2026-07-09T00:00:00Z",
        effective_until: "2026-07-30T00:00:00Z",
        pricing: ModelPricing {
            input: 1.0,
            output: 6.0,
            cache_read: 0.1,
            cache_write: 1.25,
        },
    },
    HistoricalPricingEntry {
        model: "gpt-5.6",
        effective_from: "2026-07-09T00:00:00Z",
        effective_until: "2026-08-21T00:00:00Z",
        pricing: ModelPricing {
            input: 5.0,
            output: 30.0,
            cache_read: 0.5,
            cache_write: 6.25,
        },
    },
    // DeepSeek V4 switched to Beijing-time peak/off-peak pricing on
    // 2026-08-17. Preserve the preceding flat rates for older usage buckets.
    HistoricalPricingEntry {
        model: "deepseek-v4-flash",
        effective_from: "1970-01-01T00:00:00Z",
        effective_until: "2026-08-16T16:00:00Z",
        pricing: ModelPricing {
            input: 0.14,
            output: 0.28,
            cache_read: 0.0028,
            cache_write: 0.14,
        },
    },
    HistoricalPricingEntry {
        model: "deepseek-v4-pro",
        effective_from: "1970-01-01T00:00:00Z",
        effective_until: "2026-08-16T16:00:00Z",
        pricing: ModelPricing {
            input: 0.435,
            output: 0.87,
            cache_read: 0.003625,
            cache_write: 0.435,
        },
    },
    HistoricalPricingEntry {
        model: "deepseek-v4",
        effective_from: "1970-01-01T00:00:00Z",
        effective_until: "2026-08-16T16:00:00Z",
        pricing: ModelPricing {
            input: 0.435,
            output: 0.87,
            cache_read: 0.003625,
            cache_write: 0.435,
        },
    },
];

/// Peak-hour (高峰时段) overlay for models with DeepSeek-style peak/off-peak
/// pricing. Keyed by the same normalized model names as `PRICING_DATA`; only
/// consulted when the usage record falls in a Beijing peak hour (9:00-12:00,
/// 14:00-18:00), otherwise the base off-peak rate applies. cache_write = input
/// (no separate write surcharge). RMB -> USD at ~7.2 RMB/USD.
pub static PEAK_PRICING_DATA: &[PricingEntry] = &[
    PricingEntry {
        model: "deepseek-v4-pro",
        pricing: ModelPricing {
            input: 1.25,
            output: 3.75,
            cache_read: 0.041667,
            cache_write: 1.25,
        },
    },
    PricingEntry {
        model: "deepseek-v4-flash",
        pricing: ModelPricing {
            input: 0.416667,
            output: 1.25,
            cache_read: 0.013889,
            cache_write: 0.416667,
        },
    },
    PricingEntry {
        model: "deepseek-v4",
        pricing: ModelPricing {
            input: 1.25,
            output: 3.75,
            cache_read: 0.041667,
            cache_write: 1.25,
        },
    },
];
