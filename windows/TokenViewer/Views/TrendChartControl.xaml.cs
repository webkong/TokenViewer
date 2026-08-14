using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using System.Windows.Media;
using System.Windows.Shapes;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;
using UserControl = System.Windows.Controls.UserControl;
using Brush = System.Windows.Media.Brush;
using Color = System.Windows.Media.Color;
using Point = System.Windows.Point;
using MouseEventArgs = System.Windows.Input.MouseEventArgs;
using Path = System.Windows.Shapes.Path;

namespace TokenViewerWindows.Views;

public partial class TrendChartControl : UserControl
{
    public static readonly DependencyProperty ItemsSourceProperty = DependencyProperty.Register(
        nameof(ItemsSource), typeof(IReadOnlyList<DailyPoint>), typeof(TrendChartControl),
        new PropertyMetadata(null, OnDataChanged));

    public static readonly DependencyProperty IsHourlyProperty = DependencyProperty.Register(
        nameof(IsHourly), typeof(bool), typeof(TrendChartControl),
        new PropertyMetadata(false, OnDataChanged));

    private int _hoverIndex = -1;

    public TrendChartControl() => InitializeComponent();

    public IReadOnlyList<DailyPoint> ItemsSource
    {
        get => (IReadOnlyList<DailyPoint>)GetValue(ItemsSourceProperty);
        set => SetValue(ItemsSourceProperty, value);
    }

    public bool IsHourly
    {
        get => (bool)GetValue(IsHourlyProperty);
        set => SetValue(IsHourlyProperty, value);
    }

    private static void OnDataChanged(DependencyObject d, DependencyPropertyChangedEventArgs e) =>
        ((TrendChartControl)d).Redraw();

    private void OnPlotSizeChanged(object sender, SizeChangedEventArgs e) => Redraw();

    private void Redraw()
    {
        GranularityLabel.Text = IsHourly ? L10n.Instance["byHour"] : L10n.Instance["byDay"];

        var data = ItemsSource ?? Array.Empty<DailyPoint>();
        var width = PlotCanvas.ActualWidth > 0 ? PlotCanvas.ActualWidth : 1;
        var height = PlotCanvas.ActualHeight > 0 ? PlotCanvas.ActualHeight : 1;

        // Clear only the generated series layer, not the Crosshair/Tooltip overlay.
        PlotSeries.Children.Clear();
        if (data.Count == 0) return;

        // Token series (left axis): input / output / cache-write / cache-read.
        var tokenSeries = new (Brush Brush, double[] Values)[]
        {
            (InputBrush, data.Select(p => (double)p.InputTokens).ToArray()),
            (OutputBrush, data.Select(p => (double)p.OutputTokens).ToArray()),
            (CacheBrush, data.Select(p => (double)p.CacheCreationInputTokens).ToArray()),
            (CacheReadBrush, data.Select(p => (double)p.CachedInputTokens).ToArray()),
        }.Where(s => s.Values.Any(v => v > 0)).ToArray();

        var costValues = data.Select(p => p.TotalCostUsd).ToArray();
        var tokenMax = Math.Max(tokenSeries.SelectMany(s => s.Values).DefaultIfEmpty(1).Max(), 1);
        var costMax = Math.Max(costValues.DefaultIfEmpty(0.0001).Max(), 0.0001);

        // Horizontal grid lines.
        for (var i = 0; i <= 4; i++)
        {
            var y = height * i / 4;
            PlotSeries.Children.Add(new Line
            {
                X1 = 0, Y1 = y, X2 = width, Y2 = y,
                Stroke = GridBrush, StrokeThickness = 0.5,
            });
        }

        var n = Math.Max(data.Count - 1, 1);
        Point Map(int i, double v, double max) => new(
            width * i / n,
            height * (1 - Math.Min(Math.Max(v / max, 0), 1)));

        foreach (var (brush, values) in tokenSeries)
        {
            var pts = values.Select((v, i) => Map(i, v, tokenMax)).ToArray();
            PlotSeries.Children.Add(new Path
            {
                Stroke = brush,
                StrokeThickness = 1.8,
                Data = CatmullRomGeometry(pts),
            });
        }

        var costPts = costValues.Select((v, i) => Map(i, v, costMax)).ToArray();
        PlotSeries.Children.Add(new Path
        {
            Stroke = CostBrush,
            StrokeThickness = 1.6,
            StrokeDashArray = new DoubleCollection { 5, 4 },
            Data = CatmullRomGeometry(costPts),
        });

        DrawAxes(tokenMax, costMax, data);
    }

