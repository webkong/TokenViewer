using System.Globalization;
using System.Windows.Threading;
using TokenViewerWindows.Infrastructure;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;

namespace TokenViewerWindows.ViewModels;

/// <summary>
/// Publishes usage projections (summary, daily/hourly, model breakdown, heatmap,
/// and tray panel cards) for the selected range. A generation token guards
/// against out-of-order results when the range changes rapidly: only the latest
/// refresh is applied.
/// </summary>
public sealed class UsageViewModel : ObservableObject
{
    public enum TimeRange { Today, Yesterday, Week, Month, All, Custom }

    private readonly ICoreBridge _core;
    private readonly Dispatcher? _dispatcher;
    private int _refreshToken;
    private volatile bool _hasAppliedDefaultRange;

    private TimeRange _selectedRange = TimeRange.Week;
    private DateTime _customFrom = DateTime.Today.AddDays(-29);
    private DateTime _customTo = DateTime.Today;
    private bool _isLoading;
    private UsageSummary? _summary;
    private IReadOnlyList<DailyPoint> _dailyUsage = [];
    private IReadOnlyList<ModelEntry> _modelBreakdown = [];
    private IReadOnlyList<HeatmapPoint> _heatmap = [];
    private IReadOnlyList<PanelCard> _panelCards = [];

    public UsageViewModel(ICoreBridge core, SyncCoordinator? sync = null, Dispatcher? dispatcher = null)
    {
        _core = core;
        _dispatcher = dispatcher;
        if (sync is not null)
        {
            sync.SyncCompleted += async (_, _) =>
            {
                await RefreshAsync();
                await RefreshPanelCardsAsync();
            };
        }
    }

    public TimeRange SelectedRange
    {
        get => _selectedRange;
        set
        {
            // Any explicit selection (user or the one-time default apply) marks
            // the initial auto-selection as consumed.
            _hasAppliedDefaultRange = true;
            if (SetProperty(ref _selectedRange, value))
            {
                RaisePropertyChanged(nameof(IsHourlyView));
                _ = RefreshAsync();
            }
        }
    }

    public DateTime CustomFrom
    {
        get => _customFrom;
        set
        {
            if (SetProperty(ref _customFrom, value))
            {
                RaisePropertyChanged(nameof(IsHourlyView));
                _ = RefreshAsync();
            }
        }
    }

    public DateTime CustomTo
    {
        get => _customTo;
        set
        {
            if (SetProperty(ref _customTo, value))
            {
                RaisePropertyChanged(nameof(IsHourlyView));
                _ = RefreshAsync();
            }
        }
    }

    public bool IsLoading
    {
        get => _isLoading;
        private set => SetProperty(ref _isLoading, value);
    }

    public UsageSummary? Summary
    {
        get => _summary;
        private set => SetProperty(ref _summary, value);
    }

    public IReadOnlyList<DailyPoint> DailyUsage
    {
        get => _dailyUsage;
        private set => SetProperty(ref _dailyUsage, value);
    }

    public IReadOnlyList<ModelEntry> ModelBreakdown
    {
        get => _modelBreakdown;
        private set => SetProperty(ref _modelBreakdown, value);
    }

    public IReadOnlyList<HeatmapPoint> Heatmap
    {
        get => _heatmap;
        private set => SetProperty(ref _heatmap, value);
    }

    public IReadOnlyList<PanelCard> PanelCards
    {
        get => _panelCards;
        private set => SetProperty(ref _panelCards, value);
    }

    /// <summary>Single-day windows use hourly granularity for the trend chart.</summary>
    public bool IsHourlyView =>
        SelectedRange is TimeRange.Today or TimeRange.Yesterday ||
        (SelectedRange == TimeRange.Custom && AppTime.IsSameLocalDay(_customFrom, _customTo));

    public string LocalizedTitle(TimeRange range) => range switch
    {
        TimeRange.Today => L10n.Instance["rangeToday"],
        TimeRange.Yesterday => L10n.Instance["rangeYesterday"],
        TimeRange.Week => L10n.Instance["rangeWeek"],
        TimeRange.Month => L10n.Instance["rangeMonth"],
        TimeRange.All => L10n.Instance["rangeAll"],
        _ => L10n.Instance["rangeCustom"],
    };

