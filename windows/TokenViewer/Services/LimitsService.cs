using System.Diagnostics;
using System.Linq;
using System.Text.Json;
using System.Text.RegularExpressions;
using TokenViewerWindows.Models;

namespace TokenViewerWindows.Services;

public static class LimitsService
{
    public static async Task<IReadOnlyList<ProviderLimit>> FetchAllAsync()
    {
        var claude = FetchClaudeAsync();
        var codex = FetchCodexAsync();
        var cursor = FetchCursorAsync();
        var gemini = FetchGeminiAsync();
        var kiro = FetchKiroAsync();
        var kimi = FetchKimiAsync();
        var antigravity = FetchAntigravityAsync();
        return await Task.WhenAll(claude, codex, cursor, gemini, kiro, kimi, antigravity);
    }

    private static async Task<ProviderLimit> FetchClaudeAsync()
    {
        const string name = "claude";
        var token = ReadClaudeToken();
        if (string.IsNullOrWhiteSpace(token))
        {
            return new ProviderLimit(name, null, false, null, []);
        }

        var req = new HttpRequestMessage(HttpMethod.Get, "https://api.anthropic.com/api/oauth/usage");
        req.Headers.TryAddWithoutValidation("Authorization", $"Bearer {token}");
        req.Headers.TryAddWithoutValidation("anthropic-beta", "oauth-2025-04-20");
        req.Headers.TryAddWithoutValidation("Accept", "application/json");
        var json = await GetJsonAsync(req);
        if (json is null)
        {
            return new ProviderLimit(name, "Claude", true, "Request failed", []);
        }

        var windows = new List<LimitWindow>();
        foreach (var (key, label) in new[] { ("five_hour", "5 Hour"), ("seven_day", "7 Day"), ("seven_day_opus", "7 Day (Opus)") })
        {
            if (!json.TryGetValue(key, out var raw) || raw is not JsonElement w || w.ValueKind != JsonValueKind.Object) continue;
            var util = ReadDouble(w, "utilization");
            windows.Add(new LimitWindow(label, util, ReadDate(w, "resets_at")));
        }
        return new ProviderLimit(name, "Claude", true, null, windows);
    }

    private static async Task<ProviderLimit> FetchCodexAsync()
    {
        const string name = "codex";
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var authPath = Environment.GetEnvironmentVariable("CODEX_HOME") is { Length: > 0 } codexHome
            ? Path.Combine(codexHome, "auth.json")
            : Path.Combine(home, ".codex", "auth.json");
        if (!File.Exists(authPath))
        {
            return new ProviderLimit(name, null, false, null, []);
        }

        var auth = ReadJsonFile(authPath);
        if (auth is null || !auth.TryGetValue("tokens", out var tokensObj) || tokensObj.ValueKind != JsonValueKind.Object ||
            !TryGetString(tokensObj, "access_token", out var accessToken) || string.IsNullOrWhiteSpace(accessToken))
        {
            return new ProviderLimit(name, null, false, null, []);
        }

        var plan = PlanLabel(JwtClaim(accessToken, "chatgpt_plan_type"), "Codex");
        var accountId = TryGetString(tokensObj, "account_id", out var account) ? account : JwtClaim(accessToken, "chatgpt_account_id");
        var req = new HttpRequestMessage(HttpMethod.Get, "https://chatgpt.com/backend-api/wham/usage");
        req.Headers.TryAddWithoutValidation("Authorization", $"Bearer {accessToken}");
        req.Headers.TryAddWithoutValidation("Accept", "application/json");
        if (!string.IsNullOrWhiteSpace(accountId))
        {
            req.Headers.TryAddWithoutValidation("ChatGPT-Account-Id", accountId);
        }

        var json = await GetJsonAsync(req);
        if (json is null || !TryGetJsonObject(json, "rate_limit", out var rl))
        {
            return new ProviderLimit(name, plan, true, "Request failed", []);
        }

        var windows = new List<LimitWindow>();
        foreach (var key in new[] { "primary_window", "secondary_window" })
        {
            if (!TryGetJsonObject(rl, key, out var w)) continue;
            var secs = ReadInt(w, "limit_window_seconds");
            var label = secs >= 604800 ? "Weekly" : secs >= 18000 ? "5 Hour" : "Window";
            windows.Add(new LimitWindow(label, ReadDouble(w, "used_percent"), ReadDate(w, "reset_at")));
        }
        return new ProviderLimit(name, plan, true, null, windows);
    }

