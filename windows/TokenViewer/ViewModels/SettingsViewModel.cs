using TokenViewerWindows.Infrastructure;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;

namespace TokenViewerWindows.ViewModels;

/// <summary>Pure serialization/filtering for the tray popover's limits-card
/// visibility (a stable comma-separated source list). The main Limits page always
/// shows all 15 canonical agents; this only filters the tray popover.</summary>
public static class LimitsVisibility
{
    public static string AllVisible => string.Join(",", LimitsService.CanonicalSources);

    public static IReadOnlyList<string> Parse(string raw) =>
        string.IsNullOrWhiteSpace(raw)
            ? Array.Empty<string>()
            : raw.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries).ToList();

    /// <summary>Serializes in canonical order, dropping unknown sources, so the
    /// output is stable and comparable.</summary>
    public static string Serialize(IEnumerable<string> sources)
    {
        var set = sources.ToHashSet(StringComparer.OrdinalIgnoreCase);
        return string.Join(",", LimitsService.CanonicalSources.Where(set.Contains));
    }

    /// <summary>An empty or legacy "[]" value means all 15 canonical agents are
    /// visible (migrated on read).</summary>
    public static bool IsVisible(string raw, string source)
    {
        var parsed = Parse(raw);
        if (parsed.Count == 0) return true;
        return parsed.Contains(source, StringComparer.OrdinalIgnoreCase);
    }

    public static string SetVisible(string raw, string source, bool visible)
    {
        var set = Parse(raw).ToHashSet(StringComparer.OrdinalIgnoreCase);
        // Legacy/empty input means all 15 were visible — initialize to that set
        // before applying the toggle, so a hide can actually drop a source.
        if (set.Count == 0) set = LimitsService.CanonicalSources.ToHashSet(StringComparer.OrdinalIgnoreCase);
        if (visible) set.Add(source);
        else set.Remove(source);
        return Serialize(set);
    }
}

public sealed class SettingsViewModel : ObservableObject
{
    private readonly SettingsStore _store;
    private readonly SyncCoordinator? _sync;
    private readonly Action<bool>? _setLaunchAtStartup;
    private AppSettings _settings;
    private string _theme;
    private string _language;
    private string _currency;
    private int _syncFrequencyMinutes;
    private bool _launchAtStartup;
    private bool _showMenuBarIcon;
    private bool _panelShowSummary;
    private bool _panelShowLimits;
    private bool _panelShowHeatmap;
    private bool _panelShowTrend;
    private bool _panelShowModels;
    private string _limitsVisibleSources;
    private bool _isRebuilding;
    private string _dataStatus = "";

