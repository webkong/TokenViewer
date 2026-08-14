using System.Text.Json;
using TokenViewerWindows;
using TokenViewerWindows.Models;
using Xunit;

namespace TokenViewerWindows.Tests;

/// <summary>
/// Verifies that the Rust core's snake_case JSON deserializes correctly into the
/// C# records using the shared <see cref="CoreBridge.JsonOptions"/>. No real
/// database or FFI is touched — fixtures only.
/// </summary>
public class CoreBridgeContractTests
{
    private static T? Deserialize<T>(string json) => JsonSerializer.Deserialize<T>(json, CoreBridge.JsonOptions);

    [Fact]
    public void DailyPoint_deserializes_all_snake_case_fields()
    {
        const string json = """[{"date":"2026-06-15","total_tokens":100,"total_cost_usd":0.5,"input_tokens":60,"output_tokens":40,"cached_input_tokens":10,"cache_creation_input_tokens":5,"reasoning_output_tokens":20,"conversation_count":3}]""";
        var p = Assert.Single(Deserialize<DailyPoint[]>(json)!);
        Assert.Equal("2026-06-15", p.Date);
        Assert.Equal(100UL, p.TotalTokens);
        Assert.Equal(0.5, p.TotalCostUsd);
        Assert.Equal(60UL, p.InputTokens);
        Assert.Equal(40UL, p.OutputTokens);
        Assert.Equal(10UL, p.CachedInputTokens);
        Assert.Equal(5UL, p.CacheCreationInputTokens);
        Assert.Equal(20UL, p.ReasoningOutputTokens);
        Assert.Equal(3U, p.ConversationCount);
    }

    [Fact]
    public void ModelEntry_deserializes_fields()
    {
        const string json = """[{"model":"claude-sonnet-4.6","source":"claude","total_tokens":1000,"total_cost_usd":2.5,"percentage":50.0}]""";
        var e = Assert.Single(Deserialize<ModelEntry[]>(json)!);
        Assert.Equal("claude-sonnet-4.6", e.Model);
        Assert.Equal("claude", e.Source);
        Assert.Equal(1000UL, e.TotalTokens);
        Assert.Equal(2.5, e.TotalCostUsd);
        Assert.Equal(50.0, e.Percentage);
    }

    [Fact]
    public void HeatmapPoint_deserializes_fields()
    {
        const string json = """[{"date":"2026-06-15","count":42,"level":3}]""";
        var p = Assert.Single(Deserialize<HeatmapPoint[]>(json)!);
        Assert.Equal("2026-06-15", p.Date);
        Assert.Equal(42UL, p.Count);
        Assert.Equal((byte)3, p.Level);
    }

    [Fact]
    public void SyncResult_deserializes_fields_and_ignores_providers_alias()
    {
        const string json = """{"agents_synced":12,"providers_synced":12,"records_added":300,"errors":["boom"]}""";
        var result = Deserialize<SyncResult>(json);
        Assert.NotNull(result);
        Assert.Equal(12L, result!.AgentsSynced);
        Assert.Equal(300L, result.RecordsAdded);
        Assert.Equal(new[] { "boom" }, result.Errors);
    }

    [Fact]
    public void UsageSummary_deserializes_fields()
    {
        const string json = """{"total_tokens":500,"total_cost_usd":1.25,"input_tokens":300,"output_tokens":200,"cached_input_tokens":50,"reasoning_output_tokens":100,"conversation_count":7,"active_days":3}""";
        var s = Deserialize<UsageSummary>(json);
        Assert.NotNull(s);
        Assert.Equal(500UL, s!.TotalTokens);
        Assert.Equal(1.25, s.TotalCostUsd);
        Assert.Equal(3U, s.ActiveDays);
    }
}