    private static async Task<ProviderLimit> FetchCursorAsync()
    {
        const string name = "cursor";
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var stateDb = Path.Combine(home, "AppData", "Roaming", "Cursor", "User", "globalStorage", "state.vscdb");
        var cliCfg = Path.Combine(home, ".cursor", "cli-config.json");
        if (!File.Exists(stateDb))
        {
            return new ProviderLimit(name, null, false, null, []);
        }

        var jwt = ReadSqliteValue(stateDb, "SELECT value FROM ItemTable WHERE key='cursorAuth/accessToken' LIMIT 1");
        if (string.IsNullOrWhiteSpace(jwt) || jwt.Length < 10)
        {
            return new ProviderLimit(name, null, false, null, []);
        }

        var authId = ReadCursorAuthId(cliCfg);
        var userId = !string.IsNullOrWhiteSpace(authId) ? authId : JwtClaim(jwt, "sub");
        if (string.IsNullOrWhiteSpace(userId))
        {
            return new ProviderLimit(name, null, false, "No userId", []);
        }

        var req = new HttpRequestMessage(HttpMethod.Get, "https://cursor.com/api/usage-summary");
        req.Headers.TryAddWithoutValidation("Cookie", $"WorkosCursorSessionToken={userId}%3A%3A{jwt}");
        req.Headers.TryAddWithoutValidation("Accept", "application/json");
        req.Headers.TryAddWithoutValidation("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36");
        req.Headers.TryAddWithoutValidation("Referer", "https://www.cursor.com/settings");
        var json = await GetJsonAsync(req);
        if (json is null)
        {
            return new ProviderLimit(name, null, true, "Request failed", []);
        }

        var membership = TryGetString(json, "membershipType", out var m) ? m : null;
        var plan = PlanLabel(membership, "Cursor");
        var billing = ReadDate(json, "billingCycleEnd");
        var windows = new List<LimitWindow>();
        if (TryGetJsonObject(json, "individualUsage", out var ind) && TryGetJsonObject(ind, "plan", out var planObj))
        {
            var usedPercent = ReadNullableDouble(planObj, "totalPercentUsed") ?? ReadNullableDouble(planObj, "autoPercentUsed");
            if (usedPercent is null && TryGetDouble(planObj, "used", out var used) && TryGetDouble(planObj, "limit", out var limit) && limit > 0)
            {
                usedPercent = used / limit * 100;
            }
            if (usedPercent is not null)
            {
                windows.Add(new LimitWindow("Plan", usedPercent.Value, billing));
            }
        }
        return new ProviderLimit(name, plan, true, windows.Count == 0 ? "No usage data" : null, windows);
    }

    private static async Task<ProviderLimit> FetchGeminiAsync()
    {
        const string name = "gemini";
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var credsPath = Path.Combine(home, ".gemini", "oauth_creds.json");
        var creds = ReadJsonFile(credsPath);
        if (creds is null || !TryGetString(creds, "access_token", out var accessToken) || string.IsNullOrWhiteSpace(accessToken))
        {
            return new ProviderLimit(name, null, false, null, []);
        }

        var req = new HttpRequestMessage(HttpMethod.Post, "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota");
        req.Headers.TryAddWithoutValidation("Authorization", $"Bearer {accessToken}");
        req.Content = new StringContent("{}", System.Text.Encoding.UTF8, "application/json");
        var json = await GetJsonAsync(req);
        if (json is null || !TryGetArray(json, "buckets", out var buckets))
        {
            return new ProviderLimit(name, null, true, "Request failed", []);
        }

        var windows = new List<LimitWindow>();
        double lowest = 1.0;
        DateTime? resetAt = null;
        foreach (var bucket in buckets)
        {
            lowest = Math.Min(lowest, ReadNullableDouble(bucket, "remainingFraction") ?? lowest);
            resetAt ??= ReadDate(bucket, "resetTime");
        }
        if (buckets.Count > 0)
        {
            windows.Add(new LimitWindow("Quota", (1.0 - lowest) * 100, resetAt));
        }
        return new ProviderLimit(name, null, true, null, windows);
    }

