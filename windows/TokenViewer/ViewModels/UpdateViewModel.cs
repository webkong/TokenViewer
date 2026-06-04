using System.Windows.Threading;
using TokenViewerWindows.Infrastructure;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;

namespace TokenViewerWindows.ViewModels;

public sealed class UpdateViewModel : ObservableObject
{
    private readonly Dispatcher _dispatcher;
    private readonly DispatcherTimer _timer;
    private bool _isBusy;
    private string _status = "Ready";
    private UpdateRelease? _latest;

    public UpdateViewModel(Dispatcher dispatcher)
    {
        _dispatcher = dispatcher;
        CheckCommand = new AsyncRelayCommand(CheckAsync, () => !IsBusy);
        InstallCommand = new AsyncRelayCommand(InstallAsync, () => CanInstall);
        OpenReleaseCommand = new AsyncRelayCommand(OpenReleaseAsync, () => CanOpenRelease);
        _timer = new DispatcherTimer { Interval = TimeSpan.FromHours(6) };
        _timer.Tick += async (_, _) => await CheckAsync(auto: true);
    }

    public AsyncRelayCommand CheckCommand { get; }
    public AsyncRelayCommand InstallCommand { get; }
    public AsyncRelayCommand OpenReleaseCommand { get; }

    public string CurrentVersion => UpdateService.CurrentVersion;

    public UpdateRelease? LatestRelease
    {
        get => _latest;
        private set
        {
            if (SetProperty(ref _latest, value))
            {
                RaisePropertyChanged(nameof(LatestVersion));
                RaisePropertyChanged(nameof(HasUpdate));
                RaisePropertyChanged(nameof(CanInstall));
                RaisePropertyChanged(nameof(CanOpenRelease));
                InstallCommand.RaiseCanExecuteChanged();
                OpenReleaseCommand.RaiseCanExecuteChanged();
            }
        }
    }

    public string? LatestVersion => LatestRelease?.Version;

    public bool HasUpdate => LatestRelease is not null && UpdateService.IsNewer(LatestRelease.Version, CurrentVersion);

    public bool CanInstall => HasUpdate && LatestRelease?.AssetUrl is not null;
    public bool CanOpenRelease => LatestRelease is not null;

    public bool IsBusy
    {
        get => _isBusy;
        private set
        {
            if (SetProperty(ref _isBusy, value))
            {
                CheckCommand.RaiseCanExecuteChanged();
                InstallCommand.RaiseCanExecuteChanged();
                OpenReleaseCommand.RaiseCanExecuteChanged();
            }
        }
    }

    public string Status
    {
        get => _status;
        private set => SetProperty(ref _status, value);
    }

    public void StartAutoCheck()
    {
        _timer.Start();
        _ = CheckAsync(auto: true);
    }

    public Task CheckAsync() => CheckAsync(auto: false);

    public async Task InstallAsync()
    {
        var release = LatestRelease;
        if (release is null)
        {
            return;
        }

        IsBusy = true;
        Status = $"Opening installer for v{release.Version}…";
        try
        {
            var ok = await UpdateService.DownloadAndOpenAsync(release);
            Status = ok ? $"Installer opened for v{release.Version}" : "Could not open installer";
        }
        catch (Exception ex)
        {
            Status = $"Could not open installer: {ex.Message}";
        }
        finally
        {
            IsBusy = false;
        }
    }

    public async Task OpenReleaseAsync()
    {
        var release = LatestRelease;
        if (release is null)
        {
            return;
        }

        try
        {
            var ok = await Task.Run(() => UpdateService.OpenRelease(new Uri(release.ReleaseUrl)));
            if (!ok)
            {
                Status = "Could not open release page";
            }
        }
        catch (Exception ex)
        {
            Status = $"Could not open release page: {ex.Message}";
        }
    }

    private async Task CheckAsync(bool auto)
    {
        if (IsBusy) return;
        IsBusy = true;
        Status = auto ? "Checking for updates…" : "Checking…";

        try
        {
            var release = await UpdateService.CheckLatestAsync();
            _dispatcher.Invoke(() =>
            {
                LatestRelease = release;
                if (release is null)
                {
                    Status = "Update check failed";
                    return;
                }

                Status = UpdateService.IsNewer(release.Version, CurrentVersion)
                    ? $"v{release.Version} available"
                    : $"Up to date (v{CurrentVersion})";
            });
        }
        catch (Exception ex)
        {
            Status = $"Update check failed: {ex.Message}";
        }
        finally
        {
            IsBusy = false;
        }
    }
}