    public async Task RefreshAsync()
    {
        var token = Interlocked.Increment(ref _refreshToken);
        Post(() => IsLoading = true);

        try
        {
            // One-time default range selection (today vs yesterday), guarded so a
            // range the user has already chosen is never overridden by the async
            // default resolution.
            if (!_hasAppliedDefaultRange)
            {
                var resolved = await ResolveDefaultRangeAsync();
                Post(() =>
                {
                    if (resolved is { } range && !_hasAppliedDefaultRange)
                    {
                        // Set the backing field directly (not via the setter) so the
                        // one-time default apply does not trigger a second refresh.
                        _hasAppliedDefaultRange = true;
                        _selectedRange = range;
                        RaisePropertyChanged(nameof(SelectedRange));
                        RaisePropertyChanged(nameof(IsHourlyView));
                    }
                });
            }

            var (from, to) = DateRangeFor(SelectedRange);
            var useHourly = IsHourlyView;
            var result = await Task.Run(() => QueryRange(from, to, useHourly));
            if (token != Volatile.Read(ref _refreshToken)) return; // stale — a newer refresh superseded this one
            Post(() =>
            {
                Summary = result.Summary;
                DailyUsage = result.Daily;
                ModelBreakdown = result.Models;
                Heatmap = result.Heatmap;
                IsLoading = false;
            });
        }
        catch
        {
            if (token == Volatile.Read(ref _refreshToken)) Post(() => IsLoading = false);
        }
    }

    public async Task RefreshPanelCardsAsync()
    {
        var now = DateTime.UtcNow;
        try
        {
            var cards = await Task.Run(() => FetchPanelCards(now));
            Post(() => PanelCards = cards);
        }
        catch
        {
            // Panel cards are best-effort; leave previous values on failure.
        }
    }

    private Task<TimeRange> ResolveDefaultRangeAsync() => Task.Run(() =>
    {
        var todayRange = AppTime.TrailingLocalDays(1, DateTime.UtcNow);
        var today = _core.GetSummary(todayRange.From, todayRange.To);
        return (today?.TotalTokens ?? 0) > 0 ? TimeRange.Today : TimeRange.Yesterday;
    });

    private (UsageSummary? Summary, IReadOnlyList<DailyPoint> Daily, IReadOnlyList<ModelEntry> Models, IReadOnlyList<HeatmapPoint> Heatmap) QueryRange(string from, string to, bool useHourly)
    {
        var summary = _core.GetSummary(from, to);
        var daily = useHourly ? _core.GetHourly(from, to) : _core.GetDaily(from, to);
        var models = _core.GetModelBreakdown(from, to);
        var heatmap = _core.GetHeatmap(53);
        return (summary, daily, models, heatmap);
    }

    private (string From, string To) DateRangeFor(TimeRange range)
    {
        var now = DateTime.UtcNow;
        var q = range switch
        {
            TimeRange.Today => AppTime.TrailingLocalDays(1, now),
            TimeRange.Yesterday => AppTime.YesterdayLocalDay(now),
            TimeRange.Week => AppTime.TrailingLocalDays(7, now),
            TimeRange.Month => AppTime.TrailingLocalDays(30, now),
            TimeRange.All => AppTime.AllUsage(now),
            _ => AppTime.InclusiveLocalDays(_customFrom, _customTo),
        };
        return (q.From, q.To);
    }

    private IReadOnlyList<PanelCard> FetchPanelCards(DateTime now)
    {
        var l = L10n.Instance;
        var today = SummaryFor(AppTime.TrailingLocalDays(1, now));
        var d7 = SummaryFor(AppTime.TrailingLocalDays(7, now));
        var d30 = SummaryFor(AppTime.TrailingLocalDays(30, now));
        var total = SummaryFor(AppTime.AllUsage(now));
        return new List<PanelCard>
        {
            new(l["today"], FormatTokens(today?.TotalTokens ?? 0), FormatCost(today?.TotalCostUsd ?? 0)),
            new(l["sevenDays"], FormatTokens(d7?.TotalTokens ?? 0), $"{d7?.ActiveDays ?? 0} {l["active"]}"),
            new(l["thirtyDays"], FormatTokens(d30?.TotalTokens ?? 0), $"~{FormatTokens((d30?.TotalTokens ?? 0) / 30)}{l["perDay"]}"),
            new(l["total"], FormatTokens(total?.TotalTokens ?? 0), FormatCost(total?.TotalCostUsd ?? 0)),
        };
    }

    private UsageSummary? SummaryFor(UsageQueryRange range) => _core.GetSummary(range.From, range.To);

    private static string FormatTokens(ulong value) => value.ToString("N0", CultureInfo.InvariantCulture);
    private static string FormatCost(double value) => $"${value:0.00}";

    private void Post(Action action)
    {
        if (_dispatcher is { } dispatcher && !dispatcher.CheckAccess())
            dispatcher.Invoke(action);
        else
            action();
    }
}