    private static async Task<ProviderLimit> FetchKiroAsync()
    {
        const string name = "kiro";
        var outText = RunKiroUsage();
        if (string.IsNullOrWhiteSpace(outText))
        {
            return new ProviderLimit(name, null, false, null, []);
        }

        var lower = outText.ToLowerInvariant();
        if (lower.Contains("not logged in") || lower.Contains("login required") || lower.Contains("kiro-cli login"))
        {
            return new ProviderLimit(name, null, false, "Not logged in", []);
        }

        var cleaned = Regex.Replace(outText, @"\x1B\[[0-9;]*[a-zA-Z]", "");
        var plan = PlanLabel(FirstMatch(cleaned, @"\|\s*(KIRO\s+[\w\+]+)") ?? FirstMatch(cleaned, @"Plan:\s*(.+)"), "Kiro");
        var windows = new List<LimitWindow>();
        var coveredMatch = Regex.Match(cleaned, @"(\d+(?:\.\d+)?)\s+of\s+(\d+(?:\.\d+)?)\s+covered", RegexOptions.IgnoreCase);
        if (coveredMatch.Success)
        {
            if (double.TryParse(coveredMatch.Groups[1].Value, out var used)
                && double.TryParse(coveredMatch.Groups[2].Value, out var total)
                && total > 0)
            {
                windows.Add(new LimitWindow("Credits", used / total * 100, KiroResetDate(cleaned)));
            }
        }
        else if (TryFirstDouble(FirstMatch(cleaned, @"█+\s*(\d+)%"), out var pct))
        {
            windows.Add(new LimitWindow("Credits", pct, KiroResetDate(cleaned)));
        }

        return new ProviderLimit(name, plan, windows.Count > 0, windows.Count == 0 ? "No usage data" : null, windows);
    }

    private static async Task<ProviderLimit> FetchKimiAsync()
    {
        const string name = "kimi";
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var kimiHome = Environment.GetEnvironmentVariable("KIMI_HOME");
        var credsPath = Path.Combine(string.IsNullOrWhiteSpace(kimiHome) ? Path.Combine(home, ".kimi") : kimiHome, "credentials", "kimi-code.json");
        var creds = ReadJsonFile(credsPath);
        if (creds is null || !TryGetString(creds, "access_token", out var accessToken) || string.IsNullOrWhiteSpace(accessToken))
        {
            return new ProviderLimit(name, null, false, null, []);
        }

        var req = new HttpRequestMessage(HttpMethod.Get, "https://api.kimi.com/coding/v1/usages");
        req.Headers.TryAddWithoutValidation("Authorization", $"Bearer {accessToken}");
        req.Headers.TryAddWithoutValidation("Accept", "application/json");
        var json = await GetJsonAsync(req);
        if (json is null)
        {
            return new ProviderLimit(name, null, true, "Request failed", []);
        }

        var subType = TryGetString(json, "subType", out var st)
            ? st
            : TryGetObject(json, "user", out var user) && TryGetObject(user, "membership", out var membership) && TryGetString(membership, "level", out var lvl)
                ? lvl
                : null;
        var plan = PlanLabel(subType, "Kimi");
        var windows = new List<LimitWindow>();
        if (TryGetObject(json, "usage", out var usage))
        {
            var limit = ReadDouble(usage, "limit");
            var used = ReadDouble(usage, "used");
            if (limit > 0)
            {
                windows.Add(new LimitWindow("Usage", used / limit * 100, ReadDate(usage, "resetTime") ?? ReadDate(usage, "reset_at")));
            }
        }
        return new ProviderLimit(name, plan, true, windows.Count == 0 ? "No usage data" : null, windows);
    }

    private static async Task<ProviderLimit> FetchAntigravityAsync()
    {
        const string name = "antigravity";
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var geminiDir = Path.Combine(home, ".gemini");
        var hasAntigravity = new[] { "antigravity", "antigravity-ide", "antigravity-cli" }
            .Any(dir => Directory.Exists(Path.Combine(geminiDir, dir)));
        if (!hasAntigravity)
        {
            return new ProviderLimit(name, null, false, null, []);
        }
        return new ProviderLimit(name, null, true, "Uses Gemini quota", []);
    }

