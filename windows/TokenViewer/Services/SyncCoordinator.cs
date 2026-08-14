using System.Windows.Threading;
using TokenViewerWindows.Infrastructure;
using TokenViewerWindows.Models;

namespace TokenViewerWindows.Services;

/// <summary>
/// The single owner of <c>tt_sync_all</c> / <c>tt_rebuild_all</c>. Serializes
/// sync/rebuild with one shared gate (a concurrent request is ignored while one
/// is active), publishes shared state on the dispatcher, and raises a single
/// <see cref="SyncCompleted"/> event that every projection refreshes from.
/// </summary>
public sealed class SyncCoordinator : ObservableObject
{
    private readonly ICoreBridge _core;
    private readonly Dispatcher? _dispatcher;
    private readonly SemaphoreSlim _gate = new(1, 1);
    private bool _isSyncing;
    private string _status;
    private string? _error;

    public event EventHandler<SyncResult?>? SyncCompleted;
    public event EventHandler<string>? SyncFailed;

    public SyncCoordinator(ICoreBridge core, Dispatcher? dispatcher = null)
    {
        _core = core;
        _dispatcher = dispatcher;
        _status = L10n.Instance["statusReady"];
    }

    public bool IsSyncing
    {
        get => _isSyncing;
        private set => SetProperty(ref _isSyncing, value);
    }

    public string Status
    {
        get => _status;
        private set => SetProperty(ref _status, value);
    }

    public string? Error
    {
        get => _error;
        private set => SetProperty(ref _error, value);
    }

    public Task<SyncResult?> SyncAsync() => RunAsync(core => core.SyncAll());

    public Task<SyncResult?> RebuildAsync() => RunAsync(core => core.RebuildAll());

    private async Task<SyncResult?> RunAsync(Func<ICoreBridge, SyncResult?> work)
    {
        // A second request while one is active is ignored (returns null).
        if (!await _gate.WaitAsync(0)) return null;
        try
        {
            Post(() => { IsSyncing = true; Status = L10n.Instance["statusSyncing"]; Error = null; });
            try
            {
                var result = await Task.Run(() => work(_core));
                Post(() =>
                {
                    IsSyncing = false;
                    Status = L10n.Instance["statusReady"];
                    SyncCompleted?.Invoke(this, result);
                });
                return result;
            }
            catch (Exception ex)
            {
                Post(() =>
                {
                    IsSyncing = false;
                    Error = ex.Message;
                    Status = $"{L10n.Instance["statusSyncFailed"]}: {ex.Message}";
                    SyncFailed?.Invoke(this, ex.Message);
                });
                return null;
            }
        }
        finally
        {
            _gate.Release();
        }
    }

    private void Post(Action action)
    {
        if (_dispatcher is { } dispatcher && !dispatcher.CheckAccess())
            dispatcher.Invoke(action);
        else
            action();
    }
}
