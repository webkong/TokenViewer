using System.Collections.ObjectModel;
using System.Linq;
using System.Windows.Threading;
using TokenViewerWindows.Infrastructure;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;

namespace TokenViewerWindows.ViewModels;

public sealed class LimitsViewModel : ObservableObject
{
    private readonly Dispatcher _dispatcher;
    private readonly DispatcherTimer _timer;
    private bool _isLoading;
    private string _status = "Ready";

    public LimitsViewModel(Dispatcher dispatcher)
    {
        _dispatcher = dispatcher;
        Providers = new ObservableCollection<ProviderLimit>();
        _timer = new DispatcherTimer { Interval = TimeSpan.FromMinutes(10) };
        _timer.Tick += async (_, _) => await RefreshAsync();
    }

    public ObservableCollection<ProviderLimit> Providers { get; }

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

    public void StartAutoRefresh()
    {
        _timer.Start();
    }

    public async Task RefreshAsync()
    {
        if (IsLoading) return;
        IsLoading = true;
        Status = "Refreshing limits…";

        try
        {
            var limits = await LimitsService.FetchAllAsync();
            _dispatcher.Invoke(() =>
            {
                Providers.Clear();
                foreach (var limit in limits.OrderByDescending(p => p.Configured).ThenBy(p => p.Name))
                {
                    Providers.Add(limit);
                }
                Status = "Ready";
            });
        }
        catch (Exception ex)
        {
            Status = $"Limits failed: {ex.Message}";
        }
        finally
        {
            IsLoading = false;
        }
    }
}