    private static string? ReadClaudeToken()
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var payload = ReadJsonFile(Path.Combine(home, ".claude", ".credentials.json"));
        if (payload is null) return null;
        if (TryGetObject(payload, "claudeAiOauth", out var oauth) && TryGetString(oauth, "accessToken", out var token)) return token;
        return null;
    }

    private static string? ReadCursorAuthId(string cliCfg)
    {
        var cfg = ReadJsonFile(cliCfg);
        if (cfg is null) return null;
        if (TryGetObject(cfg, "authInfo", out var authInfo) && TryGetString(authInfo, "authId", out var authId) && !string.IsNullOrWhiteSpace(authId))
        {
            return authId;
        }
        return null;
    }

    private static string? ReadSqliteValue(string dbPath, string sql)
    {
        try
        {
            var psi = new ProcessStartInfo
            {
                FileName = "sqlite3",
                ArgumentList = { dbPath, sql },
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                UseShellExecute = false,
                CreateNoWindow = true,
            };
            using var proc = Process.Start(psi);
            if (proc is null) return null;
            var output = proc.StandardOutput.ReadToEnd();
            proc.WaitForExit(5000);
            return string.IsNullOrWhiteSpace(output) ? null : output.Trim();
        }
        catch
        {
            return null;
        }
    }

    private static string? RunKiroUsage()
    {
        var candidates = new[]
        {
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".local", "bin", "kiro-cli"),
            "kiro-cli",
        };
        foreach (var bin in candidates)
        {
            var result = RunProcess(bin, ["chat", "--no-interactive", "/usage"], new Dictionary<string, string> { ["TERM"] = "xterm-256color" });
            if (!string.IsNullOrWhiteSpace(result)) return result;
        }
        return null;
    }

    private static string? RunProcess(string launchPath, string[] args, IDictionary<string, string>? env = null)
    {
        try
        {
            var psi = new ProcessStartInfo
            {
                FileName = launchPath,
                UseShellExecute = false,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                CreateNoWindow = true,
            };
            foreach (var arg in args) psi.ArgumentList.Add(arg);
            if (env is not null)
            {
                foreach (var kv in env) psi.Environment[kv.Key] = kv.Value;
            }
            using var p = Process.Start(psi);
            if (p is null) return null;
            var outText = p.StandardOutput.ReadToEnd();
            var errText = p.StandardError.ReadToEnd();
            p.WaitForExit(5000);
            return !string.IsNullOrWhiteSpace(errText) ? errText : outText;
        }
        catch
        {
            return null;
        }
    }

    private static async Task<Dictionary<string, JsonElement>?> GetJsonAsync(HttpRequestMessage req)
    {
        using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(10) };
        using var resp = await http.SendAsync(req);
        if (!resp.IsSuccessStatusCode) return null;
        var text = await resp.Content.ReadAsStringAsync();
        return ReadJson(text);
    }

    private static Dictionary<string, JsonElement>? ReadJsonFile(string path)
    {
        try
        {
            return File.Exists(path) ? ReadJson(File.ReadAllText(path)) : null;
        }
        catch
        {
            return null;
        }
    }

    private static Dictionary<string, JsonElement>? ReadJson(string text)
    {
        try
        {
            using var doc = JsonDocument.Parse(text);
            return doc.RootElement.ValueKind == JsonValueKind.Object
                ? doc.RootElement.EnumerateObject().ToDictionary(p => p.Name, p => p.Value.Clone())
                : null;
        }
        catch
        {
            return null;
        }
    }

    private static bool TryGetObject(JsonElement obj, string key, out JsonElement value)
    {
        if (obj.ValueKind == JsonValueKind.Object && obj.TryGetProperty(key, out value) && value.ValueKind == JsonValueKind.Object)
        {
            return true;
        }
        value = default;
        return false;
    }

    private static bool TryGetObject(Dictionary<string, JsonElement> obj, string key, out JsonElement value)
    {
        if (obj.TryGetValue(key, out value) && value.ValueKind == JsonValueKind.Object)
        {
            return true;
        }
        value = default;
        return false;
    }

    private static bool TryGetArray(Dictionary<string, JsonElement> obj, string key, out List<JsonElement> values)
    {
        values = [];
        if (!obj.TryGetValue(key, out var element) || element.ValueKind != JsonValueKind.Array) return false;
        values = element.EnumerateArray().Select(x => x.Clone()).ToList();
        return true;
    }

    private static bool TryGetJsonObject(JsonElement obj, string key, out JsonElement value)
    {
        if (obj.ValueKind == JsonValueKind.Object && obj.TryGetProperty(key, out value) && value.ValueKind == JsonValueKind.Object)
        {
            return true;
        }
        value = default;
        return false;
    }

    private static bool TryGetJsonObject(Dictionary<string, JsonElement> obj, string key, out JsonElement value)
    {
        if (obj.TryGetValue(key, out value) && value.ValueKind == JsonValueKind.Object)
        {
            return true;
        }
        value = default;
        return false;
    }

    private static bool TryGetString(JsonElement obj, string key, out string? value)
    {
        value = null;
        if (obj.ValueKind != JsonValueKind.Object || !obj.TryGetProperty(key, out var element)) return false;
        value = element.ValueKind == JsonValueKind.String ? element.GetString() : element.ToString();
        return true;
    }

    private static bool TryGetString(Dictionary<string, JsonElement> obj, string key, out string? value)
    {
        value = null;
        if (!obj.TryGetValue(key, out var element)) return false;
        value = element.ValueKind == JsonValueKind.String ? element.GetString() : element.ToString();
        return true;
    }

    private static double ReadDouble(JsonElement obj, string key)
        => ReadNullableDouble(obj, key) ?? 0;

    private static double? ReadNullableDouble(JsonElement obj, string key)
        => obj.ValueKind == JsonValueKind.Object && obj.TryGetProperty(key, out var element)
            ? element.ValueKind == JsonValueKind.Number && element.TryGetDouble(out var d)
                ? d
                : double.TryParse(element.ToString(), out var parsed) ? parsed : null
            : null;

    private static int ReadInt(JsonElement obj, string key)
        => obj.ValueKind == JsonValueKind.Object && obj.TryGetProperty(key, out var element) && element.TryGetInt32(out var i) ? i : 0;

    private static bool TryGetDouble(JsonElement obj, string key, out double value)
    {
        value = 0;
        if (obj.ValueKind != JsonValueKind.Object || !obj.TryGetProperty(key, out var element)) return false;
        if (element.ValueKind == JsonValueKind.Number && element.TryGetDouble(out value)) return true;
        return double.TryParse(element.ToString(), out value);
    }

    private static DateTime? ReadDate(JsonElement obj, string key)
    {
        if (obj.ValueKind != JsonValueKind.Object || !obj.TryGetProperty(key, out var element)) return null;
        if (element.ValueKind == JsonValueKind.String && DateTime.TryParse(element.GetString(), out var dt)) return dt;
        if (element.ValueKind == JsonValueKind.Number && element.TryGetDouble(out var n) && n > 0)
        {
            var seconds = n > 1_000_000_000_000 ? n / 1000 : n;
            return DateTimeOffset.FromUnixTimeSeconds((long)seconds).LocalDateTime;
        }
        return null;
    }

    private static DateTime? ReadDate(Dictionary<string, JsonElement> obj, string key)
        => obj.TryGetValue(key, out var element) ? ReadDate(element) : null;

    private static DateTime? ReadDate(JsonElement element)
    {
        if (element.ValueKind == JsonValueKind.String && DateTime.TryParse(element.GetString(), out var dt)) return dt;
        if (element.ValueKind == JsonValueKind.Number && element.TryGetDouble(out var n) && n > 0)
        {
            var seconds = n > 1_000_000_000_000 ? n / 1000 : n;
            return DateTimeOffset.FromUnixTimeSeconds((long)seconds).LocalDateTime;
        }
        return null;
    }

    private static string? JwtClaim(string token, string claim)
    {
        var parts = token.Split('.');
        if (parts.Length < 2) return null;
        var b64 = parts[1].Replace('-', '+').Replace('_', '/');
        while (b64.Length % 4 != 0) b64 += "=";
        try
        {
            var bytes = Convert.FromBase64String(b64);
            var json = JsonSerializer.Deserialize<Dictionary<string, object>>(bytes);
            if (json is null) return null;
            return json.TryGetValue(claim, out var value) ? value?.ToString() : null;
        }
        catch
        {
            return null;
        }
    }

    private static string? PlanLabel(string? raw, string prefix)
    {
        if (string.IsNullOrWhiteSpace(raw)) return null;
        var s = raw.Trim();
        if (s.StartsWith(prefix, StringComparison.OrdinalIgnoreCase))
        {
            s = s[prefix.Length..].Trim();
        }
        if (string.IsNullOrWhiteSpace(s)) return prefix;
        return char.ToUpperInvariant(s[0]) + s[1..].ToLowerInvariant();
    }

    private static bool TryFirstDouble(string? input, out double value)
        => double.TryParse(input, out value);

    private static string? FirstMatch(string text, string pattern)
    {
        var match = Regex.Match(text, pattern, RegexOptions.IgnoreCase);
        return match.Success && match.Groups.Count > 1 ? match.Groups[1].Value : null;
    }

    private static DateTime? KiroResetDate(string text)
    {
        var iso = FirstMatch(text, @"resets on (\d{4}-\d{2}-\d{2})");
        if (iso is not null && DateTime.TryParse(iso, out var dt)) return dt;
        var md = FirstMatch(text, @"resets on (\d{2}/\d{2})");
        if (md is null) return null;
        var parts = md.Split('/');
        if (parts.Length != 2 || !int.TryParse(parts[0], out var mm) || !int.TryParse(parts[1], out var dd)) return null;
        var now = DateTime.Now;
        var date = new DateTime(now.Year, mm, dd);
        return date < now ? date.AddYears(1) : date;
    }
}
