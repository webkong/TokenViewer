using System.Diagnostics;
using System.Linq;
using System.Text.Json;
using System.Text.RegularExpressions;
using TokenViewerWindows.Models;

namespace TokenViewerWindows.Services;

public static class LimitsService
{
    /// <summary>All canonical limit sources, in display order. A provider failure
    /// must never fail the whole fetch.</summary>
    public static readonly string[] CanonicalSources =
    {
        "claude", "codex", "cursor", "kiro", "copilot", "kimi", "antigravity",
        "zed", "trae", "windsurf", "qoder", "codebuddy", "workbuddy", "gemini", "zcode",
    };

    public static Task<IReadOnlyList<AgentLimit>> FetchAllAsync() =>
        FetchAllAsync(new (string Source, Func<Task<AgentLimit>> Fetch)[]
        {
            ("claude", FetchClaudeAsync),
            ("codex", FetchCodexAsync),
            ("cursor", FetchCursorAsync),
            ("kiro", FetchKiroAsync),
            ("copilot", FetchCopilotAsync),
            ("kimi", FetchKimiAsync),
            ("antigravity", FetchAntigravityAsync),
            ("zed", FetchZedAsync),
            ("trae", FetchTraeAsync),
            ("windsurf", FetchWindsurfAsync),
            ("qoder", FetchQoderAsync),
            ("codebuddy", FetchCodebuddyAsync),
            ("workbuddy", FetchWorkBuddyAsync),
            ("gemini", FetchGeminiAsync),
            ("zcode", FetchZcodeAsync),
        });

    /// <summary>Injectable orchestration: fetches every provider with the
    /// per-provider isolation wrapper, preserving order. Tests inject a mix of
    /// real fakes/throwing delegates; production passes the 15 real providers.</summary>
    public static async Task<IReadOnlyList<AgentLimit>> FetchAllAsync(
        IReadOnlyList<(string Source, Func<Task<AgentLimit>> Fetch)> fetchers)
    {
        var tasks = fetchers.Select(f => Safe(f.Source, f.Fetch)).ToArray();
        return await Task.WhenAll(tasks);
    }

    public static async Task<AgentLimit> Safe(string source, Func<Task<AgentLimit>> fetch)
    {
        try
        {
            return await fetch();
        }
        catch
        {
            return new AgentLimit(source, null, false, T("errError"), []);
        }
    }

    /// <summary>Localized limits string by stable key.</summary>
    private static string T(string key) => L10n.Instance[key];

    private static async Task<AgentLimit> FetchClaudeAsync()
    {
        const string name = "claude";
        var token = ReadClaudeToken();
        if (string.IsNullOrWhiteSpace(token))
        {
            return new AgentLimit(name, null, false, null, []);
        }

        var req = new HttpRequestMessage(HttpMethod.Get, "https://api.anthropic.com/api/oauth/usage");
        req.Headers.TryAddWithoutValidation("Authorization", $"Bearer {token}");
        req.Headers.TryAddWithoutValidation("anthropic-beta", "oauth-2025-04-20");
        req.Headers.TryAddWithoutValidation("Accept", "application/json");
        var json = await GetJsonAsync(req);
        if (json is null)
        {
            return new AgentLimit(name, "Claude", true, T("errRequestFailed"), []);
        }

        var windows = new List<LimitWindow>();
        foreach (var (key, label) in new[] { ("five_hour", T("window5Hour")), ("seven_day", T("window7Day")), ("seven_day_opus", T("window7DayOpus")) })
        {
            if (!json.TryGetValue(key, out var raw) || raw.ValueKind != JsonValueKind.Object) continue;
            var util = ReadDouble(raw, "utilization");
            windows.Add(new LimitWindow(label, util, ReadDate(raw, "resets_at")));
        }
        return new AgentLimit(name, "Claude", true, null, windows);
    }

    private static async Task<AgentLimit> FetchCodexAsync()
    {
        const string name = "codex";
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var authPath = Environment.GetEnvironmentVariable("CODEX_HOME") is { Length: > 0 } codexHome
            ? Path.Combine(codexHome, "auth.json")
            : Path.Combine(home, ".codex", "auth.json");
        if (!File.Exists(authPath))
        {
            return new AgentLimit(name, null, false, null, []);
        }

        var auth = ReadJsonFile(authPath);
        if (auth is null || !auth.TryGetValue("tokens", out var tokensObj) || tokensObj.ValueKind != JsonValueKind.Object ||
            !TryGetString(tokensObj, "access_token", out var accessToken) || string.IsNullOrWhiteSpace(accessToken))
        {
            return new AgentLimit(name, null, false, null, []);
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
            return new AgentLimit(name, plan, true, T("errRequestFailed"), []);
        }

        var windows = new List<LimitWindow>();
        foreach (var key in new[] { "primary_window", "secondary_window" })
        {
            if (!TryGetJsonObject(rl, key, out var w)) continue;
            var secs = ReadInt(w, "limit_window_seconds");
            var label = secs >= 604800 ? T("windowWeekly") : secs >= 18000 ? T("window5Hour") : T("windowWindow");
            windows.Add(new LimitWindow(label, ReadDouble(w, "used_percent"), ReadDate(w, "reset_at")));
        }
        return new AgentLimit(name, plan, true, null, windows);
    }

