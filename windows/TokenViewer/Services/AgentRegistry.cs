namespace TokenViewerWindows.Services;

/// <summary>Static source → display-name / brand-color / logo-file mapping for
/// the coding agents TokenViewer surfaces. Mirrors the macOS AgentRegistry's
/// canonical values (which are loaded from the Rust core); here they are
/// hardcoded as a fallback so the tray/limits/breakdown UI renders without the
/// Rust <c>tt_skills_list_agents</c> call.</summary>
public static class AgentRegistry
{
    public sealed record AgentInfo(string DisplayName, string BrandColorHex, string LogoFile);

    public const string FallbackColor = "#059669";

    /// <summary>The 26 user-visible agents: 15 canonical (with limits) + 11
    /// parser-only. <c>everycode</c> is folded into ChatGPT (<c>codex</c>) and is
    /// not listed separately; internal auxiliary sources are excluded.</summary>
    public static IReadOnlyList<string> UserFacingSources { get; } = new[]
    {
        // 15 canonical (limits) agents
        "claude", "codex", "cursor", "kiro", "copilot", "kimi", "antigravity",
        "zed", "trae", "windsurf", "qoder", "codebuddy", "workbuddy", "gemini", "zcode",
        // 11 parser-only agents
        "opencode", "openclaw", "hermes", "grok", "roocode", "kilocode", "kilocli",
        "goose", "ohmypi", "pi", "craft",
    };

    private static readonly IReadOnlyDictionary<string, AgentInfo> Agents =
        new Dictionary<string, AgentInfo>(StringComparer.OrdinalIgnoreCase)
        {
            ["claude"] = new("Claude Code", "#D97757", "claude-code"),
            ["codex"] = new("ChatGPT", "#10A37F", "codex"),
            ["everycode"] = new("ChatGPT", "#10A37F", "chatgpt"),
            ["cursor"] = new("Cursor", "#FFFFFF", "cursor"),
            ["gemini"] = new("Gemini", "#4285F4", "gemini"),
            ["kiro"] = new("Kiro", "#7C3AED", "kiro"),
            ["opencode"] = new("OpenCode", "#059669", "opencode"),
            ["openclaw"] = new("OpenClaw", "#F59E0B", "openclaw"),
            ["hermes"] = new("Hermes", "#8B5CF6", "hermes"),
            ["copilot"] = new("GitHub Copilot", "#8A4FFF", "copilot"),
            ["kimi"] = new("Kimi", "#10A37F", "kimi"),
            ["grok"] = new("Grok", "#FFFFFF", "grok"),
            ["antigravity"] = new("Antigravity", "#4285F4", "antigravity"),
            ["roocode"] = new("RooCode", "#10A37F", "roocode"),
            ["kilocode"] = new("KiloCode", "#F59E0B", "kilo"),
            ["kilocli"] = new("Kilo CLI", "#F59E0B", "kilo"),
            ["zed"] = new("Zed", "#FFFFFF", "zed"),
            ["goose"] = new("Goose", "#059669", "goose"),
            ["ohmypi"] = new("OhMyPi", "#F59E0B", "ohmypi"),
            ["pi"] = new("Pi", "#10A37F", "pi"),
            ["craft"] = new("Craft", "#059669", "craft"),
            ["codebuddy"] = new("CodeBuddy", "#4285F4", "codebuddy"),
            ["workbuddy"] = new("WorkBuddy", "#F59E0B", "workbuddy"),
            ["mimocode"] = new("MimoCode", "#059669", "mimo"),
            ["zcode"] = new("ZCode", "#10A37F", "zcode"),
            ["trae"] = new("Trae", "#10A37F", "trae"),
            ["windsurf"] = new("Windsurf", "#10A37F", "windsurf"),
            ["qoder"] = new("Qoder", "#10A37F", "qoder"),
        };

    public static string DisplayName(string source)
    {
        var key = source.Trim().ToLowerInvariant();
        return Agents.TryGetValue(key, out var info) ? info.DisplayName : PrettyName(key);
    }

    public static string BrandColorHex(string source)
    {
        var key = source.Trim().ToLowerInvariant();
        return Agents.TryGetValue(key, out var info) ? info.BrandColorHex : FallbackColor;
    }

    /// <summary>Logo filename (without extension) resolved against the embedded
    /// <c>brand-logos/*.png</c> resources.</summary>
    public static string LogoFile(string source)
    {
        var key = source.Trim().ToLowerInvariant();
        return Agents.TryGetValue(key, out var info) ? info.LogoFile : key;
    }

    private static string PrettyName(string source)
    {
        if (source == "codex") return "ChatGPT";
        return string.Join(" ", source.Split('-', '_')
            .Select(part => part.Length == 0 ? part : char.ToUpperInvariant(part[0]) + part[1..]));
    }
}
