using System.Collections.ObjectModel;
using System.Linq;
using System.Globalization;
using System.Windows.Input;
using System.Windows.Threading;
using TokenViewerWindows;
using TokenViewerWindows.Infrastructure;
using TokenViewerWindows.Models;

namespace TokenViewerWindows.ViewModels;

public sealed class MainViewModel : ObservableObject
{
    private readonly CoreBridge _core;
    private readonly Dispatcher _dispatcher;
    private readonly DispatcherTimer _syncTimer;
    private bool _isLoading;
    private UsageSummary? _summary;
    private string _status = "Ready";

    public MainViewModel(CoreBridge core, Dispatcher dispatcher)
    {
        _core = core;
        _dispatcher = dispatcher;
        Providers = new ObservableCollection<ProviderStatus>();
        SyncCommand = new AsyncRelayCommand(SyncAsync, () => !IsLoading);
        _syncTimer = new DispatcherTimer { Interval = TimeSpan.FromMinutes(30) };
        _syncTimer.Tick += async (_, _) => await SyncAsync();
    }

    public ObservableCollection<ProviderStatus> Providers { get; }

    public bool IsLoading
    {
        get => _isLoading;
        private set
        {
            if (SetProperty(ref _isLoading, value) && SyncCommand is AsyncRelayCommand command)
            {
                command.RaiseCanExecuteChanged();
            }
        }
    }

    public UsageSummary? Summary
    {
        get => _summary;
        private set => SetProperty(ref _summary, value);
    }

    public string Status
    {
        get => _status;
        private set => SetProperty(ref _status, value);
    }

    public string SummaryTokens => FormatTokens(Summary?.TotalTokens ?? 0);
    public string SummaryCost => FormatCost(Summary?.TotalCostUsd ?? 0);
    public ICommand SyncCommand { get; }

    public void StartAutoSync(int minutes)
    {
        _syncTimer.Stop();
        if (minutes <= 0) return;
        _syncTimer.Interval = TimeSpan.FromMinutes(minutes);
        _syncTimer.Start();
    }

    public async Task SyncAsync()
    {
        if (IsLoading) return;
        IsLoading = true;
        Status = "Syncing…";

        try
        {
            await Task.Run(() => _core.SyncAll());
            Refresh();
            Status = "Ready";
        }
        catch (Exception ex)
        {
            Status = $"Sync failed: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }

    public void Refresh()
    {
        var now = DateTime.Now;
        var todayStart = now.Date;
        var tomorrowStart = todayStart.AddDays(1);
        var from = todayStart.ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss'Z'", CultureInfo.InvariantCulture);
        var to = tomorrowStart.ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ss'Z'", CultureInfo.InvariantCulture);

        var summary = _core.GetSummary(from, to);
        var providers = _core.GetProviderStatus();

        _dispatcher.Invoke(() =>
        {
            Summary = summary;
            Providers.Clear();
            foreach (var provider in providers.OrderByDescending(p => p.Installed).ThenBy(p => p.Source))
            {
                Providers.Add(provider);
            }
            RaisePropertyChanged(nameof(SummaryTokens));
            RaisePropertyChanged(nameof(SummaryCost));
        });
    }

    private static string FormatTokens(ulong value) => value.ToString("N0", CultureInfo.InvariantCulture);
    private static string FormatCost(double value) => $"${value:0.00}";
}
