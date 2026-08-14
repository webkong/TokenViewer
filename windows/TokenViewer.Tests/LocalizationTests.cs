using TokenViewerWindows.Services;
using Xunit;

namespace TokenViewerWindows.Tests;

public class LocalizationTests
{
    [Fact]
    public void Every_key_has_non_empty_en_and_zh()
    {
        foreach (var (key, (en, zh)) in L10n.Catalog)
        {
            Assert.False(string.IsNullOrWhiteSpace(en), $"key '{key}' has empty English");
            Assert.False(string.IsNullOrWhiteSpace(zh), $"key '{key}' has empty Chinese");
        }
    }

    [Fact]
    public void Required_domains_are_covered()
    {
        var required = new[]
        {
            // Usage
            "usage", "usageTitle", "usageSubtitle", "syncNow", "rangeToday", "rangeYesterday",
            "rangeWeek", "rangeMonth", "rangeAll", "rangeCustom", "today", "sevenDays",
            "thirtyDays", "total", "perDay", "usageTrend", "byDay", "byHour", "input",
            "output", "cacheRead", "reasoning", "cost", "usageDailyDetails",
            // Limits
            "limits", "limitsTitle", "limitsSubtitle", "notConfigured", "refreshLimits",
            "heatmapLess", "heatmapMore",
            // Settings
            "settings", "settingsTitle", "appearance", "theme", "currency", "languageLabel",
            "general", "launchAtLogin", "syncFrequency", "rebuildData", "rebuildConfirm",
            "resetSettings", "resetSettingsConfirm", "cancel",
            // About
            "about", "aboutSupportedAgents",
            // Update
            "updates", "checkNow", "updateAvailableMessage", "download",
            // Tray / panel
            "dashboard", "quit", "menuBarPanel", "topModels",
            // Error / confirm
            "statusSyncing", "statusReady", "statusSyncFailed", "initFailed",
        };
        foreach (var key in required)
        {
            Assert.True(L10n.Catalog.ContainsKey(key), $"missing key '{key}'");
        }
    }

    [Fact]
    public void Format_methods_include_parameters()
    {
        var l10n = L10n.Instance;
        try
        {
            l10n.Language = "en";
            Assert.Contains("3", l10n.ExpiresInDays(3));
            Assert.Contains("1.2.3", l10n.DownloadingUpdate("1.2.3"));
            Assert.Contains("2026", l10n.CopyrightFooter(2026));
            Assert.Contains("5", l10n.RecordsCount(5));
            Assert.Contains("42", l10n.UsageActiveDays(42));
            Assert.Contains("7", l10n.CountdownText(TimeSpan.FromMinutes(7)));

            l10n.Language = "zh";
            Assert.Contains("3", l10n.ExpiresInDays(3));
            Assert.Contains("1.2.3", l10n.DownloadingUpdate("1.2.3"));
            Assert.Contains("2026", l10n.CopyrightFooter(2026));
        }
        finally
        {
            l10n.Language = "system";
        }
    }
}
