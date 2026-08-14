using System.Globalization;
using TokenViewerWindows.Infrastructure;

namespace TokenViewerWindows.Services;

/// <summary>
/// In-code EN/ZH string catalog (mirrors macOS <c>L10n</c>). Exposes a
/// binding-friendly indexer plus type-safe format methods, and notifies
/// listeners when the language changes so already-open windows update
/// immediately. Named <c>L10n</c> (not <c>Localization</c>) to avoid colliding
/// with <c>System.Windows.Localization</c>.
/// </summary>
public sealed class L10n : ObservableObject
{
    public static L10n Instance { get; } = new();

    /// <summary>Static, non-Skills strings (key -> (English, Chinese)). Skills-only
    /// and Git-sync text is deferred to the Skills phase.</summary>
    public static IReadOnlyDictionary<string, (string En, string Zh)> Catalog { get; } = new Dictionary<string, (string En, string Zh)>
    {
        // Tabs
        ["usage"] = ("Usage", "用量"),
        ["limits"] = ("Limits", "限额"),
        ["settings"] = ("Settings", "设置"),

        // Usage view
        ["usageTitle"] = ("Usage", "用量"),
        ["usageSubtitle"] = ("Token consumption across all AI tools", "所有 AI 工具的 Token 消耗"),
        ["syncNow"] = ("Sync now", "立即同步"),
        ["rangeToday"] = ("Today", "今天"),
        ["rangeYesterday"] = ("Yesterday", "昨天"),
        ["rangeWeek"] = ("Week", "近 7 天"),
        ["rangeMonth"] = ("Month", "近 30 天"),
        ["rangeAll"] = ("All", "全部"),
        ["rangeCustom"] = ("Custom", "自定义"),
        ["rangeFrom"] = ("From", "起始"),
        ["rangeTo"] = ("To", "结束"),
        ["today"] = ("Today", "今天"),
        ["sevenDays"] = ("7 Days", "7 天"),
        ["thirtyDays"] = ("30 Days", "30 天"),
        ["total"] = ("Total", "总计"),
        ["active"] = ("active", "天活跃"),
        ["perDay"] = ("/day", "/天"),

        // Panel / sections
        ["trend"] = ("Trend", "趋势"),
        ["activity"] = ("Activity", "活跃度"),
        ["topModels"] = ("Top Models", "热门模型"),
        ["dashboard"] = ("Dashboard", "仪表盘"),
        ["quit"] = ("Quit", "退出"),

        // Trend chart
        ["usageTrend"] = ("Usage Trend", "用量趋势"),
        ["byDay"] = ("by day", "按天"),
        ["byHour"] = ("by hour", "按小时"),
        ["input"] = ("Input", "输入"),
        ["output"] = ("Output", "输出"),
        ["cacheRead"] = ("Cache Read", "缓存读取"),
        ["reasoning"] = ("Reasoning", "推理"),
        ["cost"] = ("Cost", "费用"),
        ["costByModel"] = ("Cost by Model", "模型费用明细"),
        ["cacheHit"] = ("Cache hit", "缓存命中"),

        // Limits
        ["limitsTitle"] = ("Limits", "限额"),
        ["limitsSubtitle"] = ("Per-agent quota windows with reset countdowns", "各工具配额窗口及重置倒计时"),
        ["limitsVisibilityDesc"] = ("Choose which agents appear in the menu-bar limits card.", "选择哪些 Agent 显示在菜单栏弹窗的限额卡片里。"),
        ["noUsageData"] = ("No usage data", "暂无数据"),
        ["noLimitsData"] = ("No limits data yet.", "当前没有限额数据。"),
        ["limitsNoDataDesc"] = ("Use any supported agent, then sync to view data. You can also open Settings to choose which agents appear.", "使用任意支持的 Agent 后，再点击同步查看。或者前往设置勾选要显示的 Agent。"),
        ["reset"] = ("Reset", "重置"),
        ["resets"] = ("Resets", "重置"),
        ["expires"] = ("Expires", "到期"),
        ["subscriptionReset"] = ("Subscription reset", "订阅重置"),
        ["quotaReset"] = ("Quota reset", "额度重置"),
        ["refreshingLimits"] = ("Refreshing limits…", "正在刷新限额…"),
        ["refreshLimits"] = ("Refresh limits", "刷新限额"),
        ["notConfigured"] = ("Not configured", "未配置"),
        ["openSettings"] = ("Open Settings", "打开设置"),
        ["heatmapLess"] = ("Less", "少"),
        ["heatmapMore"] = ("More", "多"),

        // Limits: stable error strings
        ["errRequestFailed"] = ("Request failed", "请求失败"),
        ["errNoUsageData"] = ("No usage data", "暂无用量数据"),
        ["errNotLoggedIn"] = ("Not logged in", "未登录"),
        ["errNoUserId"] = ("No userId", "无用户 ID"),
        ["errNoApiKey"] = ("No API key", "无 API 密钥"),
        ["errNoActiveQuota"] = ("No active quota", "无活跃配额"),
        ["errNoSubscriptionData"] = ("No subscription data", "无订阅数据"),
        ["errUsesGeminiQuota"] = ("Uses Gemini quota", "使用 Gemini 配额"),
        ["errError"] = ("Error", "错误"),

        // Limits: stable window labels
        ["window5Hour"] = ("5 Hour", "5 小时"),
        ["window7Day"] = ("7 Day", "7 天"),
        ["window7DayOpus"] = ("7 Day (Opus)", "7 天 (Opus)"),
        ["windowWeekly"] = ("Weekly", "每周"),
        ["windowWindow"] = ("Window", "窗口"),
        ["windowCredits"] = ("Credits", "额度"),
        ["windowPlan"] = ("Plan", "套餐"),
        ["windowChat"] = ("Chat", "对话"),
        ["windowPremium"] = ("Premium", "高级"),
        ["windowUsage"] = ("Usage", "用量"),
        ["windowQuota"] = ("Quota", "配额"),

        // Settings
        ["settingsTitle"] = ("Settings", "设置"),
        ["appearance"] = ("Appearance", "外观"),
        ["theme"] = ("Theme", "主题"),
        ["themeLight"] = ("Light", "浅色"),
        ["themeDark"] = ("Dark", "深色"),
        ["themeSystem"] = ("System", "跟随系统"),
        ["currency"] = ("Currency", "货币"),
        ["languageLabel"] = ("Language", "语言"),
        ["menuBarPanel"] = ("Menu Bar Panel", "菜单栏面板"),
        ["menuBarPanelDesc"] = ("Choose which sections appear in the menu-bar popover.", "选择菜单栏弹窗中显示的板块。"),
        ["menuBarLimitsCards"] = ("Menu Bar Popover Limits Cards", "菜单栏弹窗限额卡片"),
        ["summary"] = ("Summary", "摘要"),
        ["models"] = ("Models", "模型"),
        ["heatmap"] = ("Heatmap", "热力图"),
        ["general"] = ("General", "通用"),
        ["launchAtLogin"] = ("Launch at Login", "开机启动"),
        ["showDockIcon"] = ("Show Dock Icon", "显示 Dock 图标"),
        ["showDockIconDesc"] = ("When off, TokenViewer only shows in the menu bar and is hidden from the Dock.", "关闭后 TokenViewer 只在菜单栏显示，不再出现在 Dock 中。"),
        ["showMenuBarIcon"] = ("Show Menu Bar Icon", "显示菜单栏图标"),
        ["showMenuBarIconDesc"] = ("When off, TokenViewer's icon is hidden from the menu bar.", "关闭后 TokenViewer 不再出现在菜单栏中。"),
        ["showBothHiddenWarning"] = ("Dock and menu bar icons can't both be hidden — the other one was turned back on.", "Dock 图标和菜单栏图标不能同时关闭，已自动恢复另一个。"),
        ["syncFrequency"] = ("Sync Frequency", "同步频率"),
        ["manual"] = ("Manual", "手动"),
        ["updates"] = ("Updates", "更新"),
        ["softwareUpdate"] = ("Software Update", "软件更新"),
        ["upToDate"] = ("Up to date", "已是最新版本"),
        ["updateCheckFailed"] = ("Failed to check for updates, please try again later", "获取更新失败，稍后重试"),
        ["newVersion"] = ("New version available", "有新版本"),
        ["download"] = ("Download", "下载"),
        ["checkNow"] = ("Check Now", "检查更新"),
        ["lastChecked"] = ("Last checked", "上次检查"),
        ["checkingUpdates"] = ("Checking for updates…", "正在检查更新…"),
        ["releaseNotesTitle"] = ("Release Notes", "更新说明"),
        ["installUpdate"] = ("Install Update", "安装更新"),
        ["later"] = ("Later", "稍后"),
        ["updateAvailableMessage"] = ("Install the new version now?", "现在安装新版本？"),
        ["checkForUpdatesFailed"] = ("Could not check for updates", "无法检查更新"),
        ["couldNotOpenInstaller"] = ("Could not open installer", "无法打开安装器"),
        ["noReleaseNotesAvailable"] = ("No release notes available.", "暂无更新说明。"),
        ["github"] = ("GitHub", "GitHub"),
        ["engine"] = ("Engine", "引擎"),
        ["storage"] = ("Storage", "存储"),
        ["engineValue"] = ("tokenviewer-core (Rust)", "tokenviewer-core (Rust)"),
        ["storageValue"] = ("SQLite · local-only", "SQLite · 本地存储"),
        ["agents"] = ("Agents", "Agent"),
        ["noAgentData"] = ("No agent data yet. Use any supported AI agent, then Sync.", "尚无数据。使用任意 AI Agent 后点击同步。"),
        ["data"] = ("Data", "数据"),
        ["dataManagement"] = ("Data Management", "数据管理"),
        ["directory"] = ("Directory", "目录"),
        ["add"] = ("Add", "添加"),
        ["openInFinder"] = ("Open in Finder", "在 Finder 中打开"),
        ["codexHomesTitle"] = ("ChatGPT Data Directories", "ChatGPT 数据目录"),
        ["codexHomesDescription"] = ("Discover the default Codex home and isolated directories used by Orca, Antigravity Cockpit, and other hosts. Discovery never modifies these directories; add a path for hosts that cannot be detected automatically.", "自动发现默认 Codex、Orca、Antigravity Cockpit 等隔离目录。自动发现不会修改目录内容；手动添加可覆盖未识别的宿主应用。"),
        ["codexHomePlaceholder"] = ("Add an extra Codex Home path", "添加额外 Codex Home 路径"),
        ["codexHomesRescan"] = ("Rescan ChatGPT data directories", "重新扫描 ChatGPT 数据目录"),
        ["codexHomesEmpty"] = ("No ChatGPT data directories found", "未发现 ChatGPT 数据目录"),
        ["codexHomeSessions"] = ("Sessions", "会话"),
        ["codexHomeAuth"] = ("Account", "账户"),
        ["codexHomeMissing"] = ("Missing", "目录不存在"),
        ["rebuildData"] = ("Rebuild Data", "重建数据"),
        ["rebuildDataDesc"] = ("Clears processed data and sync cursors, then rescans raw source files.", "清理已处理的数据和同步游标，然后从原始数据重新拉取。"),
        ["rebuildDataHint"] = ("Use when data looks stale, missing, or sync cursors are out of date.", "当数据看起来缺失、过旧，或同步游标异常时使用。"),
        ["rebuildConfirm"] = ("Confirm Rebuild", "确认重建"),
        ["rebuildDone"] = ("Data rebuild complete. Refresh to view the latest data.", "数据重建完成，请稍后刷新查看。"),
        ["resetSettings"] = ("Reset Settings", "重置设置"),
        ["resetSettingsDesc"] = ("Restore theme, language, currency, sync frequency, menu bar, and agent display preferences. Data, Git config, and tokens are kept.", "恢复主题、语言、货币、同步频率、菜单栏和 Agent 显示偏好。不删除数据、Git 配置或令牌。"),
        ["resetSettingsConfirm"] = ("Reset Settings?", "确认重置设置？"),
        ["toastSettingsReset"] = ("Settings reset", "设置已重置"),
        ["resetSettingsConfirmMessage"] = ("This restores app preferences without deleting usage data, skills files, Git config, or Keychain tokens.", "这会恢复应用偏好设置，但不会删除用量数据、skills 文件、Git 配置或 Keychain 令牌。"),
        ["cancel"] = ("Cancel", "取消"),
        ["about"] = ("About", "关于"),
        ["aboutSupportedAgents"] = ("Supported Agents", "支持的 Agent"),
        ["aboutWithLimits"] = ("With Limits", "带限额订阅"),
        ["aboutWithoutLimits"] = ("Without Limits", "不带限额"),

        // Usage section headers
        ["usageDaily"] = ("Daily Usage", "每日用量"),
        ["usageModels"] = ("Models", "模型"),
        ["usageTokenBreakdown"] = ("Token Breakdown", "Token 分解"),
        ["usageAgents"] = ("Agents", "Agent"),
        ["usageActivity"] = ("Activity", "活跃度"),
        ["usageDailyDetails"] = ("Daily Details", "每日明细"),
        ["usageTotalTokens"] = ("Total Tokens", "总 Token 数"),
        ["usageConversations"] = ("Conversations", "对话数"),
        ["usageActiveDaysTitle"] = ("Active Days", "活跃天数"),
        ["usageColDate"] = ("Date", "日期"),
        ["usageColTotal"] = ("Total", "总计"),
        ["usageColInput"] = ("Input", "输入"),
        ["usageColOutput"] = ("Output", "输出"),
        ["usageColCache"] = ("Cache", "缓存"),
        ["usageColReason"] = ("Reason", "推理"),
        ["usageColConvs"] = ("Convs", "对话"),

        // Sync frequency (single value)
        ["sync5min"] = ("5 min", "5 min"),
        ["sync10min"] = ("10 min", "10 min"),
        ["sync15min"] = ("15 min", "15 min"),
        ["sync30min"] = ("30 min", "30 min"),
        ["sync1hour"] = ("1 hour", "1 hour"),

        // Agent settings (single value)
        ["pathLabel"] = ("Path", "Path"),
        ["linkLabel"] = ("Link", "Link"),
        ["linkDirectory"] = ("Directory", "Directory"),
        ["linkSingleFile"] = ("Single File", "Single File"),
        ["linkOverlay"] = ("Overlay", "Overlay"),

        // App / tray / status
        ["appName"] = ("Token Viewer", "Token Viewer"),
        ["loading"] = ("Loading…", "加载中…"),
        ["menuBarSectionTitle"] = ("Menu Bar", "菜单栏"),
        ["statusSyncing"] = ("Syncing…", "同步中…"),
        ["statusReady"] = ("Ready", "就绪"),
        ["statusSyncFailed"] = ("Sync failed", "同步失败"),
        ["initFailed"] = ("Failed to initialize the local database.", "本地数据库初始化失败。"),
    };

