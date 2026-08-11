using System.Windows;

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
    IReadOnlyList<LimitWindow> Windows);

