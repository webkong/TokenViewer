using TokenViewerWindows.Infrastructure;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;

namespace TokenViewerWindows.ViewModels;

public sealed class SettingsViewModel : ObservableObject
{
    private readonly SettingsStore _store = new();
    private AppSettings _settings;
    private string _theme = "system";
    private string _language = "system";
    private int _syncFrequencyMinutes = 30;
    private bool _launchAtStartup;

    public SettingsViewModel()
    {
        _settings = _store.Load();
        _theme = _settings.Theme;
        _language = _settings.Language;
        _syncFrequencyMinutes = _settings.SyncFrequencyMinutes;
        _launchAtStartup = LaunchAtStartupManager.IsEnabled || _settings.LaunchAtStartup;
    }

    public string Theme
    {
        get => _theme;
        set
        {
            if (SetProperty(ref _theme, value))
            {
                Persist();
            }
        }
    }

    public string Language
    {
        get => _language;
        set
        {
            if (SetProperty(ref _language, value))
            {
                Persist();
            }
        }
    }

    public int SyncFrequencyMinutes
    {
        get => _syncFrequencyMinutes;
        set
        {
            if (SetProperty(ref _syncFrequencyMinutes, value))
            {
                Persist();
            }
        }
    }

    public bool LaunchAtStartup
    {
        get => _launchAtStartup;
        set
        {
            if (SetProperty(ref _launchAtStartup, value))
            {
                LaunchAtStartupManager.SetEnabled(value);
                Persist();
            }
        }
    }

    public IEnumerable<KeyValuePair<string, string>> ThemeOptions => new[]
    {
        new KeyValuePair<string, string>("system", "System"),
        new KeyValuePair<string, string>("light", "Light"),
        new KeyValuePair<string, string>("dark", "Dark"),
    };

    public IEnumerable<KeyValuePair<string, string>> LanguageOptions => new[]
    {
        new KeyValuePair<string, string>("system", "System"),
        new KeyValuePair<string, string>("en", "English"),
        new KeyValuePair<string, string>("zh", "中文"),
    };

    private void Persist()
    {
        _settings = _settings with
        {
            Theme = _theme,
            Language = _language,
            SyncFrequencyMinutes = _syncFrequencyMinutes,
            LaunchAtStartup = _launchAtStartup,
        };
        _store.Save(_settings);
    }
}
