using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Linq;
using System.Windows.Input;
using System.Windows.Threading;
using TokenViewerWindows.Infrastructure;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;

namespace TokenViewerWindows.ViewModels;

/// <summary>
/// Owns shell-level state only: agent status plus a status line that mirrors the
/// shared <see cref="SyncCoordinator"/>. It does not maintain its own usage
/// summary — that belongs to <see cref="UsageViewModel"/>.
/// </summary>
public sealed class MainViewModel : ObservableObject
{
    private readonly ICoreBridge _core;
    private readonly SyncCoordinator _sync;
    private readonly Dispatcher _dispatcher;
    private readonly DispatcherTimer _syncTimer;
    private readonly AsyncRelayCommand _syncCommand;
    private string _status;

    public MainViewModel(ICoreBridge core, SyncCoordinator sync, Dispatcher dispatcher)
    {
        _core = core;
        _sync = sync;
        _dispatcher = dispatcher;
        Agents = new ObservableCollection<AgentStatus>();
        _syncCommand = new AsyncRelayCommand(() => _sync.SyncAsync(), () => !_sync.IsSyncing);
        _status = L10n.Instance["statusReady"];
        _syncTimer = new DispatcherTimer { Interval = TimeSpan.FromMinutes(30) };
        _syncTimer.Tick += async (_, _) => await _sync.SyncAsync();
        _sync.PropertyChanged += OnSyncPropertyChanged;
    }

    public ObservableCollection<AgentStatus> Agents { get; }

    public string Status
    {
        get => _status;
        private set => SetProperty(ref _status, value);
    }

    public ICommand SyncCommand => _syncCommand;

    public void StartAutoSync(int minutes)
    {
        _syncTimer.Stop();
        if (minutes <= 0) return;
        _syncTimer.Interval = TimeSpan.FromMinutes(minutes);
        _syncTimer.Start();
    }

    public void RefreshAgents()
    {
        _ = Task.Run(() =>
        {
            var agents = _core.GetAgentStatus();
            _dispatcher.Invoke(() =>
            {
                Agents.Clear();
                foreach (var agent in agents.OrderByDescending(p => p.Installed).ThenBy(p => p.Source))
                {
                    Agents.Add(agent);
                }
            });
        });
    }

    private void OnSyncPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(SyncCoordinator.IsSyncing))
        {
            _syncCommand.RaiseCanExecuteChanged();
        }

        if (e.PropertyName is nameof(SyncCoordinator.IsSyncing) or nameof(SyncCoordinator.Error) or nameof(SyncCoordinator.Status))
        {
            var l = L10n.Instance;
            Status = _sync.IsSyncing
                ? l["statusSyncing"]
                : _sync.Error is { } message
                    ? $"{l["statusSyncFailed"]}: {message}"
                    : l["statusReady"];
        }
    }
}
