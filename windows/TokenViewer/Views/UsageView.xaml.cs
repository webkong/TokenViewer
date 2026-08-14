using System.Globalization;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Data;
using System.Windows.Markup;
using System.Windows.Media;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;
using TokenViewerWindows.ViewModels;
using UserControl = System.Windows.Controls.UserControl;
using Brush = System.Windows.Media.Brush;
using Color = System.Windows.Media.Color;
using Binding = System.Windows.Data.Binding;

namespace TokenViewerWindows.Views;

public partial class UsageView : UserControl
{
    public UsageView()
    {
        InitializeComponent();
        // Range options are a fixed enum list; SelectedRange binds two-way.
        RangeList.ItemsSource = Enum.GetValues<UsageViewModel.TimeRange>();
    }
}

/// <summary>Static token/cost formatting, mirroring macOS <c>tvFormatTokens</c> /
/// <c>tvFormatCost</c> (USD-only until the currency store is ported).</summary>
public static class UsageFormats
{
    public static string Tokens(ulong n)
    {
        var d = (double)n;
        if (d >= 1_000_000_000) return $"{d / 1_000_000_000:0.00}B";
        if (d >= 1_000_000) return $"{d / 1_000_000:0.00}M";
        if (d >= 1_000) return $"{d / 1_000:0.0}K";
        return n.ToString(CultureInfo.InvariantCulture);
    }

    public static string Cost(double usd)
    {
        if (usd <= 0) return "$0.00";
        if (usd < 0.01) return "<$0.01";
        if (usd >= 1000) return $"${usd:0}";
        return $"${usd:0.00}";
    }

    public static string Count(uint n) => n.ToString(CultureInfo.InvariantCulture);
}

/// <summary>Markup extension that binds to <c>L10n.Instance[key]</c> (live-updates
/// because <c>L10n</c> raises <c>Item[]</c> on language change).</summary>
public sealed class L10nExtension : MarkupExtension
{
    public L10nExtension() { }
    public L10nExtension(string key) => Key = key;

    [ConstructorArgument("key")]
    public string Key { get; set; } = "";

    public override object ProvideValue(IServiceProvider serviceProvider) =>
        new Binding($"[{Key}]") { Source = L10n.Instance };
}

public sealed class TokensConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        UsageFormats.Tokens(value is ulong v ? v : 0);
    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

public sealed class InverseBoolConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is bool b ? !b : true;
    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is bool b ? !b : false;
}

public sealed class IsCustomRangeToVisibilityConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is UsageViewModel.TimeRange.Custom ? Visibility.Visible : Visibility.Collapsed;
    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

public sealed class NullToVisibilityConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is null || (value is string s && s.Length == 0) ? Visibility.Collapsed : Visibility.Visible;
    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

public sealed class CostConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        UsageFormats.Cost(value is double d ? d : 0);
    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

public sealed class RangeTitleConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is UsageViewModel.TimeRange range
            ? range switch
            {
                UsageViewModel.TimeRange.Today => L10n.Instance["rangeToday"],
                UsageViewModel.TimeRange.Yesterday => L10n.Instance["rangeYesterday"],
                UsageViewModel.TimeRange.Week => L10n.Instance["rangeWeek"],
                UsageViewModel.TimeRange.Month => L10n.Instance["rangeMonth"],
                UsageViewModel.TimeRange.All => L10n.Instance["rangeAll"],
                _ => L10n.Instance["rangeCustom"],
            }
            : "";
    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

public sealed record TokenSegment(string Label, ulong Tokens, Brush Color);

public sealed record BreakdownRow(string Label, ulong Tokens, double Cost, double Percentage);

public sealed record DailyRow(string Date, DailyPoint? Point);

public sealed class TokenSegmentsConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is not UsageSummary s) return Array.Empty<TokenSegment>();
        var l = L10n.Instance;
        var segments = new[]
        {
            new TokenSegment(l["input"], s.InputTokens, InputBrush),
            new TokenSegment(l["output"], s.OutputTokens, OutputBrush),
            new TokenSegment(l["cacheRead"], s.CachedInputTokens, CacheBrush),
            new TokenSegment(l["reasoning"], s.ReasoningOutputTokens, CacheReadBrush),
        };
        return segments.Where(x => x.Tokens > 0).ToArray();
    }

    private static readonly Brush InputBrush = new SolidColorBrush(Color.FromRgb(0x3B, 0x82, 0xF6));
    private static readonly Brush OutputBrush = new SolidColorBrush(Color.FromRgb(0x22, 0xC5, 0x5E));
    private static readonly Brush CacheBrush = new SolidColorBrush(Color.FromRgb(0xF5, 0x9E, 0x0B));
    private static readonly Brush CacheReadBrush = new SolidColorBrush(Color.FromRgb(0x8B, 0x5C, 0xF6));

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

