using System.Collections.Concurrent;
using TokenViewerWindows;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;
using TokenViewerWindows.ViewModels;
using Xunit;

namespace TokenViewerWindows.Tests;

public class UsageViewModelTests
{
    private sealed class FakeCore : ICoreBridge
    {
        private readonly ConcurrentQueue<(TaskCompletionSource? Gate, UsageSummary? Result)> _summaryScript = new();
        private readonly TaskCompletionSource _firstSummaryEntered = new(TaskCreationOptions.RunContinuationsAsynchronously);
        private readonly TaskCompletionSource _heatmapEntered = new(TaskCreationOptions.RunContinuationsAsynchronously);
        private int _summaryCalls;
        private int _dailyCalls;
        private int _hourlyCalls;
        private int _modelCalls;
        private int _heatmapCalls;

        public UsageSummary? SummaryResult { get; set; }
        public DailyPoint[] DailyResult { get; set; } = [];
        public DailyPoint[] HourlyResult { get; set; } = [];
        public ModelEntry[] ModelResult { get; set; } = [];
        public HeatmapPoint[] HeatmapResult { get; set; } = [];

        public int DailyCalls => Volatile.Read(ref _dailyCalls);
        public int HourlyCalls => Volatile.Read(ref _hourlyCalls);
        public int ModelCalls => Volatile.Read(ref _modelCalls);
        public int HeatmapCalls => Volatile.Read(ref _heatmapCalls);
        public int TrendCalls => DailyCalls + HourlyCalls;

        /// <summary>Signals once the first GetSummary call has entered (for a
        /// "first entered" handshake that removes the queue-ordering race).</summary>
        public Task FirstSummaryEntered => _firstSummaryEntered.Task;

        /// <summary>Signals once GetHeatmap (the last query in a range refresh) has run.</summary>
        public Task HeatmapEntered => _heatmapEntered.Task;

        public void ScriptSummary(TaskCompletionSource? gate, UsageSummary? result) =>
            _summaryScript.Enqueue((gate, result));

        public bool IsReady => true;

        public UsageSummary? GetSummary(string from, string to)
        {
            if (Interlocked.Increment(ref _summaryCalls) == 1) _firstSummaryEntered.TrySetResult();
            if (_summaryScript.TryDequeue(out var entry))
            {
                if (entry.Gate is not null) entry.Gate.Task.Wait();
                return entry.Result;
            }
            return SummaryResult;
        }

        public DailyPoint[] GetDaily(string from, string to) { Interlocked.Increment(ref _dailyCalls); return DailyResult; }
        public DailyPoint[] GetHourly(string from, string to) { Interlocked.Increment(ref _hourlyCalls); return HourlyResult; }
        public ModelEntry[] GetModelBreakdown(string from, string to) { Interlocked.Increment(ref _modelCalls); return ModelResult; }

        public HeatmapPoint[] GetHeatmap(int weeks)
        {
            Interlocked.Increment(ref _heatmapCalls);
            _heatmapEntered.TrySetResult();
            return HeatmapResult;
        }

        public AgentStatus[] GetAgentStatus() => [];
        public SyncResult? SyncAll() => new SyncResult(0, 0, []);
        public SyncResult? RebuildAll() => new SyncResult(0, 0, []);
        public void Dispose() { }
    }

    private static UsageSummary Summary(ulong tokens) => new(tokens, 0, 0, 0, 0, 0, 0, 0);

    [Fact]
    public async Task Today_uses_hourly_query()
    {
        var core = new FakeCore();
        var vm = new UsageViewModel(core);
        vm.SelectedRange = UsageViewModel.TimeRange.Today;

        await vm.RefreshAsync();

        Assert.True(core.HourlyCalls > 0);
        Assert.Equal(0, core.DailyCalls);
    }

    [Fact]
    public async Task Week_uses_daily_query()
    {
        var core = new FakeCore();
        var vm = new UsageViewModel(core);
        vm.SelectedRange = UsageViewModel.TimeRange.Week;

        await vm.RefreshAsync();

        Assert.True(core.DailyCalls > 0);
        Assert.Equal(0, core.HourlyCalls);
    }

