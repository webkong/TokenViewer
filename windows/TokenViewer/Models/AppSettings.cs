namespace TokenViewerWindows.Models;

public sealed record AppSettings
{
    public string Theme { get; init; } = "system";
    public string Language { get; init; } = "system";
    public string Currency { get; init; } = "USD";
    public int SyncFrequencyMinutes { get; init; } = 30;
    public bool LaunchAtStartup { get; init; } = false;
    public bool ShowMenuBarIcon { get; init; } = true;
    public bool PanelShowSummary { get; init; } = true;
    public bool PanelShowLimits { get; init; } = true;
    public bool PanelShowHeatmap { get; init; } = true;
    public bool PanelShowTrend { get; init; } = true;
    public bool PanelShowModels { get; init; } = true;
    public string LimitsVisibleSources { get; init; } =
        "claude,codex,cursor,kiro,copilot,kimi,antigravity,zed,trae,windsurf,qoder,codebuddy,workbuddy,gemini,zcode";
}
