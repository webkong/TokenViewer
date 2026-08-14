using System.Threading;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Threading;
using TokenViewerWindows.Models;
using TokenViewerWindows.Services;
using TokenViewerWindows.Views;
using Xunit;

namespace TokenViewerWindows.Tests;

public class ChartControlTests
{
    [Fact]
    public void BuildColumns_produces_53_weeks_x_7_days()
    {
        var today = new DateTime(2026, 6, 15);
        var columns = HeatmapControl.BuildColumns(53, today, Array.Empty<HeatmapPoint>());
        Assert.Equal(53, columns.Count);
        Assert.All(columns, c => Assert.Equal(7, c.Count));
    }

    [Fact]
    public void BuildColumns_ends_in_week_containing_today()
    {
        // 2026-06-15 is a Monday; the Sunday-start week is 2026-06-14..2026-06-20.
        var today = new DateTime(2026, 6, 15);
        var columns = HeatmapControl.BuildColumns(1, today, Array.Empty<HeatmapPoint>());
        var week = Assert.Single(columns);
        Assert.Equal(new DateTime(2026, 6, 14), week[0].Date); // Sunday
        Assert.Equal(new DateTime(2026, 6, 15), week[1].Date); // Monday
        Assert.Equal(new DateTime(2026, 6, 20), week[6].Date); // Saturday
    }

    [Fact]
    public void BuildColumns_maps_points_to_correct_cells()
    {
        var today = new DateTime(2026, 6, 15); // Monday
        var points = new[] { new HeatmapPoint("2026-06-15", 42, 3) };
        var columns = HeatmapControl.BuildColumns(1, today, points);
        var cell = columns[0][1]; // Monday
        Assert.Equal((byte)3, cell.Level);
        Assert.Equal(42UL, cell.Count);
    }

    [Fact]
    public void BuildColumns_defaults_missing_days_to_level_zero()
    {
        var today = new DateTime(2026, 6, 15);
        var columns = HeatmapControl.BuildColumns(1, today, Array.Empty<HeatmapPoint>());
        var cell = columns[0][1]; // Monday has no data
        Assert.Equal((byte)0, cell.Level);
        Assert.Equal(0UL, cell.Count);
    }

    [Theory]
    [InlineData("2026-06-15", false, "06/15")]
    [InlineData("2026-06-15T14", true, "14:00")]
    [InlineData("2026-06-15T09", true, "09:00")]
    public void FormatTick_formats_daily_and_hourly(string raw, bool hourly, string expected)
    {
        Assert.Equal(expected, TrendChartControl.FormatTick(raw, hourly));
    }

    [Fact]
    public void CatmullRomGeometry_handles_empty_single_and_multi_point()
    {
        Assert.NotNull(TrendChartControl.CatmullRomGeometry(Array.Empty<Point>()));
        Assert.NotNull(TrendChartControl.CatmullRomGeometry(new[] { new Point(0, 0) }));

        var g = TrendChartControl.CatmullRomGeometry(new[]
        {
            new Point(0, 0), new Point(10, 10), new Point(20, 0),
        });
        Assert.NotNull(g);
        Assert.False(g.IsEmpty());
    }

    [Fact]
    public void DailyRowsConverter_aggregates_hourly_points_by_day()
    {
        var data = new[]
        {
            new DailyPoint("2026-06-15T09", 10, 0.1, 5, 5, 1, 0, 0, 1),
            new DailyPoint("2026-06-15T10", 20, 0.2, 10, 10, 2, 0, 0, 1),
            new DailyPoint("2026-06-14", 5, 0.05, 3, 2, 1, 0, 0, 1),
        };

        var byDay = DailyRowsConverter.AggregateByDay(data);

        Assert.Equal(2, byDay.Count);
        var jun15 = byDay["2026-06-15"];
        Assert.Equal(30UL, jun15.TotalTokens);   // 10 + 20
        Assert.Equal(15UL, jun15.InputTokens);   // 5 + 10
        Assert.Equal(2U, jun15.ConversationCount);
        Assert.Equal(5UL, byDay["2026-06-14"].TotalTokens);
    }

    [Fact]
    public void Tooltip_cache_label_is_localized()
    {
        var l10n = L10n.Instance;
        try
        {
            l10n.Language = "en";
            Assert.Equal("Cache: 1.5K", TrendChartControl.TooltipCacheText(1500));
            l10n.Language = "zh";
            Assert.Equal("缓存: 1.5K", TrendChartControl.TooltipCacheText(1500));
        }
        finally
        {
            l10n.Language = "system";
        }
    }

    [Fact]
    public void Crosshair_and_tooltip_survive_redraw()
    {
        // WPF requires an STA thread for Application/control construction, and
        // xUnit runs on MTA. Run the whole assertion on a dedicated STA thread and
        // propagate any exception back to the test thread (which also shuts the
        // Dispatcher down so the test process does not hang).
        RunSta(() =>
        {
            EnsureWpfApplication();
            var control = new TrendChartControl
            {
                ItemsSource = new[] { new DailyPoint("2026-06-15", 10, 0.1, 5, 5, 1, 0, 0, 1) },
            };
            Assert.True(control.IsOverlayPresent());
        });
    }

    /// <summary>Runs <paramref name="action"/> on a fresh STA thread, shuts the
    /// Dispatcher down, and rethrows any captured exception on the caller.</summary>
    private static void RunSta(Action action)
    {
        Exception? captured = null;
        var thread = new Thread(() =>
        {
            try
            {
                action();
            }
            catch (Exception ex)
            {
                captured = ex;
            }
            finally
            {
                var dispatcher = Dispatcher.CurrentDispatcher;
                if (!dispatcher.HasShutdownStarted)
                {
                    dispatcher.InvokeShutdown();
                }
            }
        });
        thread.SetApartmentState(ApartmentState.STA);
        thread.Start();
        thread.Join();
        if (captured is not null)
        {
            System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(captured).Throw();
        }
    }

    private static readonly object AppGate = new();
    private static void EnsureWpfApplication()
    {
        if (Application.Current is not null) return;
        lock (AppGate)
        {
            if (Application.Current is not null) return;
            var app = new Application { ShutdownMode = ShutdownMode.OnExplicitShutdown };
            var r = app.Resources;
            r["Card"] = new Style(typeof(Border));
            r["CardTitle"] = new Style(typeof(TextBlock));
            r["MutedLabel"] = new Style(typeof(TextBlock));
            r["PanelBorder"] = Brushes.Gray;
            r["MutedText"] = Brushes.Gray;
            r["CacheReadBrush"] = Brushes.Purple;
        }
    }
}