    public SettingsViewModel(
        SyncCoordinator? sync = null,
        Action<bool>? setLaunchAtStartup = null,
        bool? initialLaunchAtStartup = null,
        SettingsStore? store = null)
    {
        _sync = sync;
        _setLaunchAtStartup = setLaunchAtStartup ?? LaunchAtStartupManager.SetEnabled;
        _store = store ?? new SettingsStore();
        _settings = _store.Load();
        _theme = _settings.Theme;
        _language = _settings.Language;
        _currency = _settings.Currency;
        _syncFrequencyMinutes = _settings.SyncFrequencyMinutes;
        _launchAtStartup = (initialLaunchAtStartup ?? LaunchAtStartupManager.IsEnabled) || _settings.LaunchAtStartup;
        _showMenuBarIcon = _settings.ShowMenuBarIcon;
        _panelShowSummary = _settings.PanelShowSummary;
        _panelShowLimits = _settings.PanelShowLimits;
        _panelShowHeatmap = _settings.PanelShowHeatmap;
        _panelShowTrend = _settings.PanelShowTrend;
        _panelShowModels = _settings.PanelShowModels;
        _limitsVisibleSources = string.IsNullOrWhiteSpace(_settings.LimitsVisibleSources) || _settings.LimitsVisibleSources == "[]"
            ? LimitsVisibility.AllVisible
            : _settings.LimitsVisibleSources;

        RebuildCommand = new AsyncRelayCommand(RebuildAsync, () => _sync is not null && !_sync.IsSyncing);
        ResetSettingsCommand = new AsyncRelayCommand(ResetSettingsAsync);
        if (_sync is not null)
        {
            _sync.PropertyChanged += (_, e) =>
            {
                if (e.PropertyName == nameof(SyncCoordinator.IsSyncing)) RebuildCommand.RaiseCanExecuteChanged();
            };
        }

        // Refresh localized option labels when the language changes.
        L10n.Instance.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == "Item[]")
            {
                RaisePropertyChanged(nameof(ThemeOptions));
                RaisePropertyChanged(nameof(LanguageOptions));
                RaisePropertyChanged(nameof(CurrencyOptions));
                RaisePropertyChanged(nameof(SyncFrequencyOptions));
            }
        };
    }

    public AsyncRelayCommand RebuildCommand { get; }
    public AsyncRelayCommand ResetSettingsCommand { get; }

    public string Theme
    {
        get => _theme;
        set { if (SetProperty(ref _theme, value)) Persist(); }
    }

    public string Language
    {
        get => _language;
        set { if (SetProperty(ref _language, value)) Persist(); }
    }

    public string Currency
    {
        get => _currency;
        set { if (SetProperty(ref _currency, value)) Persist(); }
    }

    public int SyncFrequencyMinutes
    {
        get => _syncFrequencyMinutes;
        set { if (SetProperty(ref _syncFrequencyMinutes, value)) Persist(); }
    }

    public bool LaunchAtStartup
    {
        get => _launchAtStartup;
        set
        {
            if (SetProperty(ref _launchAtStartup, value))
            {
                _setLaunchAtStartup?.Invoke(value);
                Persist();
            }
        }
    }

    public bool ShowMenuBarIcon
    {
        get => _showMenuBarIcon;
        set { if (SetProperty(ref _showMenuBarIcon, value)) Persist(); }
    }

    public bool PanelShowSummary { get => _panelShowSummary; set { if (SetProperty(ref _panelShowSummary, value)) Persist(); } }
    public bool PanelShowLimits { get => _panelShowLimits; set { if (SetProperty(ref _panelShowLimits, value)) Persist(); } }
    public bool PanelShowHeatmap { get => _panelShowHeatmap; set { if (SetProperty(ref _panelShowHeatmap, value)) Persist(); } }
    public bool PanelShowTrend { get => _panelShowTrend; set { if (SetProperty(ref _panelShowTrend, value)) Persist(); } }
    public bool PanelShowModels { get => _panelShowModels; set { if (SetProperty(ref _panelShowModels, value)) Persist(); } }

    /// <summary>Limit-agent visibility chips (the 15 canonical agents), each with
    /// a two-way IsVisible that persists to <see cref="LimitsVisibleSources"/>.</summary>
    public IReadOnlyList<LimitVisibilityItem> LimitVisibilityItems =>
        LimitsService.CanonicalSources
            .Select(s => new LimitVisibilityItem(s, AgentRegistry.DisplayName(s), LimitsVisibility.IsVisible(_limitsVisibleSources, s), v => SetLimitVisibility(s, v)))
            .ToList();

    public string LimitsVisibleSources
    {
        get => _limitsVisibleSources;
        private set
        {
            if (SetProperty(ref _limitsVisibleSources, value))
            {
                RaisePropertyChanged(nameof(LimitVisibilityItems));
                Persist();
            }
        }
    }

    public void SetLimitVisibility(string source, bool visible)
    {
        LimitsVisibleSources = LimitsVisibility.SetVisible(_limitsVisibleSources, source, visible);
    }

    public bool IsRebuilding
    {
        get => _isRebuilding;
        private set => SetProperty(ref _isRebuilding, value);
    }

    public string DataStatus
    {
        get => _dataStatus;
        private set => SetProperty(ref _dataStatus, value);
    }

    public IEnumerable<KeyValuePair<string, string>> ThemeOptions => new[]
    {
        new KeyValuePair<string, string>("system", L10n.Instance["themeSystem"]),
        new KeyValuePair<string, string>("light", L10n.Instance["themeLight"]),
        new KeyValuePair<string, string>("dark", L10n.Instance["themeDark"]),
    };

    public IEnumerable<KeyValuePair<string, string>> LanguageOptions => new[]
    {
        new KeyValuePair<string, string>("system", L10n.Instance["themeSystem"]),
        new KeyValuePair<string, string>("en", "English"),
        new KeyValuePair<string, string>("zh", "中文"),
    };

    public IEnumerable<KeyValuePair<string, string>> CurrencyOptions => new[]
    {
        new KeyValuePair<string, string>("USD", "USD $"),
        new KeyValuePair<string, string>("CNY", "CNY ¥"),
        new KeyValuePair<string, string>("JPY", "JPY ¥"),
        new KeyValuePair<string, string>("EUR", "EUR €"),
        new KeyValuePair<string, string>("GBP", "GBP £"),
        new KeyValuePair<string, string>("KRW", "KRW ₩"),
    };

    public IEnumerable<KeyValuePair<int, string>> SyncFrequencyOptions => new[]
    {
        new KeyValuePair<int, string>(5, L10n.Instance["sync5min"]),
        new KeyValuePair<int, string>(10, L10n.Instance["sync10min"]),
        new KeyValuePair<int, string>(15, L10n.Instance["sync15min"]),
        new KeyValuePair<int, string>(30, L10n.Instance["sync30min"]),
        new KeyValuePair<int, string>(60, L10n.Instance["sync1hour"]),
        new KeyValuePair<int, string>(0, L10n.Instance["manual"]),
    };

    public async Task RebuildAsync()
    {
        if (_sync is null || IsRebuilding) return;
        IsRebuilding = true;
        DataStatus = L10n.Instance["rebuildDataHint"];
        try
        {
            var result = await _sync.RebuildAsync();
            DataStatus = result is not null ? L10n.Instance["rebuildDone"] : L10n.Instance["statusSyncFailed"];
        }
        finally
        {
            IsRebuilding = false;
        }
    }

    private Task ResetSettingsAsync()
    {
        // Restore defaults WITHOUT deleting usage data — that only happens on rebuild.
        // Assigning via the property setters fires the per-property side effects
        // (launch-at-startup, language, sync-frequency, tray visibility) and persists.
        var defaults = new AppSettings();
        Theme = defaults.Theme;
        Language = defaults.Language;
        Currency = defaults.Currency;
        SyncFrequencyMinutes = defaults.SyncFrequencyMinutes;
        LaunchAtStartup = defaults.LaunchAtStartup;
        ShowMenuBarIcon = defaults.ShowMenuBarIcon;
        PanelShowSummary = defaults.PanelShowSummary;
        PanelShowLimits = defaults.PanelShowLimits;
        PanelShowHeatmap = defaults.PanelShowHeatmap;
        PanelShowTrend = defaults.PanelShowTrend;
        PanelShowModels = defaults.PanelShowModels;
        LimitsVisibleSources = LimitsVisibility.AllVisible;
        DataStatus = L10n.Instance["toastSettingsReset"];
        return Task.CompletedTask;
    }

    private void Persist()
    {
        _settings = _settings with
        {
            Theme = _theme,
            Language = _language,
            Currency = _currency,
            SyncFrequencyMinutes = _syncFrequencyMinutes,
            LaunchAtStartup = _launchAtStartup,
            ShowMenuBarIcon = _showMenuBarIcon,
            PanelShowSummary = _panelShowSummary,
            PanelShowLimits = _panelShowLimits,
            PanelShowHeatmap = _panelShowHeatmap,
            PanelShowTrend = _panelShowTrend,
            PanelShowModels = _panelShowModels,
            LimitsVisibleSources = _limitsVisibleSources,
        };
        _store.Save(_settings);
    }
}

/// <summary>A single limit-agent visibility chip with two-way binding that
/// persists on change.</summary>
public sealed class LimitVisibilityItem : ObservableObject
{
    private readonly Action<bool> _onChanged;
    private bool _isVisible;

    public LimitVisibilityItem(string source, string name, bool isVisible, Action<bool> onChanged)
    {
        Source = source;
        Name = name;
        _isVisible = isVisible;
        _onChanged = onChanged;
    }

    public string Source { get; }
    public string Name { get; }

    public bool IsVisible
    {
        get => _isVisible;
        set
        {
            if (SetProperty(ref _isVisible, value))
            {
                _onChanged(value);
            }
        }
    }
}
