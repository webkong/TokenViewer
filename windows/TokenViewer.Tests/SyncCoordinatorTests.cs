using TokenViewerWindows;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;
using Xunit;

namespace TokenViewerWindows.Tests;

public class SyncCoordinatorTests
{
    private sealed class FakeCore : ICoreBridge
    {
        public Func<SyncResult?> OnSync = () => new SyncResult(1, 10, []);
        public Func<SyncResult?> OnRebuild = () => new SyncResult(1, 10, []);
        public bool IsReady => true;

        public SyncResult? SyncAll() => OnSync();
        public SyncResult? RebuildAll() => OnRebuild();
        public UsageSummary? GetSummary(string from, string to) => null;
        public DailyPoint[] GetDaily(string from, string to) => [];
        public DailyPoint[] GetHourly(string from, string to) => [];
        public ModelEntry[] GetModelBreakdown(string from, string to) => [];
        public HeatmapPoint[] GetHeatmap(int weeks) => [];
        public AgentStatus[] GetAgentStatus() => [];
        public void Dispose() { }
    }

    [Fact]
    public async Task SyncAsync_raises_completion_with_result()
    {
        var core = new FakeCore();
        var coordinator = new SyncCoordinator(core);
        SyncResult? received = null;
        coordinator.SyncCompleted += (_, r) => received = r;

        var result = await coordinator.SyncAsync();

        Assert.NotNull(result);
        Assert.Same(result, received);
        Assert.False(coordinator.IsSyncing);
        Assert.Null(coordinator.Error);
    }

    [Fact]
    public async Task SyncAsync_surfaces_failure_and_raises_failed()
    {
        var core = new FakeCore { OnSync = () => throw new InvalidOperationException("boom") };
        var coordinator = new SyncCoordinator(core);
        string? failed = null;
        coordinator.SyncFailed += (_, message) => failed = message;

        var result = await coordinator.SyncAsync();

        Assert.Null(result);
        Assert.Equal("boom", coordinator.Error);
        Assert.Equal("boom", failed);
        Assert.False(coordinator.IsSyncing);
    }

    [Fact]
    public async Task SyncAsync_ignores_concurrent_second_request()
    {
        var core = new FakeCore();
        var gate = new TaskCompletionSource();
        core.OnSync = () => { gate.Task.Wait(); return new SyncResult(1, 10, []); };
        var coordinator = new SyncCoordinator(core);

        var first = coordinator.SyncAsync();        // acquires the gate, blocks inside SyncAll
        var second = await coordinator.SyncAsync(); // ignored while the first is active

        Assert.Null(second);
        gate.SetResult();
        Assert.NotNull(await first);
    }

    [Fact]
    public async Task RebuildAsync_delegates_to_core()
    {
        var core = new FakeCore();
        var rebuildCalled = false;
        core.OnRebuild = () => { rebuildCalled = true; return new SyncResult(2, 20, []); };
        var coordinator = new SyncCoordinator(core);

        var result = await coordinator.RebuildAsync();

        Assert.True(rebuildCalled);
        Assert.NotNull(result);
        Assert.Equal(2L, result!.AgentsSynced);
    }
}