    private static async Task<AgentLimit> FetchCursorAsync()
    {
        const string name = "cursor";
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var stateDb = Path.Combine(home, "AppData", "Roaming", "Cursor", "User", "globalStorage", "state.vscdb");
        var cliCfg = Path.Combine(home, ".cursor", "cli-config.json");
        if (!File.Exists(stateDb))
        {
            return new AgentLimit(name, null, false, null, []);
        }

        var jwt = CoreBridge.ReadCursorAccessToken(stateDb);
        if (string.IsNullOrWhiteSpace(jwt) || jwt.Length < 10)
        {
            return new AgentLimit(name, null, false, null, []);
        }

        var authId = ReadCursorAuthId(cliCfg);
        var userId = !string.IsNullOrWhiteSpace(authId) ? authId : JwtClaim(jwt, "sub");
        if (string.IsNullOrWhiteSpace(userId))
        {
            return new AgentLimit(name, null, false, T("errNoUserId"), []);
        }

        var req = new HttpRequestMessage(HttpMethod.Get, "https://cursor.com/api/usage-summary");
        req.Headers.TryAddWithoutValidation("Cookie", $"WorkosCursorSessionToken={userId}%3A%3A{jwt}");
        req.Headers.TryAddWithoutValidation("Accept", "application/json");
        req.Headers.TryAddWithoutValidation("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36");
        req.Headers.TryAddWithoutValidation("Referer", "https://www.cursor.com/settings");
        var json = await GetJsonAsync(req);
        if (json is null)
        {
            return new AgentLimit(name, null, true, T("errRequestFailed"), []);
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
                windows.Add(new LimitWindow(T("windowPlan"), usedPercent.Value, billing));
            }
        }
        return new AgentLimit(name, plan, true, windows.Count == 0 ? T("errNoUsageData") : null, windows);
    }

    private static async Task<AgentLimit> FetchGeminiAsync()
    {
        const string name = "gemini";
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var credsPath = Path.Combine(home, ".gemini", "oauth_creds.json");
        var creds = ReadJsonFile(credsPath);
        if (creds is null || !TryGetString(creds, "access_token", out var accessToken) || string.IsNullOrWhiteSpace(accessToken))
        {
            return new AgentLimit(name, null, false, null, []);
        }

        var req = new HttpRequestMessage(HttpMethod.Post, "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota");
        req.Headers.TryAddWithoutValidation("Authorization", $"Bearer {accessToken}");
        req.Content = new StringContent("{}", System.Text.Encoding.UTF8, "application/json");
        var json = await GetJsonAsync(req);
        if (json is null || !TryGetArray(json, "buckets", out var buckets))
        {
            return new AgentLimit(name, null, true, T("errRequestFailed"), []);
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
            windows.Add(new LimitWindow(T("windowQuota"), (1.0 - lowest) * 100, resetAt));
        }
        return new AgentLimit(name, null, true, null, windows);
    }

    private static async Task<AgentLimit> FetchKiroAsync()
    {
        const string name = "kiro";
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        if (string.IsNullOrWhiteSpace(localAppData))
        {
            localAppData = Path.Combine(home, "AppData", "Local");
        }
        var dbPath = Path.Combine(localAppData, "kiro-cli", "data.sqlite3");
        if (!File.Exists(dbPath) || !CoreBridge.HasKiroLogin(dbPath))
        {
            return new AgentLimit(name, null, false, null, []);
        }

        var outText = RunKiroUsage();
        if (string.IsNullOrWhiteSpace(outText))
        {
            return new AgentLimit(name, null, false, null, []);
        }

        var lower = outText.ToLowerInvariant();
        if (lower.Contains("not logged in") || lower.Contains("login required") || lower.Contains("kiro-cli login"))
        {
            return new AgentLimit(name, null, false, T("errNotLoggedIn"), []);
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
                windows.Add(new LimitWindow(T("windowCredits"), used / total * 100, KiroResetDate(cleaned)));
            }
        }
        else if (TryFirstDouble(FirstMatch(cleaned, @"█+\s*(\d+)%"), out var pct))
        {
            windows.Add(new LimitWindow(T("windowCredits"), pct, KiroResetDate(cleaned)));
        }

        return new AgentLimit(name, plan, windows.Count > 0, windows.Count == 0 ? T("errNoUsageData") : null, windows);
    }

    private static async Task<AgentLimit> FetchKimiAsync()
    {
        const string name = "kimi";
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var kimiHome = Environment.GetEnvironmentVariable("KIMI_HOME");
        var credsPath = Path.Combine(string.IsNullOrWhiteSpace(kimiHome) ? Path.Combine(home, ".kimi") : kimiHome, "credentials", "kimi-code.json");
        var creds = ReadJsonFile(credsPath);
        if (creds is null || !TryGetString(creds, "access_token", out var accessToken) || string.IsNullOrWhiteSpace(accessToken))
        {
            return new AgentLimit(name, null, false, null, []);
        }

        var req = new HttpRequestMessage(HttpMethod.Get, "https://api.kimi.com/coding/v1/usages");
        req.Headers.TryAddWithoutValidation("Authorization", $"Bearer {accessToken}");
        req.Headers.TryAddWithoutValidation("Accept", "application/json");
        var json = await GetJsonAsync(req);
        if (json is null)
        {
            return new AgentLimit(name, null, true, T("errRequestFailed"), []);
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
                windows.Add(new LimitWindow(T("windowUsage"), used / limit * 100, ReadDate(usage, "resetTime") ?? ReadDate(usage, "reset_at")));
            }
        }
        return new AgentLimit(name, plan, true, windows.Count == 0 ? T("errNoUsageData") : null, windows);
    }

    private static async Task<AgentLimit> FetchAntigravityAsync()
    {
        const string name = "antigravity";
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var geminiDir = Path.Combine(home, ".gemini");
        var hasAntigravity = new[] { "antigravity", "antigravity-ide", "antigravity-cli" }
            .Any(dir => Directory.Exists(Path.Combine(geminiDir, dir)));
        if (!hasAntigravity)
        {
            return new AgentLimit(name, null, false, null, []);
        }
        return new AgentLimit(name, null, true, T("errUsesGeminiQuota"), []);
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

    // MARK: Copilot (apps.json → GitHub API)

    private static async Task<AgentLimit> FetchCopilotAsync()
    {
        const string name = "copilot";
        var token = CopilotToken();
        if (string.IsNullOrWhiteSpace(token))
        {
            return new AgentLimit(name, null, false, null, []);
        }

        var req = new HttpRequestMessage(HttpMethod.Get, "https://api.github.com/copilot_internal/user");
        req.Headers.TryAddWithoutValidation("Authorization", $"token {token}");
        req.Headers.TryAddWithoutValidation("Accept", "application/json");
        req.Headers.TryAddWithoutValidation("Editor-Version", "vscode/1.96.2");
        req.Headers.TryAddWithoutValidation("Editor-Plugin-Version", "copilot-chat/0.26.7");
        req.Headers.TryAddWithoutValidation("User-Agent", "GitHubCopilotChat/0.26.7");
        req.Headers.TryAddWithoutValidation("X-Github-Api-Version", "2025-04-01");
        var json = await GetJsonAsync(req);
        if (json is null)
        {
            return new AgentLimit(name, null, true, T("errRequestFailed"), []);
        }

        var plan = PlanLabel(ReadString(json, "copilot_plan"), "Copilot");
        var reset = ReadDate(json, "quota_reset_date");
        var windows = new List<LimitWindow>();
        if (json.TryGetValue("quota_snapshots", out var snaps) && snaps.ValueKind == JsonValueKind.Object)
        {
            foreach (var (key, label) in new[] { ("premium_interactions", T("windowPremium")), ("chat", T("windowChat")) })
            {
                if (!snaps.TryGetProperty(key, out var q) || q.ValueKind != JsonValueKind.Object) continue;
                windows.Add(new LimitWindow(label, SnapshotUsedPercent(q), reset));
            }
        }
        return new AgentLimit(name, plan, true, null, windows, QuotaResetAt: reset);
    }

    public static double SnapshotUsedPercent(JsonElement q)
    {
        if (q.TryGetProperty("percent_remaining", out var pr) && Numeric(pr) is { } remaining)
        {
            return Math.Clamp(100 - remaining, 0, 100);
        }
        var entitlement = Numeric(q.TryGetProperty("entitlement", out var e) ? e : default);
        var remaining2 = Numeric(q.TryGetProperty("remaining", out var r) ? r : default);
        if (entitlement is { } ent && remaining2 is { } rem && ent > 0)
        {
            return Math.Clamp((ent - rem) / ent * 100, 0, 100);
        }
        return 0;
    }

    // MARK: Zed / Trae / Windsurf / Qoder (cockpit-tools account cache)

    private static async Task<AgentLimit> FetchZedAsync()
    {
        const string name = "zed";
        var account = ReadCockpitAccount(name);
        if (account is null)
        {
            return new AgentLimit(name, null, false, null, []);
        }
        var expiry = NestedDate(account, new[]
        {
            new[] { "billing_period_end_at" },
            new[] { "public_account", "billing_period_end_at" },
            new[] { "subscription_raw", "subscription", "period", "end_at" },
            new[] { "subscription_raw", "period", "end_at" },
        });
        var plan = PlanLabel(NestedString(account, new[]
        {
            new[] { "plan_raw" },
            new[] { "public_account", "plan_raw" },
            new[] { "subscription_raw", "subscription", "name" },
            new[] { "subscription_raw", "name" },
        }), "Zed");
        return new AgentLimit(name, plan, true, expiry is null ? T("errNoSubscriptionData") : null, [], SubscriptionExpiresAt: expiry);
    }

    private static async Task<AgentLimit> FetchTraeAsync()
    {
        const string name = "trae";
        var account = ReadCockpitAccount(name);
        if (account is null)
        {
            return new AgentLimit(name, null, false, null, []);
        }
        var reset = NestedDate(account, new[]
        {
            new[] { "plan_reset_at" },
            new[] { "public_account", "plan_reset_at" },
            new[] { "trae_entitlement_raw", "detail", "subscription_renew_time" },
            new[] { "trae_entitlement_raw", "detail", "subscriptionRenewTime" },
            new[] { "trae_entitlement_raw", "data", "detail", "subscription_renew_time" },
        });
        var plan = PlanLabel(NestedString(account, new[]
        {
            new[] { "plan_type" },
            new[] { "public_account", "plan_type" },
            new[] { "trae_entitlement_raw", "plan_type" },
        }), "Trae");
        return new AgentLimit(name, plan, true, reset is null ? T("errNoSubscriptionData") : null, [], SubscriptionResetAt: reset);
    }

    private static async Task<AgentLimit> FetchWindsurfAsync()
    {
        const string name = "windsurf";
        var account = ReadCockpitAccount(name);
        if (account is null)
        {
            return new AgentLimit(name, null, false, null, []);
        }
        var reset = NestedDate(account, new[]
        {
            new[] { "copilot_quota_reset_date" },
            new[] { "copilot_limited_user_reset_date" },
            new[] { "public_account", "copilot_quota_reset_date" },
        });
        var plan = PlanLabel(NestedString(account, new[]
        {
            new[] { "copilot_plan" },
            new[] { "public_account", "copilot_plan" },
            new[] { "windsurf_plan_status", "plan" },
            new[] { "windsurf_plan_status", "planName" },
        }), "Windsurf");
        var windows = CopilotStyleWindows(account, reset);
        return new AgentLimit(name, plan, true, windows.Count == 0 ? T("errNoUsageData") : null, windows, QuotaResetAt: reset);
    }

    private static async Task<AgentLimit> FetchQoderAsync()
    {
        const string name = "qoder";
        var account = ReadCockpitAccount(name);
        if (account is null)
        {
            return new AgentLimit(name, null, false, null, []);
        }
        var plan = PlanLabel(NestedString(account, new[]
        {
            new[] { "plan_type" },
            new[] { "public_account", "plan_type" },
            new[] { "auth_user_plan_raw", "plan_type" },
            new[] { "auth_user_plan_raw", "planType" },
        }), "Qoder");
        var windows = new List<LimitWindow>();
        var pct = Numeric(Nested(account, new[] { "credits_usage_percent" }) ?? default)
            ?? Numeric(Nested(account, new[] { "public_account", "credits_usage_percent" }) ?? default);
        if (pct is { } p)
        {
            windows.Add(new LimitWindow(T("windowCredits"), Math.Clamp(p, 0, 100), null));
        }
        return new AgentLimit(name, plan, true, windows.Count == 0 ? T("errNoUsageData") : null, windows);
    }

    // MARK: CodeBuddy / WorkBuddy

    private static async Task<AgentLimit> FetchCodebuddyAsync()
    {
        const string name = "codebuddy";
        var cached = ReadCockpitAccount(name);
        var auth = ReadCodebuddyAuth();
        if (auth is null)
        {
            // No live auth: fall back to the cockpit cache, else not configured.
            return cached is not null
                ? WorkBuddyLimit(cached, true, name, "CodeBuddy")
                : new AgentLimit(name, null, false, null, []);
        }

        if (await RefreshWorkBuddyAccount(auth) is { } refreshed)
        {
            WriteCockpitAccountSnapshot(name, refreshed);
            return WorkBuddyLimit(refreshed, true, name, "CodeBuddy");
        }
        return cached is not null
            ? WorkBuddyLimit(cached, true, name, "CodeBuddy")
            : new AgentLimit(name, "CodeBuddy", true, T("errRequestFailed"), []);
    }

    private static async Task<AgentLimit> FetchWorkBuddyAsync()
    {
        const string name = "workbuddy";
        var cached = ReadCockpitAccount(name);
        var auth = ReadWorkBuddyAuth();
        if (auth is null)
        {
            return cached is not null
                ? WorkBuddyLimit(cached, true)
                : new AgentLimit(name, null, false, null, []);
        }

        if (await RefreshWorkBuddyAccount(auth) is { } refreshed)
        {
            WriteCockpitAccountSnapshot(name, refreshed);
            return WorkBuddyLimit(refreshed, true);
        }
        return cached is not null
            ? WorkBuddyLimit(cached, true)
            : new AgentLimit(name, "WorkBuddy", true, T("errRequestFailed"), []);
    }

    /// <summary>Makes the three CodeBuddy/WorkBuddy billing requests and folds the
    /// responses into one account object (quota_raw + usage_raw + payment_type).</summary>
    private static async Task<Dictionary<string, JsonElement>?> RefreshWorkBuddyAccount(Dictionary<string, JsonElement> auth)
    {
        var authInfo = auth.TryGetValue("auth", out var a) && a.ValueKind == JsonValueKind.Object ? a : default;
        var accountInfo = auth.TryGetValue("account", out var ac) && ac.ValueKind == JsonValueKind.Object ? ac : default;

        var accessToken = FirstString(authInfo, new[] { "accessToken", "access_token" });
        if (string.IsNullOrWhiteSpace(accessToken)) return null;

        var uid = FirstString(accountInfo, new[] { "uid", "id" });
        var enterpriseId = FirstString(accountInfo, new[] { "enterpriseId", "enterprise_id" });
        var domain = FirstString(authInfo, new[] { "domain" });
        var nickname = FirstString(accountInfo, new[] { "nickname", "label" });
        var email = FirstString(accountInfo, new[] { "email" }) ?? nickname ?? uid ?? "unknown";

        var dosage = await PostWorkBuddyJson("/v2/billing/meter/get-dosage-notify", accessToken, uid, enterpriseId, domain, "{}");
        var payment = await PostWorkBuddyJson("/v2/billing/meter/get-payment-type", accessToken, uid, enterpriseId, domain, "{}");
        var userResource = await PostWorkBuddyJson("/v2/billing/meter/get-user-resource", accessToken, uid, enterpriseId, domain, WorkBuddyUserResourceBody());

        if (dosage is null && payment is null && userResource is null) return null;

        var paymentType = payment is null ? null : NestedString(payment, new[] { new[] { "data", "paymentType" }, new[] { "data" } });

        var account = new Dictionary<string, object>();
        var nowSecs = DateTimeOffset.UtcNow.ToUnixTimeSeconds();
        account["id"] = "workbuddy_" + SanitizeFileComponent(string.IsNullOrWhiteSpace(uid) ? email : uid);
        account["email"] = email;
        account["last_used"] = nowSecs;
        account["created_at"] = nowSecs;
        if (paymentType is not null) account["payment_type"] = paymentType;
        var quota = new Dictionary<string, object>();
        if (dosage is not null) quota["dosage"] = dosage;
        if (payment is not null) quota["payment"] = payment;
        if (userResource is not null) quota["userResource"] = userResource;
        if (quota.Count > 0) account["quota_raw"] = quota;
        if (userResource is not null) account["usage_raw"] = userResource;

        return ReadJson(JsonSerializer.Serialize(account));
    }

    /// <summary>Best-effort persistence of a refreshed account into the cockpit
    /// account cache. Never surfaces token/auth/body on failure.</summary>
    public static void WriteCockpitAccountSnapshot(string agent, Dictionary<string, JsonElement> account, string? baseDir = null)
    {
        try
        {
            var id = FirstString(account, new[] { "id" });
            if (string.IsNullOrWhiteSpace(id)) return;

            var home = baseDir ?? Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
            var root = Path.Combine(home, ".antigravity_cockpit");
            var accountsDir = Path.Combine(root, $"{agent}_accounts");
            var indexPath = Path.Combine(root, $"{agent}_accounts.json");
            var detailPath = Path.Combine(accountsDir, $"{SanitizeFileComponent(id)}.json");

            Directory.CreateDirectory(accountsDir);
            AtomicWriteText(detailPath, JsonSerializer.Serialize(account));

            var summary = new Dictionary<string, object>
            {
                ["id"] = id,
                ["email"] = FirstString(account, new[] { "email" }) ?? id,
                ["plan_type"] = FirstString(account, new[] { "plan_type", "payment_type" }) ?? "",
                ["last_used"] = DateTimeOffset.UtcNow.ToUnixTimeSeconds(),
            };
            var index = new Dictionary<string, object> { ["version"] = "1.0", ["accounts"] = new[] { summary } };
            AtomicWriteText(indexPath, JsonSerializer.Serialize(index));
        }
        catch
        {
            // Snapshot is best-effort; ignore any I/O/serialization failure.
        }
    }

    /// <summary>Atomically writes a file via a same-directory temp file + rename,
    /// so a process interruption never leaves a truncated target. Best-effort:
    /// the temp file is removed on failure and no content is surfaced.</summary>
    private static void AtomicWriteText(string path, string content)
    {
        var dir = Path.GetDirectoryName(path);
        if (string.IsNullOrEmpty(dir)) return;
        Directory.CreateDirectory(dir);
        var tmp = Path.Combine(dir, $".{Path.GetFileName(path)}.{Guid.NewGuid():N}.tmp");
        try
        {
            File.WriteAllText(tmp, content);
            File.Move(tmp, path, overwrite: true);
        }
        catch
        {
            try { if (File.Exists(tmp)) File.Delete(tmp); } catch { }
        }
    }

    private static string SanitizeFileComponent(string raw)
    {
        var cleaned = Regex.Replace(raw, "[^A-Za-z0-9._-]", "_");
        return string.IsNullOrEmpty(cleaned) ? "local" : cleaned;
    }

    private static async Task<Dictionary<string, JsonElement>?> PostWorkBuddyJson(
        string path, string accessToken, string? uid, string? enterpriseId, string? domain, string body)
    {
        var req = new HttpRequestMessage(HttpMethod.Post, $"https://www.codebuddy.cn{path}");
        req.Headers.TryAddWithoutValidation("Authorization", $"Bearer {accessToken}");
        req.Headers.TryAddWithoutValidation("Accept", "application/json, text/plain, */*");
        if (!string.IsNullOrWhiteSpace(uid)) req.Headers.TryAddWithoutValidation("X-User-Id", uid);
        if (!string.IsNullOrWhiteSpace(enterpriseId))
        {
            req.Headers.TryAddWithoutValidation("X-Enterprise-Id", enterpriseId);
            req.Headers.TryAddWithoutValidation("X-Tenant-Id", enterpriseId);
        }
        if (!string.IsNullOrWhiteSpace(domain)) req.Headers.TryAddWithoutValidation("X-Domain", domain);
        req.Content = new StringContent(body, System.Text.Encoding.UTF8, "application/json");
        return await GetJsonAsync(req);
    }

    private static string WorkBuddyUserResourceBody()
    {
        var now = DateTime.Now;
        var end = now.AddYears(101);
        return JsonSerializer.Serialize(new Dictionary<string, object>
        {
            ["PageNumber"] = 1,
            ["PageSize"] = 100,
            ["ProductCode"] = "p_tcaca",
            ["Status"] = new[] { 0, 3 },
            ["PackageEndTimeRangeBegin"] = now.ToString("yyyy-MM-dd HH:mm:ss"),
            ["PackageEndTimeRangeEnd"] = end.ToString("yyyy-MM-dd HH:mm:ss"),
        });
    }

    private static AgentLimit WorkBuddyLimit(Dictionary<string, JsonElement> account, bool configured, string agentName = "workbuddy", string displayPrefix = "WorkBuddy")
    {
        var plan = PlanLabel(NestedString(account, new[]
        {
            new[] { "payment_type" },
            new[] { "plan_type" },
            new[] { "quota_raw", "payment", "data", "paymentType" },
        }), displayPrefix);
        var windows = WorkBuddyWindows(account);
        return new AgentLimit(
            agentName, plan, configured, windows.Count == 0 ? T("errNoUsageData") : null, windows,
            QuotaResetAt: windows.Select(w => w.ResetAt).Where(d => d is not null).OrderBy(d => d).FirstOrDefault());
    }

    public static List<LimitWindow> WorkBuddyWindows(Dictionary<string, JsonElement> account)
    {
        var roots = new[]
        {
            Nested(account, new[] { "usage_raw" }),
            Nested(account, new[] { "quota_raw", "userResource" }),
            Nested(account, new[] { "quota_raw" }),
        };
        foreach (var root in roots)
        {
            if (root is not { } r) continue;
            var accounts = Nested(r, new[] { "data", "Response", "Data", "Accounts" });
            if (accounts is not { } a || a.ValueKind != JsonValueKind.Array) continue;

            double used = 0, total = 0;
            DateTime? resetAt = null;
            foreach (var item in a.EnumerateArray())
            {
                if (item.TryGetProperty("Status", out var status) && status.TryGetInt32(out var st) && st != 0) continue;
                var itemTotal = Numeric(item.TryGetProperty("CycleCapacitySizePrecise", out var c1) ? c1
                    : item.TryGetProperty("CycleCapacitySize", out var c2) ? c2
                    : item.TryGetProperty("CapacitySizePrecise", out var c3) ? c3
                    : item.TryGetProperty("CapacitySize", out var c4) ? c4 : default) ?? 0;
                var itemRemain = Numeric(item.TryGetProperty("CycleCapacityRemainPrecise", out var r1) ? r1
                    : item.TryGetProperty("CycleCapacityRemain", out var r2) ? r2
                    : item.TryGetProperty("CapacityRemainPrecise", out var r3) ? r3
                    : item.TryGetProperty("CapacityRemain", out var r4) ? r4 : default) ?? 0;
                if (itemTotal <= 0) continue;
                total += itemTotal;
                used += Math.Max(itemTotal - itemRemain, 0);
                var candidate = ReadDate(item.TryGetProperty("CycleEndTime", out var e1) ? e1
                    : item.TryGetProperty("ExpiredTime", out var e2) ? e2
                    : item.TryGetProperty("DeductionEndTime", out var e3) ? e3 : default);
                if (candidate is { } c)
                {
                    resetAt = resetAt is null ? c : (c > DateTime.Now && c < resetAt ? c : resetAt);
                }
            }
            if (total <= 0) continue;
            return new List<LimitWindow> { new(T("windowCredits"), Math.Clamp(used / total * 100, 0, 100), resetAt) };
        }
        return [];
    }

    // MARK: ZCode (智谱 / Z.ai)

    private static async Task<AgentLimit> FetchZcodeAsync()
    {
        const string name = "zcode";
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var zcodeDir = Path.Combine(home, ".zcode");
        if (!Directory.Exists(zcodeDir))
        {
            return new AgentLimit(name, null, false, null, []);
        }

        var cfg = ReadJsonFile(Path.Combine(zcodeDir, "v2", "config.json"));
        if (cfg is null || !cfg.TryGetValue("provider", out var providersEl) || providersEl.ValueKind != JsonValueKind.Object)
        {
            return new AgentLimit(name, null, false, null, []);
        }

        var best = providersEl.EnumerateObject()
            .Select(p => (Id: p.Name, Obj: p.Value))
            .Where(p => p.Obj.ValueKind == JsonValueKind.Object)
            .OrderByDescending(p => ZcodeProviderScore(p.Id, p.Obj))
            .FirstOrDefault();

        if (best.Obj.ValueKind != JsonValueKind.Object)
        {
            return new AgentLimit(name, null, false, null, []);
        }

        var plan = ZcodePlanLabel(best.Id, ReadString(best.Obj, "name"));
        var hasCliDb = File.Exists(Path.Combine(zcodeDir, "cli", "db", "db.sqlite"));
        var configured = plan is not null || hasCliDb;

        if (!best.Obj.TryGetProperty("options", out var options) || options.ValueKind != JsonValueKind.Object
            || !options.TryGetProperty("apiKey", out var apiKeyEl) || apiKeyEl.ValueKind != JsonValueKind.String
            || string.IsNullOrWhiteSpace(apiKeyEl.GetString()))
        {
            return new AgentLimit(name, plan, configured, configured ? T("errNoApiKey") : null, []);
        }

        var req = new HttpRequestMessage(HttpMethod.Get, "https://zcode.z.ai/api/v1/zcode-plan/billing/balance?app_version=3.1.1");
        req.Headers.TryAddWithoutValidation("Authorization", $"Bearer {apiKeyEl.GetString()}");
        req.Headers.TryAddWithoutValidation("Accept", "application/json");
        var json = await GetJsonAsync(req);
        if (json is null)
        {
            return new AgentLimit(name, plan, true, T("errRequestFailed"), []);
        }

        var dataObj = json.TryGetValue("data", out var d) && d.ValueKind == JsonValueKind.Object ? d : default;
        var balancesEl = (dataObj.ValueKind == JsonValueKind.Object && dataObj.TryGetProperty("balances", out var b) ? b : default);
        if (balancesEl.ValueKind != JsonValueKind.Array && json.TryGetValue("balances", out var b2)) balancesEl = b2;

        var windows = new List<LimitWindow>();
        if (balancesEl.ValueKind == JsonValueKind.Array)
        {
            foreach (var bal in balancesEl.EnumerateArray())
            {
                var label = bal.TryGetProperty("show_name", out var sn) && sn.ValueKind == JsonValueKind.String ? sn.GetString()! : T("windowUsage");
                var used = Numeric(bal.TryGetProperty("used_units", out var u) ? u : default) ?? 0;
                var total = Numeric(bal.TryGetProperty("total_units", out var t) ? t : default) ?? 0;
                var reset = ReadDate(bal.TryGetProperty("period_end", out var pe) ? pe : default);
                windows.Add(new LimitWindow(label, total > 0 ? used / total * 100 : 0, reset));
            }
        }

        return new AgentLimit(name, plan, true, windows.Count == 0 ? T("errNoActiveQuota") : null, windows);
    }

    private static int ZcodeProviderScore(string id, JsonElement obj)
    {
        var enabled = obj.TryGetProperty("enabled", out var e) && e.ValueKind is JsonValueKind.True;
        var hasKey = obj.TryGetProperty("options", out var o) && o.ValueKind == JsonValueKind.Object
            && o.TryGetProperty("apiKey", out var k) && k.ValueKind == JsonValueKind.String && !string.IsNullOrWhiteSpace(k.GetString());
        var idL = id.ToLowerInvariant();
        var s = 0;
        if (enabled) s += 4;
        if (hasKey) s += 2;
        if (idL.Contains("start-plan")) s += 3;
        else if (idL.Contains("coding-plan")) s += 2;
        else if (idL.Contains("bigmodel") || idL.Contains("zai")) s += 1;
        return s;
    }

    private static string? ZcodePlanLabel(string providerId, string? fallback)
    {
        var id = providerId.ToLowerInvariant();
        if (id.Contains("start-plan")) return "Start Plan";
        if (id.Contains("coding-plan")) return "Coding Plan";
        if (id.Contains("bigmodel")) return "BigModel API";
        if (id.Contains("zai")) return "Z.ai API";
        return PlanLabel(fallback, "ZCode");
    }

    // MARK: Cockpit account cache + nested JSON helpers

    private static Dictionary<string, JsonElement>? ReadCockpitAccount(string agent)
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var baseDir = Path.Combine(home, ".antigravity_cockpit");
        var indexPath = Path.Combine(baseDir, $"{agent}_accounts.json");
        var accountsDir = Path.Combine(baseDir, $"{agent}_accounts");

        if (ReadJsonFile(indexPath) is { } index && CockpitAccountId(index) is { } id)
        {
            if (ReadJsonFile(Path.Combine(accountsDir, $"{id}.json")) is { } detail) return detail;
        }

        if (Directory.Exists(accountsDir))
        {
            foreach (var file in Directory.GetFiles(accountsDir, "*.json").OrderByDescending(File.GetLastWriteTimeUtc))
            {
                if (ReadJsonFile(file) is { } detail) return detail;
            }
        }
        return null;
    }

    private static string? CockpitAccountId(Dictionary<string, JsonElement> index)
    {
        if (index.TryGetValue("accounts", out var accountsEl) && accountsEl.ValueKind == JsonValueKind.Array)
        {
            var best = accountsEl.EnumerateArray()
                .Where(a => a.ValueKind == JsonValueKind.Object)
                .OrderByDescending(a =>
                    Numeric(a.TryGetProperty("last_used_at", out var l1) ? l1
                        : a.TryGetProperty("last_used", out var l2) ? l2
                        : a.TryGetProperty("updated_at", out var l3) ? l3 : default) ?? 0)
                .FirstOrDefault();
            if (best.ValueKind == JsonValueKind.Object)
            {
                foreach (var key in new[] { "id", "account_id", "user_id" })
                {
                    if (best.TryGetProperty(key, out var v) && v.ValueKind == JsonValueKind.String) return v.GetString();
                }
            }
        }
        return FirstString(index, new[] { "current_account_id", "active_account_id", "account_id", "id" });
    }

    private static List<LimitWindow> CopilotStyleWindows(Dictionary<string, JsonElement> account, DateTime? resetAt)
    {
        var windows = new List<LimitWindow>();
        var snapshots = Nested(account, new[] { "copilot_quota_snapshots" }) ?? Nested(account, new[] { "public_account", "copilot_quota_snapshots" });
        if (snapshots is { } snaps && snaps.ValueKind == JsonValueKind.Object)
        {
            foreach (var (key, label) in new[] { ("premium_interactions", T("windowPremium")), ("premium_models", T("windowPremium")) })
            {
                if (snaps.TryGetProperty(key, out var q) && q.ValueKind == JsonValueKind.Object && Numeric(q.TryGetProperty("percent_remaining", out var pr) ? pr : default) is { } remaining)
                {
                    windows.Add(new LimitWindow(label, Math.Clamp(100 - remaining, 0, 100), resetAt));
                    break;
                }
            }
            if (snaps.TryGetProperty("chat", out var chat) && chat.ValueKind == JsonValueKind.Object && Numeric(chat.TryGetProperty("percent_remaining", out var cr) ? cr : default) is { } chatRemaining)
            {
                windows.Add(new LimitWindow(T("windowChat"), Math.Clamp(100 - chatRemaining, 0, 100), resetAt));
            }
        }
        return windows;
    }

    private static JsonElement? Nested(Dictionary<string, JsonElement> obj, string[] path)
    {
        if (path.Length == 0 || !obj.TryGetValue(path[0], out var current)) return null;
        for (var i = 1; i < path.Length; i++)
        {
            if (current.ValueKind != JsonValueKind.Object || !current.TryGetProperty(path[i], out current)) return null;
        }
        return current;
    }

    private static JsonElement? Nested(JsonElement obj, string[] path)
    {
        if (path.Length == 0) return obj;
        var current = obj;
        foreach (var key in path)
        {
            if (current.ValueKind != JsonValueKind.Object || !current.TryGetProperty(key, out current)) return null;
        }
        return current;
    }

    private static string? NestedString(Dictionary<string, JsonElement> obj, string[][] paths)
    {
        foreach (var path in paths)
        {
            var v = Nested(obj, path);
            if (v is { } el && el.ValueKind == JsonValueKind.String && !string.IsNullOrWhiteSpace(el.GetString())) return el.GetString();
        }
        return null;
    }

    private static DateTime? NestedDate(Dictionary<string, JsonElement> obj, string[][] paths)
    {
        foreach (var path in paths)
        {
            if (Nested(obj, path) is { } el && ReadDate(el) is { } d) return d;
        }
        return null;
    }

    private static double? Numeric(JsonElement el)
    {
        if (el.ValueKind == JsonValueKind.Number && el.TryGetDouble(out var d)) return d;
        if (el.ValueKind == JsonValueKind.String && double.TryParse(el.GetString(), out var s)) return s;
        return null;
    }

    private static string? FirstString(Dictionary<string, JsonElement> obj, string[] keys)
    {
        foreach (var key in keys)
        {
            if (!obj.TryGetValue(key, out var el)) continue;
            if (el.ValueKind == JsonValueKind.String && !string.IsNullOrWhiteSpace(el.GetString())) return el.GetString();
            if (el.ValueKind == JsonValueKind.Number) return el.ToString();
        }
        return null;
    }

    private static string? FirstString(JsonElement obj, string[] keys)
    {
        if (obj.ValueKind != JsonValueKind.Object) return null;
        foreach (var key in keys)
        {
            if (!obj.TryGetProperty(key, out var el)) continue;
            if (el.ValueKind == JsonValueKind.String && !string.IsNullOrWhiteSpace(el.GetString())) return el.GetString();
            if (el.ValueKind == JsonValueKind.Number) return el.ToString();
        }
        return null;
    }

    private static string? ReadString(Dictionary<string, JsonElement> obj, string key)
        => obj.TryGetValue(key, out var el) && el.ValueKind == JsonValueKind.String ? el.GetString() : null;

    private static string? ReadString(JsonElement obj, string key)
        => obj.ValueKind == JsonValueKind.Object && obj.TryGetProperty(key, out var el) && el.ValueKind == JsonValueKind.String ? el.GetString() : null;

    private static string? CopilotToken()
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var baseDir = Path.Combine(home, ".config", "github-copilot");
        foreach (var file in new[] { "apps.json", "hosts.json" })
        {
            var json = ReadJsonFile(Path.Combine(baseDir, file));
            if (json is null) continue;
            foreach (var (key, entry) in json.OrderByDescending(p => p.Key.StartsWith("github.com", StringComparison.OrdinalIgnoreCase)))
            {
                if (entry.ValueKind != JsonValueKind.Object) continue;
                if (entry.TryGetProperty("oauth_token", out var t) && t.ValueKind == JsonValueKind.String && !string.IsNullOrWhiteSpace(t.GetString()))
                {
                    return t.GetString();
                }
            }
        }
        return null;
    }

    private static Dictionary<string, JsonElement>? ReadCodebuddyAuth()
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var path = Path.Combine(home, "AppData", "Roaming", "CodeBuddyExtension", "Data", "Public", "auth", "Tencent-Cloud.coding-copilot.info");
        return ReadJsonFile(path) ?? ReadJsonFile(Path.Combine(home, ".local", "share", "CodeBuddyExtension", "Data", "Public", "auth", "Tencent-Cloud.coding-copilot.info"));
    }

    private static Dictionary<string, JsonElement>? ReadWorkBuddyAuth()
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var path = Path.Combine(home, "AppData", "Roaming", "CodeBuddyExtension", "Data", "Public", "auth", "workbuddy-desktop.info");
        return ReadJsonFile(path) ?? ReadJsonFile(Path.Combine(home, ".local", "share", "CodeBuddyExtension", "Data", "Public", "auth", "workbuddy-desktop.info"));
    }
}
