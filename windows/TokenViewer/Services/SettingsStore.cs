using System.Text.Json;
using TokenViewerWindows.Models;

namespace TokenViewerWindows.Services;

public sealed class SettingsStore
{
    private readonly string _path;
    private readonly JsonSerializerOptions _json = new() { WriteIndented = true };

    public SettingsStore(string? path = null)
    {
        _path = path ?? DefaultPath();
    }

    public AppSettings Load()
    {
        try
        {
            if (!File.Exists(_path)) return new AppSettings();
            var json = File.ReadAllText(_path);
            // Missing fields fall back to the record's defaults; present fields win.
            return JsonSerializer.Deserialize<AppSettings>(json, _json) ?? new AppSettings();
        }
        catch
        {
            return new AppSettings();
        }
    }

    public void Save(AppSettings settings)
    {
        var json = JsonSerializer.Serialize(settings, _json);
        var dir = Path.GetDirectoryName(_path);
        if (!string.IsNullOrEmpty(dir)) Directory.CreateDirectory(dir);
        File.WriteAllText(_path, json);
    }

    private static string DefaultPath()
    {
        var dir = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "TokenViewer");
        Directory.CreateDirectory(dir);
        return Path.Combine(dir, "settings.json");
    }
}