    private string _language = "system";

    public string Language
    {
        get => _language;
        set
        {
            if (SetProperty(ref _language, value))
            {
                RaisePropertyChanged(nameof(IsZh));
                RaisePropertyChanged("Item[]");
            }
        }
    }

    public bool IsZh =>
        _language == "zh" ||
        (_language == "system" && CultureInfo.CurrentUICulture.TwoLetterISOLanguageName.Equals("zh", StringComparison.OrdinalIgnoreCase));

    public string this[string key] =>
        Catalog.TryGetValue(key, out var pair) ? (IsZh ? pair.Zh : pair.En) : key;

    // MARK: - Limits format

    public string ExpiresInDays(int days) => IsZh ? $"{days} 天后到期" : $"Expires in {DayCount(days)}";
    public string QuotaResetsInDays(int days) => IsZh ? $"{days} 天后重置额度" : $"Quota resets in {DayCount(days)}";
    public string SubscriptionResetsInDays(int days) => IsZh ? $"{days} 天后重置订阅" : $"Subscription resets in {DayCount(days)}";
    public string ResetsInDays(int days) => IsZh ? $"{days} 天后重置" : $"Resets in {DayCount(days)}";
    public string ResetsIn(string value) => IsZh ? $"{value} 后重置" : $"Resets in {value}";

