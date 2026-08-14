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

public sealed record AgentStatus(
    [property: JsonPropertyName("source")] string Source,
    [property: JsonPropertyName("record_count")] long RecordCount,
    [property: JsonPropertyName("installed")] bool Installed,
    [property: JsonPropertyName("last_sync")] string? LastSync);

public sealed record DailyPoint(
    [property: JsonPropertyName("date")] string Date,
    [property: JsonPropertyName("total_tokens")] ulong TotalTokens,
    [property: JsonPropertyName("total_cost_usd")] double TotalCostUsd,
    [property: JsonPropertyName("input_tokens")] ulong InputTokens,
    [property: JsonPropertyName("output_tokens")] ulong OutputTokens,
    [property: JsonPropertyName("cached_input_tokens")] ulong CachedInputTokens,
    [property: JsonPropertyName("cache_creation_input_tokens")] ulong CacheCreationInputTokens,
    [property: JsonPropertyName("reasoning_output_tokens")] ulong ReasoningOutputTokens,
    [property: JsonPropertyName("conversation_count")] uint ConversationCount);

public sealed record ModelEntry(
    [property: JsonPropertyName("model")] string Model,
    [property: JsonPropertyName("source")] string Source,
    [property: JsonPropertyName("total_tokens")] ulong TotalTokens,
    [property: JsonPropertyName("total_cost_usd")] double TotalCostUsd,
    [property: JsonPropertyName("percentage")] double Percentage);

public sealed record HeatmapPoint(
    [property: JsonPropertyName("date")] string Date,
    [property: JsonPropertyName("count")] ulong Count,
    [property: JsonPropertyName("level")] byte Level);

public sealed record SyncResult(
    [property: JsonPropertyName("agents_synced")] long AgentsSynced,
    [property: JsonPropertyName("records_added")] long RecordsAdded,
    [property: JsonPropertyName("errors")] string[] Errors);

/// A summary card for the tray panel (Today / 7D / 30D / Total). Not deserialized from JSON.
public sealed record PanelCard(string Title, string Value, string Subtitle);