    private void DrawAxes(double tokenMax, double costMax, IReadOnlyList<DailyPoint> data)
    {
        LeftAxis.Children.Clear();
        RightAxis.Children.Clear();
        for (var i = 4; i >= 0; i--)
        {
            LeftAxis.Children.Add(AxisLabel(UsageFormats.Tokens((ulong)Math.Max(tokenMax * i / 4, 0))));
            RightAxis.Children.Add(AxisLabel(UsageFormats.Cost(Math.Max(costMax * i / 4, 0))));
        }

        XAxis.Children.Clear();
        foreach (var label in TickLabels(data))
        {
            XAxis.Children.Add(new TextBlock
            {
                Text = label,
                FontSize = 9,
                Foreground = MutedBrush,
            });
        }
    }

    private static TextBlock AxisLabel(string text) => new()
    {
        Text = text,
        FontSize = 9,
        Foreground = MutedBrush,
        TextAlignment = TextAlignment.Right,
        Margin = new Thickness(0, 0, 4, 0),
    };

    private IReadOnlyList<string> TickLabels(IReadOnlyList<DailyPoint> data)
    {
        if (data.Count == 0) return Array.Empty<string>();
        var count = Math.Min(data.Count, 6);
        var step = Math.Max(data.Count / count, 1);
        var labels = new List<string>();
        for (var i = 0; i < data.Count; i += step)
        {
            labels.Add(FormatTick(data[i].Date, IsHourly));
        }
        return labels;
    }

    /// <summary>"YYYY-MM-DDTHH" → "HH:00" (hourly), "YYYY-MM-DD" → "MM/DD" (daily).</summary>
    public static string FormatTick(string raw, bool hourly)
    {
        if (hourly)
        {
            var t = raw.Split('T').LastOrDefault() ?? raw;
            return $"{t}:00";
        }
        var parts = raw.Split('-');
        return parts.Length == 3 ? $"{parts[1]}/{parts[2]}" : raw;
    }

    /// <summary>True when the crosshair and tooltip overlays are still in the
    /// plot's visual tree after a redraw (they must never be cleared).</summary>
    public bool IsOverlayPresent() =>
        PlotCanvas.Children.Contains(Crosshair) && PlotCanvas.Children.Contains(TooltipPanel);

    /// <summary>Localized tooltip label for the combined cache (read + write).</summary>
    public static string TooltipCacheText(ulong cache) =>
        $"{L10n.Instance["usageColCache"]}: {UsageFormats.Tokens(cache)}";

    /// <summary>Builds a smooth Catmull-Rom spline through the points as a
    /// <see cref="StreamGeometry"/>. Single/empty inputs degrade to a dot/no-op.</summary>
    public static StreamGeometry CatmullRomGeometry(IReadOnlyList<Point> pts)
    {
        var g = new StreamGeometry();
        if (pts.Count == 0) return g;
        using (var ctx = g.Open())
        {
            if (pts.Count == 1)
            {
                ctx.BeginFigure(pts[0], false, false);
                ctx.LineTo(pts[0], true, false);
                return g;
            }
            ctx.BeginFigure(pts[0], false, false);
            for (var i = 0; i < pts.Count - 1; i++)
            {
                var p0 = i == 0 ? pts[i] : pts[i - 1];
                var p1 = pts[i];
                var p2 = pts[i + 1];
                var p3 = i + 2 < pts.Count ? pts[i + 2] : p2;
                var c1 = new Point(p1.X + (p2.X - p0.X) / 6, p1.Y + (p2.Y - p0.Y) / 6);
                var c2 = new Point(p2.X - (p3.X - p1.X) / 6, p2.Y - (p3.Y - p1.Y) / 6);
                ctx.BezierTo(c1, c2, p2, true, false);
            }
        }
        g.Freeze();
        return g;
    }

