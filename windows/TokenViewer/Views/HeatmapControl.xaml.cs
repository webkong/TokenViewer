using System.Globalization;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;
using UserControl = System.Windows.Controls.UserControl;
using Brush = System.Windows.Media.Brush;
using Color = System.Windows.Media.Color;

namespace TokenViewerWindows.Views;

public partial class HeatmapControl : UserControl
{
    public static readonly DependencyProperty ItemsSourceProperty = DependencyProperty.Register(
        nameof(ItemsSource), typeof(IReadOnlyList<HeatmapPoint>), typeof(HeatmapControl),
        new PropertyMetadata(null, OnDataChanged));

    public static readonly DependencyProperty WeeksProperty = DependencyProperty.Register(
        nameof(Weeks), typeof(int), typeof(HeatmapControl),
        new PropertyMetadata(53, OnDataChanged));

    public HeatmapControl() => InitializeComponent();

    public IReadOnlyList<HeatmapPoint> ItemsSource
    {
        get => (IReadOnlyList<HeatmapPoint>)GetValue(ItemsSourceProperty);
        set => SetValue(ItemsSourceProperty, value);
    }

    public int Weeks
    {
        get => (int)GetValue(WeeksProperty);
        set => SetValue(WeeksProperty, value);
    }

    private static void OnDataChanged(DependencyObject d, DependencyPropertyChangedEventArgs e) =>
        ((HeatmapControl)d).Rebuild();

    private void Rebuild()
    {
        var points = ItemsSource ?? Array.Empty<HeatmapPoint>();
        var columns = BuildColumns(Weeks, DateTime.Today, points);

        ActiveDaysLabel.Text = L10n.Instance.UsageActiveDays(points.Count(p => p.Count > 0));

        CellGrid.Children.Clear();
        CellGrid.Rows = 7;
        CellGrid.Columns = Weeks;
        // UniformGrid fills row-major (row = weekday, column = week).
        for (var day = 0; day < 7; day++)
        {
            for (var week = 0; week < Weeks; week++)
            {
                var cell = columns[week][day];
                var border = new Border
                {
                    Background = LevelBrush(cell.Level),
                    CornerRadius = new CornerRadius(2),
                    Margin = new Thickness(1.5),
                };
                border.ToolTip = HelpText(cell);
                CellGrid.Children.Add(border);
            }
        }
    }

    /// <summary>A single heatmap cell (date, activity level, token count).</summary>
    public readonly record struct HeatCell(DateTime Date, byte Level, ulong Count);

    /// <summary>Builds a fixed <paramref name="weeks"/>-week grid (Sunday-start)
    /// ending in the week containing <paramref name="today"/>. Every day in range
    /// gets a cell (level 0 when no activity), so the grid is always full.</summary>
    public static IReadOnlyList<IReadOnlyList<HeatCell>> BuildColumns(
        int weeks, DateTime today, IReadOnlyList<HeatmapPoint> points)
    {
        if (weeks < 1) weeks = 1;
        var byDate = points.ToDictionary(p => p.Date, p => p);
        var thisSunday = today.Date.AddDays(-(int)today.Date.DayOfWeek); // DayOfWeek: 0 = Sunday
        var start = thisSunday.AddDays(-(weeks - 1) * 7);

        var columns = new List<IReadOnlyList<HeatCell>>();
        for (var w = 0; w < weeks; w++)
        {
            var col = new List<HeatCell>();
            for (var r = 0; r < 7; r++)
            {
                var date = start.AddDays(w * 7 + r);
                var key = date.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture);
                col.Add(byDate.TryGetValue(key, out var p)
                    ? new HeatCell(date, p.Level, p.Count)
                    : new HeatCell(date, 0, 0));
            }
            columns.Add(col);
        }
        return columns;
    }

    /// <summary>Level (0-4) to cell brush, matching the macOS emerald ramp.</summary>
    public static Brush LevelBrush(byte level) => level switch
    {
        1 => Level1,
        2 => Level2,
        3 => Level3,
        4 => Level4,
        _ => Level0,
    };

    private static readonly Brush Level0 = new SolidColorBrush(Color.FromRgb(0x37, 0x41, 0x51));
    private static readonly Brush Level1 = new SolidColorBrush(Color.FromArgb(0x59, 0x05, 0x96, 0x69));
    private static readonly Brush Level2 = new SolidColorBrush(Color.FromArgb(0x8C, 0x05, 0x96, 0x69));
    private static readonly Brush Level3 = new SolidColorBrush(Color.FromArgb(0xC7, 0x05, 0x96, 0x69));
    private static readonly Brush Level4 = new SolidColorBrush(Color.FromRgb(0x05, 0x96, 0x69));

    private static string HelpText(HeatCell cell)
    {
        var ds = cell.Date.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture);
        return cell.Count > 0 ? $"{ds}: {UsageFormats.Tokens(cell.Count)}" : $"{ds}: 0";
    }
}
