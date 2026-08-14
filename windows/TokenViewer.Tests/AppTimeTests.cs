using TokenViewerWindows.Services;
using Xunit;

namespace TokenViewerWindows.Tests;

public class AppTimeTests
{
    // A deterministic US-Eastern-like timezone (UTC-5, DST +1 from the 2nd Sunday
    // of March through the 1st Sunday of November). No dependence on OS tz data
    // or the machine's current timezone.
    private static TimeZoneInfo UsEastern()
    {
        var delta = TimeSpan.FromHours(1);
        var rule = TimeZoneInfo.AdjustmentRule.CreateAdjustmentRule(
            DateTime.MinValue, DateTime.MaxValue, delta,
            TimeZoneInfo.TransitionTime.CreateFloatingDateRule(new DateTime(1, 1, 1, 2, 0, 0), 3, 2, DayOfWeek.Sunday),
            TimeZoneInfo.TransitionTime.CreateFloatingDateRule(new DateTime(1, 1, 1, 2, 0, 0), 11, 1, DayOfWeek.Sunday));
        return TimeZoneInfo.CreateCustomTimeZone(
            "US-Eastern-Test", TimeSpan.FromHours(-5), "US-Eastern-Test", "US-Eastern-Test", "US-Eastern-Test", new[] { rule });
    }

    private static DateTime Utc(int year, int month, int day, int hour, int minute = 0) =>
        new(year, month, day, hour, minute, 0, DateTimeKind.Utc);

    [Fact]
    public void TrailingLocalDays_normal_day_is_24h()
    {
        var tz = UsEastern();
        // 2026-06-15 12:00 UTC == 08:00 EDT (UTC-4).
        var range = AppTime.TrailingLocalDays(1, Utc(2026, 6, 15, 12), tz);
        Assert.Equal("2026-06-15T04:00:00Z", range.From);
        Assert.Equal("2026-06-16T04:00:00Z", range.To);
    }

    [Fact]
    public void TrailingLocalDays_spring_forward_day_is_23h()
    {
        var tz = UsEastern();
        // 2026-03-08 is the US spring-forward day. Noon is EDT (UTC-4), but local
        // midnight is still EST (UTC-5), so the local day spans 23 hours.
        var range = AppTime.TrailingLocalDays(1, Utc(2026, 3, 8, 12), tz);
        Assert.Equal("2026-03-08T05:00:00Z", range.From);
        Assert.Equal("2026-03-09T04:00:00Z", range.To);
    }

    [Fact]
    public void TrailingLocalDays_covers_requested_day_count()
    {
        var tz = UsEastern();
        var range = AppTime.TrailingLocalDays(7, Utc(2026, 6, 15, 12), tz);
        Assert.Equal("2026-06-09T04:00:00Z", range.From);
        Assert.Equal("2026-06-16T04:00:00Z", range.To);
    }

    [Fact]
    public void Yesterday_is_prior_local_day()
    {
        var tz = UsEastern();
        var range = AppTime.YesterdayLocalDay(Utc(2026, 6, 15, 12), tz);
        Assert.Equal("2026-06-14T04:00:00Z", range.From);
        Assert.Equal("2026-06-15T04:00:00Z", range.To);
    }

    [Fact]
    public void InclusiveLocalDays_orders_bounds_and_is_inclusive()
    {
        var tz = UsEastern();
        var range = AppTime.InclusiveLocalDays(new DateTime(2026, 6, 10), new DateTime(2026, 6, 12), tz);
        Assert.Equal("2026-06-10T04:00:00Z", range.From);
        Assert.Equal("2026-06-13T04:00:00Z", range.To);
    }

    [Fact]
    public void InclusiveLocalDays_handles_reversed_bounds()
    {
        var tz = UsEastern();
        var range = AppTime.InclusiveLocalDays(new DateTime(2026, 6, 12), new DateTime(2026, 6, 10), tz);
        Assert.Equal("2026-06-10T04:00:00Z", range.From);
        Assert.Equal("2026-06-13T04:00:00Z", range.To);
    }

    [Fact]
    public void AllUsage_starts_at_epoch_and_ends_at_tomorrow_local_midnight()
    {
        var tz = UsEastern();
        var range = AppTime.AllUsage(Utc(2026, 6, 15, 12), tz);
        Assert.Equal(AppTime.AllUsageStart, range.From);
        Assert.Equal("2026-06-16T04:00:00Z", range.To);
    }

    [Fact]
    public void IsSameLocalDay_compares_calendar_days()
    {
        Assert.True(AppTime.IsSameLocalDay(new DateTime(2026, 6, 15), new DateTime(2026, 6, 15, 23, 59, 0)));
        Assert.False(AppTime.IsSameLocalDay(new DateTime(2026, 6, 15), new DateTime(2026, 6, 16)));
    }
}
