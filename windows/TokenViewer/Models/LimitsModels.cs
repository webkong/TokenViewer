using System.Linq;

namespace TokenViewerWindows.Models;

public sealed record LimitWindow(
    string Label,
    double UsedPercent,
    DateTime? ResetAt);

public sealed record AgentLimit(
    string Name,
    string? PlanLabel,
    bool Configured,
    string? Error,
    IReadOnlyList<LimitWindow> Windows,
    DateTime? SubscriptionExpiresAt = null,
    DateTime? SubscriptionResetAt = null,
    DateTime? QuotaResetAt = null)
{
    public bool HasLimitDisplay =>
        Windows.Count > 0 ||
        SubscriptionExpiresAt is not null ||
        SubscriptionResetAt is not null ||
        QuotaResetAt is not null;

    /// <summary>The earliest reset in the future, else the latest reset in the past.</summary>
    public DateTime? NextResetAt
    {
        get
        {
            var dates = Windows.Select(w => w.ResetAt).Where(d => d is not null).Select(d => d!.Value).ToList();
            if (dates.Count == 0) return null;
            var now = DateTime.Now;
            return dates.Where(d => d >= now).OrderBy(d => d).Cast<DateTime?>().FirstOrDefault()
                ?? dates.Max();
        }
    }

    /// <summary>Next reset across all sources (windows, subscription expiry/reset,
    /// quota reset) for the countdown badge: earliest future value, else the most
    /// recent past value.</summary>
    public DateTime? DisplayResetAt
    {
        get
        {
            var dates = Windows.Select(w => w.ResetAt)
                .Concat(new[] { SubscriptionExpiresAt, SubscriptionResetAt, QuotaResetAt })
                .Where(d => d is not null)
                .Select(d => d!.Value)
                .ToList();
            if (dates.Count == 0) return null;
            var now = DateTime.Now;
            return dates.Where(d => d >= now).OrderBy(d => d).Cast<DateTime?>().FirstOrDefault()
                ?? dates.Max();
        }
    }
}