    // MARK: - Update format

    public string UpdateAvailableTitle(string version) => IsZh ? $"TokenViewer {version} 有可用更新" : $"TokenViewer {version} is available";
    public string DownloadingUpdate(string version) => IsZh ? $"正在下载 v{version}…" : $"Downloading v{version}…";
    public string UpdateAvailableStatus(string version) => IsZh ? $"v{version} 有新版本" : $"v{version} available";

    // MARK: - About / counts format

    public string CopyrightFooter(int year) => IsZh ? $"© {year} webkong. 保留所有权利。" : $"© {year} webkong. All rights reserved.";
    public string AboutAgentCount(int n) => IsZh ? $"{n} 个 AI 编程工具" : $"{n} AI coding tools";
    public string AboutLimitsCount(int n) => IsZh ? $"{n} 支持限额" : $"{n} with limits";
    public string AboutOtherCount(int n) => IsZh ? $"{n} 其他" : $"{n} other";
    public string RecordsCount(int n) => IsZh ? $"{n} 条记录" : $"{n} records";
    public string ActiveCount(int n) => IsZh ? $"23 个支持工具中 {n} 个活跃" : $"{n} of 23 supported tools active";
    public string UsageActiveDays(int n) => IsZh ? $"{n} 天活跃" : $"{n} active days";

