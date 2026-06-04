namespace TokenViewerWindows.Models;

public sealed record UpdateRelease(
    string Version,
    string ReleaseUrl,
    string? AssetUrl,
    string Notes);
