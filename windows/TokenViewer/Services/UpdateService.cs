using System.Diagnostics;
using System.Globalization;
using System.Linq;
using System.Net.Http.Headers;
using System.Text.Json;
using TokenViewerWindows.Models;

namespace TokenViewerWindows.Services;

public static class UpdateService
{
    private const string Repo = "webkong/TokenViewer";

    public static string CurrentVersion =>
        typeof(UpdateService).Assembly.GetName().Version?.ToString(3) ?? "0.1.0";

    public static async Task<UpdateRelease?> CheckLatestAsync()
    {
        try
        {
            using var req = new HttpRequestMessage(HttpMethod.Get, $"https://api.github.com/repos/{Repo}/releases/latest");
            req.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/vnd.github+json"));
            req.Headers.UserAgent.ParseAdd($"TokenViewer/{CurrentVersion}");

            using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(15) };
            using var resp = await http.SendAsync(req);
            if (!resp.IsSuccessStatusCode)
            {
                return null;
            }

            using var doc = JsonDocument.Parse(await resp.Content.ReadAsStringAsync());
            var root = doc.RootElement;
            var tagName = root.GetProperty("tag_name").GetString()?.Trim();
            if (string.IsNullOrWhiteSpace(tagName))
            {
                return null;
            }

            var version = tagName.TrimStart('v', 'V').Trim();
            if (string.IsNullOrWhiteSpace(version))
            {
                return null;
            }

            var releaseUrl = root.TryGetProperty("html_url", out var htmlUrl) && htmlUrl.ValueKind == JsonValueKind.String
                ? htmlUrl.GetString() ?? $"https://github.com/{Repo}/releases/latest"
                : $"https://github.com/{Repo}/releases/latest";

            string? assetUrl = null;
            if (root.TryGetProperty("assets", out var assets) && assets.ValueKind == JsonValueKind.Array)
            {
                foreach (var asset in assets.EnumerateArray())
                {
                    if (!asset.TryGetProperty("browser_download_url", out var downloadUrl) || downloadUrl.ValueKind != JsonValueKind.String)
                    {
                        continue;
                    }

                    var candidate = downloadUrl.GetString();
                    if (string.IsNullOrWhiteSpace(candidate))
                    {
                        continue;
                    }

                    var ext = Path.GetExtension(candidate).ToLowerInvariant();
                    if (ext is ".msi" or ".exe" or ".msix" or ".zip")
                    {
                        assetUrl = candidate;
                        break;
                    }
                }
            }

            var notes = root.TryGetProperty("body", out var body) && body.ValueKind == JsonValueKind.String
                ? body.GetString()?.Trim() ?? ""
                : "";

            return new UpdateRelease(version, releaseUrl, assetUrl, notes);
        }
        catch
        {
            return null;
        }
    }

    public static bool IsNewer(string candidate, string baseline)
    {
        var a = ParseParts(candidate);
        var b = ParseParts(baseline);
        for (var i = 0; i < Math.Max(a.Length, b.Length); i++)
        {
            var x = i < a.Length ? a[i] : 0;
            var y = i < b.Length ? b[i] : 0;
            if (x != y)
            {
                return x > y;
            }
        }

        return false;
    }

    public static bool OpenRelease(Uri releaseUri) => OpenTarget(releaseUri.AbsoluteUri);

    public static async Task<bool> DownloadAndOpenAsync(UpdateRelease release)
    {
        if (string.IsNullOrWhiteSpace(release.AssetUrl))
        {
            return OpenTarget(release.ReleaseUrl);
        }

        try
        {
            using var http = new HttpClient { Timeout = TimeSpan.FromMinutes(5) };
            var assetUri = new Uri(release.AssetUrl);
            var bytes = await http.GetByteArrayAsync(assetUri);

            var downloads = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
                "Downloads",
                "TokenViewer");
            Directory.CreateDirectory(downloads);

            var fileName = Path.GetFileName(assetUri.LocalPath);
            if (string.IsNullOrWhiteSpace(fileName))
            {
                fileName = $"TokenViewer-{release.Version}-Installer";
            }

            var dest = Path.Combine(downloads, fileName);
            await File.WriteAllBytesAsync(dest, bytes);
            return OpenTarget(dest);
        }
        catch
        {
            return OpenTarget(release.ReleaseUrl);
        }
    }

    private static int[] ParseParts(string version)
        => version
            .Split('.', StringSplitOptions.RemoveEmptyEntries)
            .Select(part =>
            {
                var digits = new string(part.TakeWhile(char.IsDigit).ToArray());
                return int.TryParse(digits, NumberStyles.Integer, CultureInfo.InvariantCulture, out var parsed) ? parsed : 0;
            })
            .ToArray();

    private static bool OpenTarget(string target)
    {
        try
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = target,
                UseShellExecute = true,
            });
            return true;
        }
        catch
        {
            return false;
        }
    }
}