public sealed class CacheHitRateConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is not UsageSummary s) return "";
        var denom = s.InputTokens + s.CachedInputTokens;
        if (denom == 0) return "";
        return $"{s.CachedInputTokens * 100.0 / denom:0.0}%";
    }
    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

public sealed class MergedModelsConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is not IReadOnlyList<ModelEntry> models) return Array.Empty<BreakdownRow>();
        var merged = new Dictionary<string, (ulong Tokens, double Cost)>();
        foreach (var m in models)
        {
            merged.TryGetValue(m.Model, out var e);
            merged[m.Model] = (e.Tokens + m.TotalTokens, e.Cost + m.TotalCostUsd);
        }
        var grand = merged.Values.Sum(x => (double)x.Tokens);
        return merged
            .OrderByDescending(x => x.Value.Tokens)
            .Take(8)
            .Select(x => new BreakdownRow(
                x.Key, x.Value.Tokens, x.Value.Cost,
                grand > 0 ? x.Value.Tokens * 100.0 / grand : 0))
            .ToArray();
    }
    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

public sealed class AgentRollupConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is not IReadOnlyList<ModelEntry> models) return Array.Empty<BreakdownRow>();
        var rolled = new Dictionary<string, (ulong Tokens, double Cost)>();
        foreach (var m in models)
        {
            rolled.TryGetValue(m.Source, out var e);
            rolled[m.Source] = (e.Tokens + m.TotalTokens, e.Cost + m.TotalCostUsd);
        }
        var grand = rolled.Values.Sum(x => (double)x.Tokens);
        return rolled
            .OrderByDescending(x => x.Value.Tokens)
            .Select(x => new BreakdownRow(
                x.Key, x.Value.Tokens, x.Value.Cost,
                grand > 0 ? x.Value.Tokens * 100.0 / grand : 0))
            .ToArray();
    }
    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}

public sealed class DailyRowsConverter : IValueConverter
{
    public object Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is not IReadOnlyList<DailyPoint> data || data.Count == 0) return Array.Empty<DailyRow>();
        var byDate = AggregateByDay(data);
        if (byDate.Count == 0) return Array.Empty<DailyRow>();

        var maxDate = byDate.Keys.Max();
        if (!DateTime.TryParseExact(maxDate, "yyyy-MM-dd", CultureInfo.InvariantCulture, DateTimeStyles.None, out var d))
            return Array.Empty<DailyRow>();

        var rows = new List<DailyRow>(14);
        for (var i = 0; i < 14; i++)
        {
            var key = d.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture);
            rows.Add(new DailyRow(key, byDate.TryGetValue(key, out var p) ? p : null));
            d = d.AddDays(-1);
        }
        return rows;
    }

    /// <summary>Aggregates daily ("yyyy-MM-dd") or hourly ("yyyy-MM-ddTHH")
    /// points into one record per local day.</summary>
    public static IReadOnlyDictionary<string, DailyPoint> AggregateByDay(IReadOnlyList<DailyPoint> data)
    {
        var byDate = new Dictionary<string, DailyPoint>();
        foreach (var p in data)
        {
            var day = p.Date.Length >= 10 ? p.Date[..10] : p.Date;
            byDate[day] = byDate.TryGetValue(day, out var existing)
                ? Sum(existing, p, day)
                : WithDate(p, day);
        }
        return byDate;
    }

    private static DailyPoint WithDate(DailyPoint p, string day) => new(
        day, p.TotalTokens, p.TotalCostUsd, p.InputTokens, p.OutputTokens,
        p.CachedInputTokens, p.CacheCreationInputTokens, p.ReasoningOutputTokens, p.ConversationCount);

    private static DailyPoint Sum(DailyPoint a, DailyPoint b, string day) => new(
        day, a.TotalTokens + b.TotalTokens, a.TotalCostUsd + b.TotalCostUsd,
        a.InputTokens + b.InputTokens, a.OutputTokens + b.OutputTokens,
        a.CachedInputTokens + b.CachedInputTokens, a.CacheCreationInputTokens + b.CacheCreationInputTokens,
        a.ReasoningOutputTokens + b.ReasoningOutputTokens, a.ConversationCount + b.ConversationCount);

    public object ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}