    [Fact]
    public async Task Empty_data_yields_null_summary_and_empty_collections()
    {
        var core = new FakeCore { SummaryResult = null };
        var vm = new UsageViewModel(core);
        vm.SelectedRange = UsageViewModel.TimeRange.Week;

        await vm.RefreshAsync();

        Assert.Null(vm.Summary);
        Assert.Empty(vm.DailyUsage);
        Assert.Empty(vm.ModelBreakdown);
        Assert.Empty(vm.Heatmap);
        Assert.False(vm.IsLoading);
    }

    [Fact]
    public async Task Single_point_data_is_published()
    {
        var core = new FakeCore
        {
            SummaryResult = Summary(100),
            DailyResult = new[] { new DailyPoint("2026-06-15", 100, 0.5, 60, 40, 10, 5, 20, 3) },
        };
        var vm = new UsageViewModel(core);
        vm.SelectedRange = UsageViewModel.TimeRange.Week;

        await vm.RefreshAsync();

        Assert.NotNull(vm.Summary);
        Assert.Single(vm.DailyUsage);
    }

    [Fact]
    public async Task Default_range_selects_today_when_today_has_tokens()
    {
        var core = new FakeCore { SummaryResult = Summary(100) };
        var vm = new UsageViewModel(core);

        await vm.RefreshAsync();

        Assert.Equal(UsageViewModel.TimeRange.Today, vm.SelectedRange);
    }

    [Fact]
    public async Task Default_range_selects_yesterday_when_today_has_no_tokens()
    {
        var core = new FakeCore { SummaryResult = Summary(0) };
        var vm = new UsageViewModel(core);

        await vm.RefreshAsync();

        Assert.Equal(UsageViewModel.TimeRange.Yesterday, vm.SelectedRange);
    }

    [Fact]
    public async Task User_selection_during_default_check_is_preserved()
    {
        var core = new FakeCore();
        var gate = new TaskCompletionSource();
        core.ScriptSummary(gate, Summary(500)); // default check blocks here

        var vm = new UsageViewModel(core);
        var refresh = vm.RefreshAsync();                      // starts, blocks in default check
        vm.SelectedRange = UsageViewModel.TimeRange.Week;     // user selects during the gap
        gate.SetResult();                                     // unblock the default check
        await refresh;

        Assert.Equal(UsageViewModel.TimeRange.Week, vm.SelectedRange);
    }

    [Fact]
    public async Task Stale_result_is_discarded()
    {
        var core = new FakeCore();
        var slow = new TaskCompletionSource();
        core.ScriptSummary(slow, Summary(500));   // refresh #1 (gated)
        core.ScriptSummary(null, Summary(1000));  // refresh #2 (fast)

        var vm = new UsageViewModel(core);
        vm.SelectedRange = UsageViewModel.TimeRange.Week; // skip default check

        var first = vm.RefreshAsync();              // token 1
        await core.FirstSummaryEntered;             // handshake: #1 entered the gate
        var second = vm.RefreshAsync();             // token 2, safe to start now

        await second;
        slow.SetResult();                           // unblock #1
        await first;

        Assert.Equal(1000UL, vm.Summary!.TotalTokens); // #2 wins; #1 discarded
    }

    [Fact]
    public async Task Sync_completion_refreshes_each_query_once()
    {
        var core = new FakeCore { SummaryResult = Summary(0) }; // zero tokens -> Yesterday
        var sync = new SyncCoordinator(core);
        var vm = new UsageViewModel(core, sync); // self-subscribes to SyncCompleted

        await sync.SyncAsync();

        // Wait until the range refresh has finished querying (GetHeatmap is last).
        await core.HeatmapEntered;

        // A single sync must drive exactly one Usage refresh (no duplicate owner).
        Assert.Equal(1, core.TrendCalls);
        Assert.Equal(1, core.ModelCalls);
        Assert.Equal(1, core.HeatmapCalls);
    }

    [Fact]
    public async Task Setting_selected_range_triggers_refresh()
    {
        var core = new FakeCore();
        var vm = new UsageViewModel(core);

        vm.SelectedRange = UsageViewModel.TimeRange.Today; // setter triggers a refresh

        await core.HeatmapEntered;
        Assert.Equal(1, core.HeatmapCalls);
        Assert.Equal(0, core.DailyCalls); // single-day range uses hourly
    }
}
