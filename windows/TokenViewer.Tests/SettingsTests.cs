using System.IO;
using TokenViewerWindows;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;
using TokenViewerWindows.ViewModels;
using Xunit;

namespace TokenViewerWindows.Tests;

public class SettingsTests
{
    private static string TempSettingsPath()
    {
        var dir = Path.Combine(Path.GetTempPath(), "tv-settings-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(dir);
        return Path.Combine(dir, "settings.json");
    }

    [Fact]
    public void Load_returns_defaults_when_file_missing()
    {
        var path = TempSettingsPath();
        try
        {
            var store = new SettingsStore(path);
            var s = store.Load();
            Assert.Equal("system", s.Theme);
            Assert.Equal("USD", s.Currency);
            Assert.Equal(30, s.SyncFrequencyMinutes);
            Assert.True(s.ShowMenuBarIcon);
            Assert.True(s.PanelShowSummary);
            Assert.True(s.PanelShowModels);
        }
        finally
        {
            CleanupTemp(path);
        }
    }

    [Fact]
    public void Load_preserves_legacy_fields_and_defaults_new_fields()
    {
        var path = TempSettingsPath();
        try
        {
            File.WriteAllText(path, """{"Theme":"dark","Language":"zh","SyncFrequencyMinutes":15,"LaunchAtStartup":true}""");
            var s = new SettingsStore(path).Load();

            Assert.Equal("dark", s.Theme);
            Assert.Equal("zh", s.Language);
            Assert.Equal(15, s.SyncFrequencyMinutes);
            Assert.True(s.LaunchAtStartup);
            // New fields fall back to defaults and do not lose the old values.
            Assert.Equal("USD", s.Currency);
            Assert.True(s.ShowMenuBarIcon);
        }
        finally
        {
            CleanupTemp(path);
        }
    }

    [Fact]
    public void Save_then_Load_roundtrips()
    {
        var path = TempSettingsPath();
        try
        {
            var store = new SettingsStore(path);
            store.Save(new AppSettings { Theme = "light", Currency = "EUR", SyncFrequencyMinutes = 60, ShowMenuBarIcon = false });
            var s = store.Load();
            Assert.Equal("light", s.Theme);
            Assert.Equal("EUR", s.Currency);
            Assert.Equal(60, s.SyncFrequencyMinutes);
            Assert.False(s.ShowMenuBarIcon);
        }
        finally
        {
            CleanupTemp(path);
        }
    }

    [Fact]
    public void ResetSettings_persists_defaults_and_calls_startup_side_effect()
    {
        var tmp = TempSettingsPath();
        try
        {
            File.WriteAllText(tmp, """{"Theme":"dark","Currency":"EUR","SyncFrequencyMinutes":5,"ShowMenuBarIcon":false,"LaunchAtStartup":true}""");
            var store = new SettingsStore(tmp);
            var startupCalls = new List<bool>();
            var vm = new SettingsViewModel(store: store, setLaunchAtStartup: b => startupCalls.Add(b), initialLaunchAtStartup: true);

            vm.ResetSettingsCommand.Execute(null);

            Assert.Equal("system", vm.Theme);
            Assert.Equal("USD", vm.Currency);
            Assert.Equal(30, vm.SyncFrequencyMinutes);
            Assert.True(vm.ShowMenuBarIcon);
            Assert.Contains(false, startupCalls); // launch-at-startup side effect fired on reset

            var reloaded = store.Load();
            Assert.Equal("system", reloaded.Theme);
            Assert.Equal("USD", reloaded.Currency);
        }
        finally
        {
            CleanupTemp(tmp);
        }
    }

    [Fact]
    public void ResetSettings_does_not_touch_data_db()
    {
        var dir = Path.GetDirectoryName(TempSettingsPath())!;
        try
        {
            var dataDb = Path.Combine(dir, "data.db");
            File.WriteAllText(dataDb, "sentinel-usage-data");
            var before = File.GetLastWriteTimeUtc(dataDb);

            var vm = new SettingsViewModel(store: new SettingsStore(Path.Combine(dir, "settings.json")), setLaunchAtStartup: _ => { }, initialLaunchAtStartup: false);
            vm.ResetSettingsCommand.Execute(null);

            Assert.True(File.Exists(dataDb));
            Assert.Equal("sentinel-usage-data", File.ReadAllText(dataDb));
            Assert.Equal(before, File.GetLastWriteTimeUtc(dataDb));
        }
        finally
        {
            CleanupTemp(Path.Combine(dir, "settings.json"));
        }
    }

    private sealed class FakeCore : ICoreBridge
    {
        public SyncResult? RebuildResult { get; set; }
        public bool IsReady => true;
        public UsageSummary? GetSummary(string from, string to) => null;
        public DailyPoint[] GetDaily(string from, string to) => [];
        public DailyPoint[] GetHourly(string from, string to) => [];
        public ModelEntry[] GetModelBreakdown(string from, string to) => [];
        public HeatmapPoint[] GetHeatmap(int weeks) => [];
        public AgentStatus[] GetAgentStatus() => [];
        public SyncResult? SyncAll() => new SyncResult(0, 0, []);
        public SyncResult? RebuildAll() => RebuildResult;
        public void Dispose() { }
    }

    [Fact]
    public async Task Rebuild_failure_does_not_show_done()
    {
        var path = TempSettingsPath();
        try
        {
            var core = new FakeCore { RebuildResult = null };
            var sync = new SyncCoordinator(core);
            var vm = new SettingsViewModel(sync: sync, setLaunchAtStartup: _ => { }, initialLaunchAtStartup: false, store: new SettingsStore(path));

            await vm.RebuildAsync();

            Assert.NotEqual(L10n.Instance["rebuildDone"], vm.DataStatus);
        }
        finally
        {
            CleanupTemp(path);
        }
    }

    [Fact]
    public void LimitsVisibility_serialization_and_filter()
    {
        var all = LimitsVisibility.AllVisible;
        Assert.True(LimitsVisibility.IsVisible(all, "claude"));
        Assert.True(LimitsVisibility.IsVisible(all, "zcode"));

        var hidden = LimitsVisibility.SetVisible(all, "claude", false);
        Assert.False(LimitsVisibility.IsVisible(hidden, "claude"));
        Assert.True(LimitsVisibility.IsVisible(hidden, "zcode"));

        var restored = LimitsVisibility.SetVisible(hidden, "claude", true);
        Assert.True(LimitsVisibility.IsVisible(restored, "claude"));
    }

    [Fact]
    public void TrayReachabilityPolicy_should_exit_when_tray_hidden()
    {
        Assert.True(TrayReachabilityPolicy.ShouldExitOnClose(false));
        Assert.False(TrayReachabilityPolicy.ShouldExitOnClose(true));
    }

    [Fact]
    public void UserFacingSources_are_26_and_unique()
    {
        var sources = AgentRegistry.UserFacingSources;
        Assert.Equal(26, sources.Count);
        Assert.Equal(sources.Count, sources.Distinct(StringComparer.OrdinalIgnoreCase).Count());
    }

    [Fact]
    public void LimitsVisibility_serializes_in_canonical_order()
    {
        var reordered = new[] { "zcode", "claude", "cursor" };
        Assert.Equal("claude,cursor,zcode", LimitsVisibility.Serialize(reordered));
        // Unknown sources are dropped and order is stable.
        Assert.Equal("claude,cursor,zcode", LimitsVisibility.Serialize(new[] { "zcode", "unknown-x", "cursor", "claude" }));
    }

    [Fact]
    public void LimitsVisibility_migrates_legacy_empty_to_all_visible()
    {
        Assert.True(LimitsVisibility.IsVisible("[]", "claude"));
        Assert.True(LimitsVisibility.IsVisible("", "claude"));
        Assert.True(LimitsVisibility.IsVisible(null!, "claude"));
    }

    [Fact]
    public void SetLimitVisibility_only_affects_filter_not_canonical_sources()
    {
        var before = LimitsService.CanonicalSources.ToArray();
        var hidden = LimitsVisibility.SetVisible(LimitsVisibility.AllVisible, "claude", false);
        Assert.Equal(before, LimitsService.CanonicalSources); // canonical list unchanged
        Assert.False(LimitsVisibility.IsVisible(hidden, "claude"));
        Assert.True(LimitsVisibility.IsVisible(hidden, "codex"));
    }

    [Fact]
    public void SetVisible_legacy_empty_inits_to_all_15()
    {
        var hidden = LimitsVisibility.SetVisible("[]", "claude", false);
        Assert.False(LimitsVisibility.IsVisible(hidden, "claude"));
        Assert.True(LimitsVisibility.IsVisible(hidden, "codex"));
        Assert.Equal(14, LimitsVisibility.Parse(hidden).Count);
    }

    [Fact]
    public void ResetSettings_restores_visibility_and_persists()
    {
        var path = TempSettingsPath();
        try
        {
            var store = new SettingsStore(path);
            var vm = new SettingsViewModel(store: store, setLaunchAtStartup: _ => { }, initialLaunchAtStartup: false);
            vm.SetLimitVisibility("claude", false); // hide one source

            var notified = new List<string>();
            vm.PropertyChanged += (_, e) => notified.Add(e.PropertyName ?? "");

            vm.ResetSettingsCommand.Execute(null);

            Assert.Equal(LimitsVisibility.AllVisible, vm.LimitsVisibleSources);
            Assert.Equal(LimitsVisibility.AllVisible, store.Load().LimitsVisibleSources);
            Assert.Contains(nameof(SettingsViewModel.LimitsVisibleSources), notified);
            Assert.Contains(nameof(SettingsViewModel.LimitVisibilityItems), notified);
        }
        finally
        {
            CleanupTemp(path);
        }
    }

    private static void CleanupTemp(string settingsPath)
    {
        try
        {
            var dir = Path.GetDirectoryName(settingsPath);
            if (dir is not null && Directory.Exists(dir)) Directory.Delete(dir, true);
        }
        catch
        {
            // Best-effort cleanup.
        }
    }
}
