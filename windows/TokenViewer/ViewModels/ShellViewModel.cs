using System.ComponentModel;
using System.Windows.Threading;
using TokenViewerWindows.Services;

namespace TokenViewerWindows.ViewModels;

public sealed class ShellViewModel
{
    public ShellViewModel(ICoreBridge core, Dispatcher dispatcher)
    {
        Sync = new SyncCoordinator(core, dispatcher);
        Usage = new UsageViewModel(core, Sync, dispatcher);
        Main = new MainViewModel(core, Sync, dispatcher);
        Limits = new LimitsViewModel(dispatcher);
        Settings = new SettingsViewModel(Sync);
        Updates = new UpdateViewModel(dispatcher);

        L10n.Instance.Language = Settings.Language;

        Main.StartAutoSync(Settings.SyncFrequencyMinutes);
        Settings.PropertyChanged += OnSettingsChanged;
        Limits.StartAutoRefresh();
        _ = Limits.RefreshAsync();
        Updates.StartAutoCheck();

        // UsageViewModel subscribes to SyncCompleted itself (single refresh
        // owner). The shell only refreshes agent status on completion.
        Sync.SyncCompleted += (_, _) => Main.RefreshAgents();
    }

    public SyncCoordinator Sync { get; }
    public UsageViewModel Usage { get; }
    public MainViewModel Main { get; }
    public LimitsViewModel Limits { get; }
    public SettingsViewModel Settings { get; }
    public UpdateViewModel Updates { get; }

    private void OnSettingsChanged(object? sender, PropertyChangedEventArgs e)
    {
        switch (e.PropertyName)
        {
            case nameof(SettingsViewModel.SyncFrequencyMinutes):
                Main.StartAutoSync(Settings.SyncFrequencyMinutes);
                break;
            case nameof(SettingsViewModel.Language):
                L10n.Instance.Language = Settings.Language;
                break;
        }
    }
}
