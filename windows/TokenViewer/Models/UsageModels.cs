using System.Text.Json.Serialization;

namespace TokenViewerWindows.Models;

public sealed record UsageSummary(
    [property: JsonPropertyName("total_tokens")] ulong TotalTokens,
    [property: JsonPropertyName("total_cost_usd")] double TotalCostUsd,
    [property: JsonPropertyName("input_tokens")] ulong InputTokens,
    [property: JsonPropertyName("output_tokens")] ulong OutputTokens,
    [property: JsonPropertyName("cached_input_tokens")] ulong CachedInputTokens,
    [property: JsonPropertyName("reasoning_output_tokens")] ulong ReasoningOutputTokens,
    [property: JsonPropertyName("conversation_count")] uint ConversationCount,
    [property: JsonPropertyName("active_days")] uint ActiveDays);

public sealed record ProviderStatus(
    [property: JsonPropertyName("source")] string Source,
    [property: JsonPropertyName("record_count")] long RecordCount,
    [property: JsonPropertyName("installed")] bool Installed,
    [property: JsonPropertyName("last_sync")] string? LastSync);
