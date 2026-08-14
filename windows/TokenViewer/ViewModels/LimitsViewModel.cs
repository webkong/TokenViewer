using System.Collections.ObjectModel;
using System.Windows.Threading;
using TokenViewerWindows.Infrastructure;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;

namespace TokenViewerWindows.ViewModels;

public sealed class LimitsViewModel : ObservableObject
{
    private readonly Dispatcher _dispatcher;
    private readonly DispatcherTimer _refreshTimer;
    private readonly DispatcherTimer _countdownTimer;
    private bool _isLoading;
    private string _status;
    private DateTime _now = DateTime.Now;

    public LimitsViewModel(Dispatcher dispatcher)
    {
        _dispatcher = dispatcher;
        Agents = new ObservableCollection<AgentLimit>();
        _status = L10n.Instance["statusReady"];

        _refreshTimer = new DispatcherTimer { Interval = TimeSpan.FromMinutes(10) };
        _refreshTimer.Tick += async (_, _) => await RefreshAsync();

        // Drives the per-minute reset countdown; lives on the app-level view model
        // (constructed once), so it does not leak per window.
        _countdownTimer = new DispatcherTimer { Interval = TimeSpan.FromMinutes(1) };
        _countdownTimer.Tick += (_, _) => Now = DateTime.Now;
        _countdownTimer.Start();
    }

    public ObservableCollection<AgentLimit> Agents { get; }

    /// <summary>Configured agents with a limit display, sorted first.</summary>
    public IReadOnlyList<AgentLimit> ActiveAgents =>
        Agents.Where(a => a.Configured && a.HasLimitDisplay).ToList();

    public IReadOnlyList<AgentLimit> InactiveAgents =>
        Agents.Where(a => !(a.Configured && a.HasLimitDisplay)).ToList();

    public bool IsLoading
    {
        get => _isLoading;
        private set => SetProperty(ref _isLoading, value);
    }

    public string Status
    {
        get => _status;
        private set => SetProperty(ref _status, value);
    }

    /// <summary>Current time, bumped once a minute so countdown bindings re-evaluate.</summary>
    public DateTime Now
    {
        get => _now;
        private set => SetProperty(ref _now, value);
    }

    public void StartAutoRefresh() => _refreshTimer.Start();

    public async Task RefreshAsync()
    {
        if (IsLoading) return;
        IsLoading = true;
        Status = L10n.Instance["refreshingLimits"];

        try
        {
            var limits = await LimitsService.FetchAllAsync();
            _dispatcher.Invoke(() =>
            {
                Agents.Clear();
                foreach (var limit in limits.OrderByDescending(p => p.Configured).ThenBy(p => p.Name))
                {
                    Agents.Add(limit);
                }
                Status = L10n.Instance["statusReady"];
                RaisePropertyChanged(nameof(ActiveAgents));
                RaisePropertyChanged(nameof(InactiveAgents));
            });
        }
        catch
        {
            Status = L10n.Instance["statusSyncFailed"];
        }
        finally
        {
            IsLoading = false;
        }
    }
}
