using System.IO;
using System.Text.Json;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;
using TokenViewerWindows.Views;
using Xunit;

namespace TokenViewerWindows.Tests;

public class LimitsContractTests
{
    [Fact]
    public void CanonicalSources_contains_all_15_sources()
    {
        var expected = new[]
        {
            "claude", "codex", "cursor", "kiro", "copilot", "kimi", "antigravity",
            "zed", "trae", "windsurf", "qoder", "codebuddy", "workbuddy", "gemini", "zcode",
        };
        Assert.Equal(expected, LimitsService.CanonicalSources);
    }

    [Fact]
    public void NextResetAt_returns_earliest_future_reset()
    {
        var now = DateTime.Now;
        var future1 = now.AddHours(2);
        var future2 = now.AddHours(1);
        var agent = new AgentLimit("a", null, true, null, new[]
        {
            new LimitWindow("w1", 50, future1),
            new LimitWindow("w2", 30, future2),
        });
        Assert.Equal(future2, agent.NextResetAt);
    }

    [Fact]
    public void NextResetAt_falls_back_to_latest_past_reset()
    {
        var now = DateTime.Now;
        var past1 = now.AddHours(-2);
        var past2 = now.AddHours(-1);
        var agent = new AgentLimit("a", null, true, null, new[]
        {
            new LimitWindow("w1", 50, past1),
            new LimitWindow("w2", 30, past2),
        });
        Assert.Equal(past2, agent.NextResetAt);
    }

    [Fact]
    public void HasLimitDisplay_reflects_windows_and_dates()
    {
        var withWindows = new AgentLimit("a", null, true, null, new[] { new LimitWindow("w", 10, null) });
        Assert.True(withWindows.HasLimitDisplay);

        var withExpiry = new AgentLimit("b", null, true, null, [], SubscriptionExpiresAt: DateTime.Now.AddDays(1));
        Assert.True(withExpiry.HasLimitDisplay);

        var empty = new AgentLimit("c", null, false, null, []);
        Assert.False(empty.HasLimitDisplay);
    }

    [Fact]
    public void DisplayResetAt_falls_back_to_subscription_expiry()
    {
        var expiry = DateTime.Now.AddDays(3);
        var agent = new AgentLimit("zed", "Pro", true, null, [], SubscriptionExpiresAt: expiry);
        Assert.Equal(expiry, agent.DisplayResetAt);
    }

    [Fact]
    public void DisplayResetAt_prefers_future_subscription_over_past_windows()
    {
        var now = DateTime.Now;
        var pastWindow = now.AddHours(-5);
        var futureSubscription = now.AddDays(2);
        var agent = new AgentLimit("zed", "Pro", true, null,
            new[] { new LimitWindow("w", 50, pastWindow) },
            SubscriptionExpiresAt: futureSubscription);
        Assert.Equal(futureSubscription, agent.DisplayResetAt);
    }

    [Fact]
    public async Task Safe_isolates_provider_failure()
    {
        var result = await LimitsService.Safe("claude", () => throw new InvalidOperationException("boom"));
        Assert.Equal("claude", result.Name);
        Assert.False(result.Configured);
    }

    [Fact]
    public void SnapshotUsedPercent_parses_copilot_fixture()
    {
        using var doc = JsonDocument.Parse("""{"percent_remaining": 40.0}""");
        Assert.Equal(60.0, LimitsService.SnapshotUsedPercent(doc.RootElement), 2);
    }

    [Fact]
    public void WorkBuddyWindows_parses_usage_raw_accounts()
    {
        var json = """{"usage_raw":{"data":{"Response":{"Data":{"Accounts":[{"Status":0,"CycleCapacitySizePrecise":100.0,"CycleCapacityRemainPrecise":40.0,"CycleEndTime":"2026-08-01T00:00:00Z"}]}}}}}""";
        var account = ParseJson(json);
        var windows = LimitsService.WorkBuddyWindows(account);
        var w = Assert.Single(windows);
        Assert.Equal(60.0, w.UsedPercent, 2);
        Assert.NotNull(w.ResetAt);
    }

    [Fact]
    public void ColumnsForWidth_two_columns_wide_one_narrow()
    {
        Assert.Equal(2, LimitsLayout.ColumnsForWidth(1200));
        Assert.Equal(1, LimitsLayout.ColumnsForWidth(900));
    }

    [Fact]
    public void Limits_errors_and_labels_are_localized()
    {
        var keys = new[] { "errRequestFailed", "errNoUsageData", "window5Hour", "windowWeekly", "windowCredits", "windowQuota" };
        foreach (var key in keys)
        {
            Assert.True(L10n.Catalog.ContainsKey(key), $"missing '{key}'");
            var (en, zh) = L10n.Catalog[key];
            Assert.False(string.IsNullOrWhiteSpace(en), $"'{key}' en empty");
            Assert.False(string.IsNullOrWhiteSpace(zh), $"'{key}' zh empty");
        }
    }

    private static Dictionary<string, JsonElement> ParseJson(string text)
    {
        using var doc = JsonDocument.Parse(text);
        return doc.RootElement.EnumerateObject().ToDictionary(p => p.Name, p => p.Value.Clone());
    }

    [Fact]
    public async Task FetchAll_injects_15_and_isolates_one_failure()
    {
        var fetchers = new List<(string Source, Func<Task<AgentLimit>> Fetch)>();
        for (var i = 0; i < LimitsService.CanonicalSources.Length; i++)
        {
            var source = LimitsService.CanonicalSources[i];
            var index = i;
            if (index == 3)
            {
                fetchers.Add((source, () => throw new InvalidOperationException("boom")));
            }
            else
            {
                fetchers.Add((source, () => Task.FromResult(new AgentLimit(source, null, false, null, []))));
            }
        }

        var results = await LimitsService.FetchAllAsync(fetchers);

        Assert.Equal(15, results.Count);
        Assert.Equal(LimitsService.CanonicalSources, results.Select(r => r.Name).ToArray());
        Assert.False(results[3].Configured); // the throwing provider degraded, not dropped
    }

    [Fact]
    public void WriteCockpitAccountSnapshot_writes_parsable_files_and_no_temp_residue()
    {
        var tmp = Path.Combine(Path.GetTempPath(), "tv-snapshot-" + Guid.NewGuid().ToString("N"));
        try
        {
            var account = ParseJson("""{"id":"wb_user","email":"a@b.c","payment_type":"pro","quota_raw":{"payment":{"data":{}}}}""");
            LimitsService.WriteCockpitAccountSnapshot("workbuddy", account, tmp);

            var root = Path.Combine(tmp, ".antigravity_cockpit");
            var detail = Path.Combine(root, "workbuddy_accounts", "wb_user.json");
            var index = Path.Combine(root, "workbuddy_accounts.json");
            Assert.True(File.Exists(detail));
            Assert.True(File.Exists(index));

            var detailParsed = ParseJson(File.ReadAllText(detail));
            Assert.Equal("wb_user", detailParsed["id"].GetString());
            var indexParsed = ParseJson(File.ReadAllText(index));
            Assert.Equal("1.0", indexParsed["version"].GetString());

            Assert.Empty(Directory.GetFiles(root, "*.tmp", SearchOption.AllDirectories));
        }
        finally
        {
            if (Directory.Exists(tmp)) Directory.Delete(tmp, true);
        }
    }
}
