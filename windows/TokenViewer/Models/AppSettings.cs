namespace TokenViewerWindows.Models;

public sealed record AppSettings
{
    public string Theme { get; init; } = "system";
    public string Language { get; init; } = "system";
    public int SyncFrequencyMinutes { get; init; } = 30;
    public bool LaunchAtStartup { get; init; } = false;
}