    public string CodexHomeSource(string source) => source switch
    {
        "user_configured" => IsZh ? "手动添加" : "User",
        "environment" => "CODEX_HOME",
        "default" => IsZh ? "默认" : "Default",
        "known_host" => IsZh ? "已知应用" : "Known Host",
        "discovered" => IsZh ? "自动发现" : "Discovered",
        "cached" => IsZh ? "历史发现" : "Cached",
        _ => source,
    };

    /// <summary>Human countdown ("now" / "N minutes" / "N hours" / "N days").</summary>
    public string CountdownText(TimeSpan remaining)
    {
        var seconds = remaining.TotalSeconds;
        if (seconds <= 0) return IsZh ? "现在" : "now";
        if (seconds < 3600)
        {
            var minutes = Math.Max(1, (int)Math.Ceiling(seconds / 60));
            return IsZh ? $"{minutes} 分钟" : (minutes == 1 ? "1 minute" : $"{minutes} minutes");
        }
        if (seconds < 172800)
        {
            var hours = Math.Max(1, (int)Math.Ceiling(seconds / 3600));
            return IsZh ? $"{hours} 小时" : (hours == 1 ? "1 hour" : $"{hours} hours");
        }
        return DayCount(Math.Max(1, (int)Math.Ceiling(seconds / 86400)));
    }

    private static string DayCount(int days) => days == 1 ? "1 day" : $"{days} days";
}