    private void OnPlotMouseMove(object sender, MouseEventArgs e)
    {
        var data = ItemsSource ?? Array.Empty<DailyPoint>();
        if (data.Count == 0 || PlotCanvas.ActualWidth <= 0) return;
        var pos = e.GetPosition(PlotCanvas);
        var n = Math.Max(data.Count - 1, 1);
        _hoverIndex = Math.Clamp((int)Math.Round(pos.X / PlotCanvas.ActualWidth * n), 0, data.Count - 1);
        UpdateCrosshair(data[_hoverIndex]);
    }

    private void OnPlotMouseLeave(object sender, MouseEventArgs e)
    {
        _hoverIndex = -1;
        Crosshair.Visibility = Visibility.Collapsed;
        TooltipPanel.Visibility = Visibility.Collapsed;
    }

    private void UpdateCrosshair(DailyPoint p)
    {
        if (_hoverIndex < 0) return;
        var data = ItemsSource ?? Array.Empty<DailyPoint>();
        var n = Math.Max(data.Count - 1, 1);
        var x = PlotCanvas.ActualWidth * _hoverIndex / n;
        Crosshair.X1 = x;
        Crosshair.X2 = x;
        Crosshair.Y1 = 0;
        Crosshair.Y2 = PlotCanvas.ActualHeight;
        Crosshair.Visibility = Visibility.Visible;

        var cache = p.CachedInputTokens + p.CacheCreationInputTokens;
        var denom = p.InputTokens + p.CachedInputTokens;
        var hit = denom > 0 ? p.CachedInputTokens * 100.0 / denom : 0;
        TooltipTitle.Text = FormatTick(p.Date, IsHourly);
        TooltipInput.Text = $"{L10n.Instance["input"]}: {UsageFormats.Tokens(p.InputTokens)}";
        TooltipOutput.Text = $"{L10n.Instance["output"]}: {UsageFormats.Tokens(p.OutputTokens)}";
        TooltipCache.Text = TooltipCacheText(cache);
        TooltipCost.Text = $"{L10n.Instance["cost"]}: {UsageFormats.Cost(p.TotalCostUsd)}";
        TooltipHit.Text = $"{L10n.Instance["cacheHit"]}: {hit:0.0}%";
        TooltipPanel.Visibility = Visibility.Visible;
        Canvas.SetLeft(TooltipPanel, Math.Min(Math.Max(x - 50, 0), PlotCanvas.ActualWidth - 110));
        Canvas.SetTop(TooltipPanel, 4);
    }

    private static readonly Brush InputBrush = new SolidColorBrush(Color.FromRgb(0x3B, 0x82, 0xF6));
    private static readonly Brush OutputBrush = new SolidColorBrush(Color.FromRgb(0x22, 0xC5, 0x5E));
    private static readonly Brush CacheBrush = new SolidColorBrush(Color.FromRgb(0xF5, 0x9E, 0x0B));
    private static readonly Brush CacheReadBrush = new SolidColorBrush(Color.FromRgb(0x8B, 0x5C, 0xF6));
    private static readonly Brush CostBrush = new SolidColorBrush(Color.FromRgb(0xEF, 0x44, 0x44));
    private static readonly Brush GridBrush = new SolidColorBrush(Color.FromArgb(0x1E, 0x9C, 0xA3, 0xAF));
    private static readonly Brush MutedBrush = new SolidColorBrush(Color.FromRgb(0x94, 0xA3, 0xB8));
}
