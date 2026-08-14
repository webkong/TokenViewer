using System.Globalization;

namespace TokenViewerWindows.Services;

public readonly record struct UsageQueryRange(string From, string To);

/// <summary>
/// Shared time semantics for usage queries, mirroring macOS <c>AppTime</c>.
/// Usage is stored as UTC instants; user-facing ranges are local calendar days,
/// converted to UTC only at the FFI boundary (from inclusive, to exclusive).
///
/// Instant-taking methods expect a UTC <see cref="DateTime"/> (Kind.Utc);
/// local-day methods take date-only values interpreted in <paramref name="tz"/>.
/// The timezone is injectable so tests never depend on the machine's timezone.
/// </summary>
public static class AppTime
{
    public const string AllUsageStart = "2020-01-01T00:00:00Z";

    public static TimeZoneInfo LocalTimeZone => TimeZoneInfo.Local;

    /// <summary>True when two date-only values fall on the same calendar day.</summary>
    public static bool IsSameLocalDay(DateTime lhs, DateTime rhs) => lhs.Date == rhs.Date;

    /// <summary>The last <paramref name="count"/> local calendar days, inclusive of today.</summary>
    public static UsageQueryRange TrailingLocalDays(int count, DateTime utcNow, TimeZoneInfo? tz = null)
    {
        if (count < 1) throw new ArgumentOutOfRangeException(nameof(count), "count must be >= 1");
        tz ??= LocalTimeZone;
        var localToday = TimeZoneInfo.ConvertTimeFromUtc(AsUtc(utcNow), tz).Date;
        var startLocal = localToday.AddDays(-(count - 1));
        var endLocal = localToday.AddDays(1);
        return new UsageQueryRange(ToUtcString(startLocal, tz), ToUtcString(endLocal, tz));
    }

    /// <summary>The single local calendar day before today.</summary>
    public static UsageQueryRange YesterdayLocalDay(DateTime utcNow, TimeZoneInfo? tz = null)
    {
        tz ??= LocalTimeZone;
        var localToday = TimeZoneInfo.ConvertTimeFromUtc(AsUtc(utcNow), tz).Date;
        return new UsageQueryRange(ToUtcString(localToday.AddDays(-1), tz), ToUtcString(localToday, tz));
    }

    /// <summary>The inclusive range covering the local calendar days of
    /// <paramref name="fromLocalDay"/> and <paramref name="throughLocalDay"/> (order-agnostic).</summary>
    public static UsageQueryRange InclusiveLocalDays(DateTime fromLocalDay, DateTime throughLocalDay, TimeZoneInfo? tz = null)
    {
        tz ??= LocalTimeZone;
        var from = fromLocalDay.Date;
        var through = throughLocalDay.Date;
        var start = from <= through ? from : through;
        var end = (from >= through ? from : through).AddDays(1);
        return new UsageQueryRange(ToUtcString(start, tz), ToUtcString(end, tz));
    }

    /// <summary>All usage from the epoch constant through tomorrow's local midnight.</summary>
    public static UsageQueryRange AllUsage(DateTime utcNow, TimeZoneInfo? tz = null) =>
        new(AllUsageStart, TrailingLocalDays(1, utcNow, tz).To);

    private static DateTime AsUtc(DateTime dt) => dt.Kind switch
    {
        DateTimeKind.Utc => dt,
        DateTimeKind.Local => dt.ToUniversalTime(),
        _ => DateTime.SpecifyKind(dt, DateTimeKind.Utc),
    };

    private static string ToUtcString(DateTime localDate, TimeZoneInfo tz)
    {
        var utc = TimeZoneInfo.ConvertTimeToUtc(DateTime.SpecifyKind(localDate, DateTimeKind.Unspecified), tz);
        return utc.ToString("yyyy-MM-dd'T'HH:mm:ss'Z'", CultureInfo.InvariantCulture);
    }
}
