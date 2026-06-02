use crate::models::ModelPricing;

pub struct PricingEntry {
    pub model: &'static str,
    pub pricing: ModelPricing,
}

pub static PRICING_DATA: &[PricingEntry] = &[
    PricingEntry { model: "claude-sonnet-4-20250514", pricing: ModelPricing { input: 3.0, output: 15.0, cache_read: 0.3, cache_write: 3.75 } },
    PricingEntry { model: "claude-opus-4-20250414", pricing: ModelPricing { input: 5.0, output: 25.0, cache_read: 0.5, cache_write: 6.25 } },
    PricingEntry { model: "claude-3-5-sonnet-20241022", pricing: ModelPricing { input: 3.0, output: 15.0, cache_read: 0.3, cache_write: 3.75 } },
    PricingEntry { model: "claude-3-5-haiku-20241022", pricing: ModelPricing { input: 0.8, output: 4.0, cache_read: 0.08, cache_write: 1.0 } },
    PricingEntry { model: "gpt-4o", pricing: ModelPricing { input: 2.5, output: 10.0, cache_read: 1.25, cache_write: 0.0 } },
    PricingEntry { model: "gpt-4o-mini", pricing: ModelPricing { input: 0.15, output: 0.6, cache_read: 0.075, cache_write: 0.0 } },
    PricingEntry { model: "o3", pricing: ModelPricing { input: 2.0, output: 8.0, cache_read: 0.5, cache_write: 0.0 } },
    PricingEntry { model: "o4-mini", pricing: ModelPricing { input: 1.1, output: 4.4, cache_read: 0.275, cache_write: 0.0 } },
    PricingEntry { model: "gemini-2.5-pro", pricing: ModelPricing { input: 1.25, output: 10.0, cache_read: 0.31, cache_write: 0.0 } },
    PricingEntry { model: "gemini-2.5-flash", pricing: ModelPricing { input: 0.15, output: 0.6, cache_read: 0.0375, cache_write: 0.0 } },
    PricingEntry { model: "kiro-agent", pricing: ModelPricing { input: 3.0, output: 15.0, cache_read: 0.3, cache_write: 3.75 } },
    PricingEntry { model: "deepseek-chat", pricing: ModelPricing { input: 0.14, output: 0.28, cache_read: 0.0028, cache_write: 0.14 } },
    PricingEntry { model: "deepseek-reasoner", pricing: ModelPricing { input: 0.14, output: 0.28, cache_read: 0.0028, cache_write: 0.14 } },
    PricingEntry { model: "grok-4", pricing: ModelPricing { input: 3.0, output: 15.0, cache_read: 0.75, cache_write: 0.0 } },
    PricingEntry { model: "grok-4-fast", pricing: ModelPricing { input: 0.2, output: 0.5, cache_read: 0.05, cache_write: 0.0 } },
    PricingEntry { model: "kimi-for-coding", pricing: ModelPricing { input: 0.6, output: 2.0, cache_read: 0.15, cache_write: 0.0 } },
    PricingEntry { model: "composer-1", pricing: ModelPricing { input: 1.25, output: 10.0, cache_read: 0.125, cache_write: 0.0 } },
    PricingEntry { model: "composer-2", pricing: ModelPricing { input: 0.5, output: 2.5, cache_read: 0.2, cache_write: 0.0 } },
    // Family prefixes (lowercase) for real-world model names like claude-sonnet-4-6.
    PricingEntry { model: "claude-opus-4", pricing: ModelPricing { input: 5.0, output: 25.0, cache_read: 0.5, cache_write: 6.25 } },
    PricingEntry { model: "claude-sonnet-4", pricing: ModelPricing { input: 3.0, output: 15.0, cache_read: 0.3, cache_write: 3.75 } },
    PricingEntry { model: "claude-haiku-4", pricing: ModelPricing { input: 0.8, output: 4.0, cache_read: 0.08, cache_write: 1.0 } },
    PricingEntry { model: "claude-3-5-sonnet", pricing: ModelPricing { input: 3.0, output: 15.0, cache_read: 0.3, cache_write: 3.75 } },
    PricingEntry { model: "claude-3-5-haiku", pricing: ModelPricing { input: 0.8, output: 4.0, cache_read: 0.08, cache_write: 1.0 } },
    PricingEntry { model: "deepseek-v4-pro", pricing: ModelPricing { input: 0.435, output: 0.87, cache_read: 0.003625, cache_write: 0.435 } },
    PricingEntry { model: "deepseek-v4-flash", pricing: ModelPricing { input: 0.14, output: 0.28, cache_read: 0.0028, cache_write: 0.14 } },
    PricingEntry { model: "deepseek-v4", pricing: ModelPricing { input: 0.435, output: 0.87, cache_read: 0.003625, cache_write: 0.435 } },
    PricingEntry { model: "kimi-k2", pricing: ModelPricing { input: 0.6, output: 2.0, cache_read: 0.15, cache_write: 0.0 } },
    PricingEntry { model: "glm-4", pricing: ModelPricing { input: 0.6, output: 2.2, cache_read: 0.11, cache_write: 0.0 } },
    PricingEntry { model: "qwen3-coder", pricing: ModelPricing { input: 0.3, output: 1.2, cache_read: 0.06, cache_write: 0.0 } },
    PricingEntry { model: "gpt-5", pricing: ModelPricing { input: 1.25, output: 10.0, cache_read: 0.125, cache_write: 0.0 } },
    PricingEntry { model: "gemini-2.5", pricing: ModelPricing { input: 1.25, output: 10.0, cache_read: 0.31, cache_write: 0.0 } },
];
