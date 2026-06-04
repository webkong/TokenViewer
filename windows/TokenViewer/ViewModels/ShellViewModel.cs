using System.ComponentModel;
using TokenViewerWindows;

namespace TokenViewerWindows.ViewModels;

public sealed class ShellViewModel
{
    public ShellViewModel(CoreBridge core, System.Windows.Threading.Dispatcher dispatcher)
    {
        Main = new MainViewModel(core, dispatcher);
        Limits = new LimitsViewModel(dispatcher);
        Settings = new SettingsViewModel();
        Updates = new UpdateViewModel(dispatcher);
        Main.StartAutoSync(Settings.SyncFrequencyMinutes);
        Settings.PropertyChanged += OnSettingsChanged;
        Limits.StartAutoRefresh();
        _ = Limits.RefreshAsync();
        Updates.StartAutoCheck();
    }

    public MainViewModel Main { get; }
    public LimitsViewModel Limits { get; }
    public SettingsViewModel Settings { get; }
    public UpdateViewModel Updates { get; }

    private void OnSettingsChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(SettingsViewModel.SyncFrequencyMinutes))
        {
            Main.StartAutoSync(Settings.SyncFrequencyMinutes);
        }
    }
}
