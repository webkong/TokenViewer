use crate::models::{ModelPricing, UsageRecord};
use super::data::{PricingEntry, PRICING_DATA};

const ZERO_PRICING: ModelPricing = ModelPricing { input: 0.0, output: 0.0, cache_read: 0.0, cache_write: 0.0 };

/// Look up pricing for a model. Tries exact match, then longest-prefix match
/// (a known family key that the model name starts with, e.g. "claude-sonnet-4"
/// matches "claude-sonnet-4-6" / "claude-sonnet-4.6").
pub fn get_model_pricing(model: &str, _source: &str) -> ModelPricing {
    let lower = model.to_lowercase();

    // Exact match wins.
    for entry in PRICING_DATA {
        if entry.model == lower {
            return entry.pricing;
        }
    }

    // Longest-prefix family match: pick the known key that is the longest
    // prefix of the input model name.
    let mut best: Option<&PricingEntry> = None;
    for entry in PRICING_DATA {
        if lower.starts_with(entry.model)
            && best.map_or(true, |b| entry.model.len() > b.model.len())
        {
            best = Some(entry);
        }
    }
    best.map(|e| e.pricing).unwrap_or(ZERO_PRICING)
}

/// Compute USD cost for a single usage record.
pub fn compute_row_cost(record: &UsageRecord) -> f64 {
    let pricing = get_model_pricing(&record.model, &record.source);

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
