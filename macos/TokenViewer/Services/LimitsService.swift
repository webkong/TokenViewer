import Foundation

// MARK: - Models

struct LimitWindow: Identifiable, Codable {
    var id: String { label }
    let label: String
    let usedPercent: Double   // 0-100
    let resetAt: Date?
}

struct ProviderLimit: Identifiable, Codable {
    var id: String { name }
    let name: String          // "claude", "codex", ...
    let planLabel: String?
    let configured: Bool
    let error: String?
    var subscriptionExpiresAt: Date? = nil
    var subscriptionResetAt: Date? = nil
    var quotaResetAt: Date? = nil
    let windows: [LimitWindow]
}

extension ProviderLimit {
    var hasLimitDisplay: Bool {
        !windows.isEmpty || subscriptionExpiresAt != nil || subscriptionResetAt != nil || quotaResetAt != nil
    }

    var nextResetAt: Date? {
        let dates = windows.compactMap(\.resetAt)
        let now = Date()
        return dates.filter { $0 >= now }.min() ?? dates.max()
    }
}

// MARK: - Service

/// Fetches live rate-limit / quota info per provider. Network + Keychain + process
/// based — runs entirely client-side using the user's own local credentials.
enum LimitsService {
    static func fetchAll() async -> [ProviderLimit] {
        async let codex = fetchCodex()
        async let copilot = fetchCopilot()
        async let claude = fetchClaude()
        async let kiro = fetchKiro()
        async let cursor = fetchCursor()
        async let gemini = fetchGemini()
        async let kimi = fetchKimi()
        async let antigravity = fetchAntigravity()
        async let zed = fetchZed()
        async let trae = fetchTrae()
        async let windsurf = fetchWindsurf()
        async let qoder = fetchQoder()
        async let codebuddy = fetchCodebuddy()
        async let workbuddy = fetchWorkBuddy()
        async let zcode = fetchZcode()
        return await [claude, codex, copilot, kiro, cursor, gemini, kimi, antigravity, zed, trae, windsurf, qoder, codebuddy, workbuddy, zcode]
    }

    // MARK: Claude (Keychain → Anthropic OAuth usage API)

    static func fetchClaude() async -> ProviderLimit {
        let name = "claude"
        guard let token = claudeAccessToken() else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }
        var req = URLRequest(url: URL(string: "https://api.anthropic.com/api/oauth/usage")!)
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("oauth-2025-04-20", forHTTPHeaderField: "anthropic-beta")
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        guard let json = await getJSON(req) else {
            return ProviderLimit(name: name, planLabel: planLabel(claudeSubscription(), "Claude"), configured: true, error: "Request failed", windows: [])
        }
        var windows: [LimitWindow] = []
        for (key, label) in [("five_hour", "5 Hour"), ("seven_day", "7 Day"), ("seven_day_opus", "7 Day (Opus)")] {
            if let w = json[key] as? [String: Any] {
                let util = (w["utilization"] as? Double) ?? Double(w["utilization"] as? Int ?? 0)
                windows.append(LimitWindow(label: label, usedPercent: util, resetAt: parseDate(w["resets_at"])))
            }
        }
        return ProviderLimit(name: name, planLabel: planLabel(claudeSubscription(), "Claude"), configured: true, error: nil, windows: windows)
    }

    // MARK: Codex (auth.json → ChatGPT wham API, with refresh)

    static func fetchCodex() async -> ProviderLimit {
        let name = "codex"
        let home = NSHomeDirectory()
        let authPath = ProcessInfo.processInfo.environment["CODEX_HOME"].map { "\($0)/auth.json" } ?? "\(home)/.codex/auth.json"
        guard let auth = readJSON(authPath),
              let tokens = auth["tokens"] as? [String: Any],
              let accessToken = tokens["access_token"] as? String else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }
        let idToken = tokens["id_token"] as? String
        let plan = planLabel(jwtClaim(accessToken, "chatgpt_plan_type") ?? idToken.flatMap { jwtClaim($0, "chatgpt_plan_type") }, "Codex")
        let accountId = tokens["account_id"] as? String ?? jwtClaim(accessToken, "chatgpt_account_id")
        let subscriptionExpiresAt = await codexSubscriptionExpiresAt(
            accessToken: accessToken,
            idToken: idToken,
            accountId: accountId
        )

        var req = URLRequest(url: URL(string: "https://chatgpt.com/backend-api/wham/usage")!)
        req.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        if let acc = accountId { req.setValue(acc, forHTTPHeaderField: "ChatGPT-Account-Id") }

        guard let json = await getJSON(req), json["rate_limit"] is [String: Any] else {
            return ProviderLimit(name: name, planLabel: plan, configured: true, error: "Request failed", subscriptionExpiresAt: subscriptionExpiresAt, windows: [])
        }
        let windows = codexLimitWindows(from: json)
        return ProviderLimit(name: name, planLabel: plan, configured: true, error: nil, subscriptionExpiresAt: subscriptionExpiresAt, windows: windows)
    }

    static func codexLimitWindows(from json: [String: Any]) -> [LimitWindow] {
        guard let rl = codexRateLimitPayload(from: json) else { return [] }
        var windows = codexStandardWindows(from: rl)
        windows.append(contentsOf: codexSparkWindows(from: json["additional_rate_limits"]))
        return windows
    }

    private enum CodexRateWindowKind: Equatable {
        case session
        case weekly
    }

    private static func codexRateLimitPayload(from json: [String: Any]) -> [String: Any]? {
        (json["rate_limit"] as? [String: Any])
            ?? (json["rateLimits"] as? [String: Any])
    }

    private static func codexStandardWindows(from rateLimit: [String: Any]) -> [LimitWindow] {
        [
            ("primary_window", "primary", CodexRateWindowKind.session),
            ("secondary_window", "secondary", CodexRateWindowKind.weekly),
        ].compactMap { snakeKey, camelKey, fallbackKind in
            guard let window = (rateLimit[snakeKey] as? [String: Any]) ?? (rateLimit[camelKey] as? [String: Any]) else { return nil }
            return codexWindow(
                window,
                label: codexWindowLabel(for: codexWindowKind(window) ?? fallbackKind, fallback: "Window")
            )
        }
    }

    private static func codexSparkWindows(from additionalRateLimits: Any?) -> [LimitWindow] {
        guard let entries = additionalRateLimits as? [[String: Any]] else { return [] }
        var classified: [(CodexRateWindowKind, [String: Any])] = []
        var fallbacks: [(CodexRateWindowKind, [String: Any])] = []

        for entry in entries where codexIsSparkLimit(entry) {
            guard let rateLimit = entry["rate_limit"] as? [String: Any] else { continue }
            let primary = rateLimit["primary_window"] as? [String: Any]
            let secondary = rateLimit["secondary_window"] as? [String: Any]
            for window in [primary, secondary].compactMap({ $0 }) {
                if let kind = codexWindowKind(window) {
                    classified.append((kind, window))
                }
            }
            fallbacks.append(contentsOf: codexSparkFallbackCandidates(primary: primary, secondary: secondary))
        }

        var session: [String: Any]?
        var weekly: [String: Any]?
        for (kind, window) in classified + fallbacks {
            switch kind {
            case .session where session == nil:
                session = window
            case .weekly where weekly == nil:
                weekly = window
            default:
                break
            }
        }

        return [
            session.flatMap { codexWindow($0, label: "Spark 5h") },
            weekly.flatMap { codexWindow($0, label: "Spark 7d") },
        ].compactMap { $0 }
    }

    private static func codexIsSparkLimit(_ entry: [String: Any]) -> Bool {
        [entry["limit_name"], entry["metered_feature"]].contains { value in
            guard let raw = value as? String else { return false }
            return raw.trimmingCharacters(in: .whitespacesAndNewlines).lowercased().contains("spark")
        }
    }

    private static func codexSparkFallbackCandidates(primary: [String: Any]?, secondary: [String: Any]?) -> [(CodexRateWindowKind, [String: Any])] {
        let primaryKind = primary.flatMap(codexWindowKind)
        let secondaryKind = secondary.flatMap(codexWindowKind)
        let primaryDurationMissing = primary?["limit_window_seconds"] == nil

        if primaryKind != nil || secondaryKind != nil {
            var out: [(CodexRateWindowKind, [String: Any])] = []
            if primaryKind == nil, let primary, secondaryKind == .weekly {
                out.append((.session, primary))
            }
            if primaryKind == nil, primaryDurationMissing, let primary, secondaryKind == .session {
                out.append((.weekly, primary))
            }
            if secondaryKind == nil, let secondary, primaryKind == .weekly {
                out.append((.session, secondary))
            }
            if secondaryKind == nil, let secondary, primaryKind == .session {
                out.append((.weekly, secondary))
            }
            return out
        }

        var out: [(CodexRateWindowKind, [String: Any])] = []
        if let primary { out.append((.session, primary)) }
        if let secondary { out.append((.weekly, secondary)) }
        return out
    }

    private static func codexWindow(_ window: [String: Any], label: String) -> LimitWindow? {
        guard let usedPercent = codexUsedPercent(window) else { return nil }
        return LimitWindow(label: label, usedPercent: usedPercent, resetAt: codexResetDate(window))
    }

    private static func codexUsedPercent(_ window: [String: Any]) -> Double? {
        numeric(window["used_percent"] ?? window["usedPercent"]).map { min(max($0.rounded(), 0), 100) }
    }

    private static func codexWindowKind(_ window: [String: Any]) -> CodexRateWindowKind? {
        guard let seconds = numeric(window["limit_window_seconds"] ?? window["limitWindowSeconds"]) else { return nil }
        if seconds == 18_000 { return .session }
        if seconds == 604_800 { return .weekly }
        return nil
    }

    private static func codexResetDate(_ window: [String: Any]) -> Date? {
        parseDate(window["reset_at"] ?? window["resets_at"] ?? window["resetsAt"] ?? window["resetAt"])
    }

    private static func codexWindowLabel(for kind: CodexRateWindowKind?, fallback: String) -> String {
        switch kind {
        case .session:
            return "5 Hour"
        case .weekly:
            return "Weekly"
        case nil:
            return fallback
        }
    }

    // MARK: Copilot (apps.json → GitHub API)

    static func fetchCopilot() async -> ProviderLimit {
        let name = "copilot"
        guard let token = copilotToken() else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }
        var req = URLRequest(url: URL(string: "https://api.github.com/copilot_internal/user")!)
        req.setValue("token \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        req.setValue("vscode/1.96.2", forHTTPHeaderField: "Editor-Version")
        req.setValue("copilot-chat/0.26.7", forHTTPHeaderField: "Editor-Plugin-Version")
        req.setValue("GitHubCopilotChat/0.26.7", forHTTPHeaderField: "User-Agent")
        req.setValue("2025-04-01", forHTTPHeaderField: "X-Github-Api-Version")

        guard let json = await getJSON(req) else {
            return ProviderLimit(name: name, planLabel: nil, configured: true, error: "Request failed", windows: [])
        }
        let plan = planLabel(json["copilot_plan"] as? String, "Copilot")
        let reset = parseDate(json["quota_reset_date"])
        var windows: [LimitWindow] = []
        if let snaps = json["quota_snapshots"] as? [String: Any] {
            for (key, label) in [("premium_interactions", "Premium"), ("chat", "Chat")] {
                guard let q = snaps[key] as? [String: Any] else { continue }
                let used: Double
                if let pr = q["percent_remaining"] as? Double { used = 100 - pr }
                else if let pr = q["percent_remaining"] as? Int { used = 100 - Double(pr) }
                else if let ent = numeric(q["entitlement"]), let rem = numeric(q["remaining"]), ent > 0 { used = (ent - rem) / ent * 100 }
                else { continue }
                windows.append(LimitWindow(label: label, usedPercent: used, resetAt: reset))
            }
        }
        return ProviderLimit(name: name, planLabel: plan, configured: true, error: nil, quotaResetAt: reset, windows: windows)
    }

    // MARK: Kiro (kiro-cli /usage)

    static func fetchKiro() async -> ProviderLimit {
        let name = "kiro"
        guard let out = runKiroUsage() else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }
        let lower = out.lowercased()
        if lower.contains("not logged in") || lower.contains("login required") || lower.contains("kiro-cli login") {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: "Not logged in", windows: [])
        }
        // Strip ANSI escape codes (ESC [ ... m) before parsing
        let esc = "\u{1B}"
        let clean = out.replacingOccurrences(of: "\(esc)\\[[0-9;]*[a-zA-Z]", with: "", options: .regularExpression)
        // "KIRO PRO+\" / "KIRO POWER" / "KIRO PRO" / "KIRO FREE" — match plan including '+'
        let plan = planLabel(firstMatch(clean, #"\|\s*(KIRO\s+[\w\+]+)"#) ?? firstMatch(clean, #"Plan:\s*(.+)"#), "Kiro")
        let resetAt = kiroResetDate(clean)
        var windows: [LimitWindow] = []
        // "1850.54 of 2000 covered" or "1850 of 2000 covered"
        if let used = firstMatch(clean, #"(\d+(?:\.\d+)?)\s+of\s+(\d+(?:\.\d+)?)\s+covered"#, group: 1).flatMap(Double.init),
           let total = firstMatch(clean, #"(\d+(?:\.\d+)?)\s+of\s+(\d+(?:\.\d+)?)\s+covered"#, group: 2).flatMap(Double.init), total > 0 {
            windows.append(LimitWindow(label: "Credits", usedPercent: used / total * 100, resetAt: resetAt))
        } else if let pct = firstMatch(clean, #"█+\s*(\d+)%"#).flatMap({ Double($0) }) {
            windows.append(LimitWindow(label: "Credits", usedPercent: pct, resetAt: resetAt))
        }
        return ProviderLimit(name: name, planLabel: plan, configured: !windows.isEmpty, error: windows.isEmpty ? "No usage data" : nil, quotaResetAt: resetAt, windows: windows)
    }

    // MARK: Cursor (state.vscdb SQLite → cursor.com/api/usage-summary)

    static func fetchCursor() async -> ProviderLimit {
        let name = "cursor"
        #if os(macOS)
        let stateDb = "\(NSHomeDirectory())/Library/Application Support/Cursor/User/globalStorage/state.vscdb"
        let cliCfg  = "\(NSHomeDirectory())/.cursor/cli-config.json"
        #else
        let stateDb = "\(NSHomeDirectory())/.config/Cursor/User/globalStorage/state.vscdb"
        let cliCfg  = "\(NSHomeDirectory())/.cursor/cli-config.json"
        #endif
        guard FileManager.default.fileExists(atPath: stateDb) else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }
        guard let jwt = readSqliteValue(stateDb, sql: "SELECT value FROM ItemTable WHERE key='cursorAuth/accessToken' LIMIT 1"),
              jwt.count > 10 else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }
        let userId: String
        if let cfg = readJSON(cliCfg), let authId = (cfg["authInfo"] as? [String: Any])?["authId"] as? String, !authId.isEmpty {
            userId = authId
        } else {
            userId = jwtClaim(jwt, "sub") ?? ""
        }
        guard !userId.isEmpty else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: "No userId", windows: [])
        }
        let cookie = "WorkosCursorSessionToken=\(userId)%3A%3A\(jwt)"
        var req = URLRequest(url: URL(string: "https://cursor.com/api/usage-summary")!)
        req.setValue(cookie, forHTTPHeaderField: "Cookie")
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        req.setValue("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36", forHTTPHeaderField: "User-Agent")
        req.setValue("https://www.cursor.com/settings", forHTTPHeaderField: "Referer")
        guard let json = await getJSON(req) else {
            return ProviderLimit(name: name, planLabel: nil, configured: true, error: "Request failed", windows: [])
        }
        let membership = json["membershipType"] as? String
        let plan = planLabel(membership, "Cursor")
        let billing = (json["billingCycleEnd"] as? String)
        let ind = json["individualUsage"] as? [String: Any]
        let planData = ind?["plan"] as? [String: Any]
        var pct: Double? = (planData?["totalPercentUsed"] as? Double)
            ?? (planData?["autoPercentUsed"] as? Double)
        if pct == nil, let used = numeric(planData?["used"]), let lim = numeric(planData?["limit"]), lim > 0 {
            pct = used / lim * 100
        }
        var windows: [LimitWindow] = []
        if let p = pct { windows.append(LimitWindow(label: "Plan", usedPercent: p, resetAt: parseDate(billing))) }
        let resetAt = parseDate(billing)
        return ProviderLimit(name: name, planLabel: plan, configured: true, error: windows.isEmpty ? "No usage data" : nil, quotaResetAt: resetAt, windows: windows)
    }

    // MARK: Gemini (oauth_creds.json → cloudcode-pa.googleapis.com)

    static func fetchGemini() async -> ProviderLimit {
        let name = "gemini"
        let credsPath = "\(NSHomeDirectory())/.gemini/oauth_creds.json"
        guard let creds = readJSON(credsPath), let accessToken = creds["access_token"] as? String, !accessToken.isEmpty else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }
        var req = URLRequest(url: URL(string: "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota")!)
        req.httpMethod = "POST"
        req.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try? JSONSerialization.data(withJSONObject: [:])
        guard let json = await getJSON(req),
              let buckets = json["buckets"] as? [[String: Any]] else {
            return ProviderLimit(name: name, planLabel: nil, configured: true, error: "Request failed", windows: [])
        }
        var lowestFrac: Double = 1.0
        var resetAt: Date? = nil
        for bucket in buckets {
            if let frac = bucket["remainingFraction"] as? Double { lowestFrac = min(lowestFrac, frac) }
            if resetAt == nil { resetAt = parseDate(bucket["resetTime"]) }
        }
        let used = (1.0 - lowestFrac) * 100
        let windows = buckets.isEmpty ? [] : [LimitWindow(label: "Quota", usedPercent: used, resetAt: resetAt)]
        return ProviderLimit(name: name, planLabel: nil, configured: true, error: nil, quotaResetAt: resetAt, windows: windows)
    }

    // MARK: Kimi (~/.kimi/credentials/kimi-code.json → api.kimi.com)

    static func fetchKimi() async -> ProviderLimit {
        let name = "kimi"
        let kimiHome = ProcessInfo.processInfo.environment["KIMI_HOME"] ?? "\(NSHomeDirectory())/.kimi"
        let credsPath = "\(kimiHome)/credentials/kimi-code.json"
        guard let creds = readJSON(credsPath), let accessToken = creds["access_token"] as? String, !accessToken.isEmpty else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }
        var req = URLRequest(url: URL(string: "https://api.kimi.com/coding/v1/usages")!)
        req.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        guard let json = await getJSON(req) else {
            return ProviderLimit(name: name, planLabel: nil, configured: true, error: "Request failed", windows: [])
        }
        var windows: [LimitWindow] = []
        // Plan: body.subType or body.user.membership.level
        let subType = json["subType"] as? String
            ?? (json["user"] as? [String: Any]).flatMap { ($0["membership"] as? [String: Any])?["level"] as? String }
        let plan = planLabel(subType, "Kimi")
        if let usage = json["usage"] as? [String: Any] {
            let limit = numeric(usage["limit"]) ?? 0
            let used = numeric(usage["used"]) ?? 0
            let pct = limit > 0 ? used / limit * 100 : 0
            windows.append(LimitWindow(label: "Usage", usedPercent: pct, resetAt: parseDate(usage["resetTime"] ?? usage["reset_at"])))
        }
        return ProviderLimit(name: name, planLabel: plan, configured: true, error: windows.isEmpty ? "No usage data" : nil, windows: windows)
    }

    // MARK: Antigravity (Gemini IDE extension — check install via data dir)

    static func fetchAntigravity() async -> ProviderLimit {
        let name = "antigravity"
        // Antigravity writes to ~/.gemini/antigravity* directories
        let geminiDir = "\(NSHomeDirectory())/.gemini"
        let hasAntigravity = ["antigravity", "antigravity-ide", "antigravity-cli"].contains {
            FileManager.default.fileExists(atPath: "\(geminiDir)/\($0)")
        }
        guard hasAntigravity else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }
        // Antigravity quota is shared with Gemini — no separate API available
        return ProviderLimit(name: name, planLabel: nil, configured: true, error: "Uses Gemini quota", windows: [])
    }

    // MARK: Zed / Trae (cockpit-tools account cache)

    static func fetchZed() async -> ProviderLimit {
        let name = "zed"
        guard let account = readCockpitAccount(provider: name) else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }
        let expiry = nestedDate(account, paths: [
            ["billing_period_end_at"],
            ["public_account", "billing_period_end_at"],
            ["subscription_raw", "subscription", "period", "end_at"],
            ["subscription_raw", "period", "end_at"],
            ["public_account", "subscription_raw", "subscription", "period", "end_at"],
            ["public_account", "subscription_raw", "period", "end_at"],
            ["user_raw", "plan", "subscription_period", "ended_at"],
            ["public_account", "user_raw", "plan", "subscription_period", "ended_at"],
        ])
        let plan = planLabel(nestedString(account, paths: [
            ["plan_raw"],
            ["public_account", "plan_raw"],
            ["subscription_raw", "subscription", "name"],
            ["subscription_raw", "name"],
            ["public_account", "subscription_raw", "subscription", "name"],
            ["public_account", "subscription_raw", "name"],
        ]), "Zed")
        return ProviderLimit(name: name, planLabel: plan, configured: true, error: expiry == nil ? "No subscription data" : nil, subscriptionExpiresAt: expiry, windows: [])
    }

    static func fetchTrae() async -> ProviderLimit {
        let name = "trae"
        guard let account = readCockpitAccount(provider: name) else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }
        let resetAt = nestedDate(account, paths: [
            ["plan_reset_at"],
            ["public_account", "plan_reset_at"],
            ["trae_entitlement_raw", "detail", "subscription_renew_time"],
            ["trae_entitlement_raw", "detail", "subscriptionRenewTime"],
            ["trae_entitlement_raw", "data", "detail", "subscription_renew_time"],
            ["trae_entitlement_raw", "data", "detail", "subscriptionRenewTime"],
            ["trae_entitlement_raw", "entitlementInfo", "detail", "subscription_renew_time"],
            ["trae_entitlement_raw", "entitlementInfo", "detail", "subscriptionRenewTime"],
            ["public_account", "trae_entitlement_raw", "detail", "subscription_renew_time"],
            ["public_account", "trae_entitlement_raw", "detail", "subscriptionRenewTime"],
        ])
        let plan = planLabel(nestedString(account, paths: [
            ["plan_type"],
            ["public_account", "plan_type"],
            ["trae_entitlement_raw", "plan_type"],
            ["trae_entitlement_raw", "data", "plan_type"],
            ["public_account", "trae_entitlement_raw", "plan_type"],
        ]), "Trae")
        return ProviderLimit(name: name, planLabel: plan, configured: true, error: resetAt == nil ? "No subscription data" : nil, subscriptionResetAt: resetAt, windows: [])
    }

    static func fetchWindsurf() async -> ProviderLimit {
        let name = "windsurf"
        guard let account = readCockpitAccount(provider: name) else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }
        let resetAt = nestedDate(account, paths: [
            ["copilot_quota_reset_date"],
            ["copilot_limited_user_reset_date"],
            ["public_account", "copilot_quota_reset_date"],
            ["public_account", "copilot_limited_user_reset_date"],
        ])
        let plan = planLabel(nestedString(account, paths: [
            ["copilot_plan"],
            ["public_account", "copilot_plan"],
            ["windsurf_plan_status", "plan"],
            ["windsurf_plan_status", "planName"],
            ["windsurf_user_status", "plan"],
        ]), "Windsurf")
        let windows = copilotStyleWindows(from: account, resetAt: resetAt)
        return ProviderLimit(name: name, planLabel: plan, configured: true, error: windows.isEmpty ? "No usage data" : nil, quotaResetAt: resetAt, windows: windows)
    }

    static func fetchQoder() async -> ProviderLimit {
        let name = "qoder"
        guard let account = readCockpitAccount(provider: name) else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }
        let plan = planLabel(nestedString(account, paths: [
            ["plan_type"],
            ["public_account", "plan_type"],
            ["auth_user_plan_raw", "plan_type"],
            ["auth_user_plan_raw", "planType"],
        ]), "Qoder")
        var windows: [LimitWindow] = []
        if let pct = numeric(nestedValue(account, ["credits_usage_percent"]))
            ?? numeric(nestedValue(account, ["public_account", "credits_usage_percent"])) {
            windows.append(LimitWindow(label: "Credits", usedPercent: min(max(pct, 0), 100), resetAt: nil))
        } else if let used = numeric(nestedValue(account, ["credits_used"])),
                  let total = numeric(nestedValue(account, ["credits_total"])), total > 0 {
            windows.append(LimitWindow(label: "Credits", usedPercent: min(max(used / total * 100, 0), 100), resetAt: nil))
        }
        return ProviderLimit(name: name, planLabel: plan, configured: true, error: windows.isEmpty ? "No usage data" : nil, windows: windows)
    }

    static func fetchCodebuddy() async -> ProviderLimit {
        let name = "codebuddy"
        let cached = readCockpitAccount(provider: name)
        guard let auth = readCodebuddyAuth() else {
            guard let cached else {
                return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
            }
            return workbuddyLimit(from: cached, configured: true, providerName: name, displayPrefix: "CodeBuddy")
        }

        if let refreshed = await refreshWorkBuddyAccount(from: auth) {
            writeCockpitAccountSnapshot(provider: name, account: refreshed)
            return workbuddyLimit(from: refreshed, configured: true, providerName: name, displayPrefix: "CodeBuddy")
        }

        if let cached {
            return workbuddyLimit(from: cached, configured: true, providerName: name, displayPrefix: "CodeBuddy")
        }
        return ProviderLimit(name: name, planLabel: "CodeBuddy", configured: true, error: "Request failed", windows: [])
    }

    static func fetchWorkBuddy() async -> ProviderLimit {
        let name = "workbuddy"
        let cached = readCockpitAccount(provider: name)
        guard let auth = readWorkBuddyAuth() else {
            guard let cached else {
                return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
            }
            return workbuddyLimit(from: cached, configured: true)
        }

        if let refreshed = await refreshWorkBuddyAccount(from: auth) {
            writeCockpitAccountSnapshot(provider: name, account: refreshed)
            return workbuddyLimit(from: refreshed, configured: true)
        }

        if let cached {
            return workbuddyLimit(from: cached, configured: true)
        }
        return ProviderLimit(name: name, planLabel: "WorkBuddy", configured: true, error: "Request failed", windows: [])
    }

    // MARK: ZCode (智谱 / Z.ai coding agent — local config detection)

    /// ZCode (智谱 / Z.ai coding agent) — queries the ZCode plan billing API
    /// for live token quota + reset countdown.
    ///
    /// Auth: the active provider's `apiKey` in `~/.zcode/v2/config.json` is a
    /// plaintext JWT that serves as the Bearer token for
    /// `https://zcode.z.ai/api/v1/zcode-plan/billing/balance`.
    ///
    /// Response shape (per entitlement/model):
    ///   { total_units, used_units, remaining_units, period_end (epoch secs) }
    ///
    /// Each balance entry becomes a `LimitWindow` (label = model show_name,
    /// usedPercent = used/total, resetAt = period_end). When the plan has
    /// expired or no balances are returned, the card falls back to showing the
    /// plan name without progress bars.
    static func fetchZcode() async -> ProviderLimit {
        let name = "zcode"
        let home = NSHomeDirectory()
        let zcodeDir = "\(home)/.zcode"
        guard FileManager.default.fileExists(atPath: zcodeDir) else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }

        // Read the active provider's JWT apiKey from config.json.
        guard let cfg = readJSON("\(zcodeDir)/v2/config.json"),
              let providers = cfg["provider"] as? [String: Any] else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }

        let bestProvider = providers
            .compactMap { (id, value) -> (String, [String: Any])? in
                guard let obj = value as? [String: Any] else { return nil }
                return (id, obj)
            }
            .sorted { a, b in zcodeProviderScore(a.0, a.1) > zcodeProviderScore(b.0, b.1) }
            .first

        guard let (providerId, providerObj) = bestProvider else {
            return ProviderLimit(name: name, planLabel: nil, configured: false, error: nil, windows: [])
        }

        let plan = zcodePlanLabel(for: providerId, fallback: providerObj["name"] as? String)
        let hasCliDb = FileManager.default.fileExists(atPath: "\(zcodeDir)/cli/db/db.sqlite")
        let configured = plan != nil || hasCliDb

        guard let options = providerObj["options"] as? [String: Any],
              let token = options["apiKey"] as? String,
              !token.isEmpty else {
            return ProviderLimit(name: name, planLabel: plan, configured: configured, error: configured ? "No API key" : nil, windows: [])
        }

        // Query billing/balance.
        var req = URLRequest(url: URL(string: "https://zcode.z.ai/api/v1/zcode-plan/billing/balance?app_version=3.1.1")!)
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Accept")

        guard let json = await getJSON(req) else {
            return ProviderLimit(name: name, planLabel: plan, configured: true, error: "Request failed", windows: [])
        }

        // Response: { code: 0, data: { balances: [...] } }
        let dataObj = json["data"] as? [String: Any] ?? json
        let balances = (dataObj["balances"] as? [[String: Any]])
            ?? (json["balances"] as? [[String: Any]])
            ?? []

        var windows: [LimitWindow] = []
        for b in balances {
            let label = (b["show_name"] as? String) ?? "Usage"
            let used = numeric(b["used_units"]) ?? 0
            let total = numeric(b["total_units"]) ?? 0
            let pct = total > 0 ? (used / total * 100) : 0
            let resetAt = parseDate(b["period_end"])
            windows.append(LimitWindow(label: label, usedPercent: pct, resetAt: resetAt))
        }

        // If no balances returned (plan expired / not entitled), still show the
        // card with plan label so the user knows ZCode is installed.
        if windows.isEmpty {
            return ProviderLimit(name: name, planLabel: plan, configured: true, error: "No active quota", windows: [])
        }

        return ProviderLimit(name: name, planLabel: plan, configured: true, error: nil, windows: windows)
    }

    private static func zcodeProviderScore(_ id: String, _ obj: [String: Any]) -> Int {
        let enabled = (obj["enabled"] as? Bool) ?? false
        let hasKey = !((obj["options"] as? [String: Any])?["apiKey"] as? String ?? "").isEmpty
        let idL = id.lowercased()
        var s = 0
        if enabled { s += 4 }
        if hasKey { s += 2 }
        if idL.contains("start-plan") { s += 3 }
        else if idL.contains("coding-plan") { s += 2 }
        else if idL.contains("bigmodel") || idL.contains("zai") { s += 1 }
        return s
    }

    /// Map a ZCode provider id (e.g. `builtin:bigmodel-start-plan`) to a short,
    /// user-facing plan label.
    private static func zcodePlanLabel(for providerId: String, fallback: String?) -> String? {
        let id = providerId.lowercased()
        if id.contains("start-plan") { return "Start Plan" }
        if id.contains("coding-plan") { return "Coding Plan" }
        if id.contains("bigmodel") { return "BigModel API" }
        if id.contains("zai") { return "Z.ai API" }
        return planLabel(fallback, "ZCode")
    }

    // MARK: Codex subscription expiry

    private static func codexSubscriptionExpiresAt(accessToken: String, idToken: String?, accountId: String?) async -> Date? {
        if let date = codexJwtSubscriptionExpiresAt(accessToken: accessToken, idToken: idToken) { return date }

        let accountCheck = await fetchCodexAccountCheckSubscription(accessToken: accessToken, accountId: accountId)
        if let date = futureDate(accountCheck.expiresAt) { return date }

        let resolvedAccountId = accountCheck.accountId ?? accountId ?? jwtClaim(accessToken, "chatgpt_account_id")
        guard let resolvedAccountId,
              let subscription = await fetchCodexSubscription(accessToken: accessToken, accountId: resolvedAccountId) else {
            return futureDate(accountCheck.expiresAt)
        }
        return futureDate(subscription)
    }

    private static func codexJwtSubscriptionExpiresAt(accessToken: String, idToken: String?) -> Date? {
        [idToken, accessToken].compactMap { $0 }.compactMap { token in
            let start = jwtClaimDate(token, "chatgpt_subscription_active_start")
            let until = jwtClaimDate(token, "chatgpt_subscription_active_until")
            return projectedSubscriptionEnd(start: start, until: until)
        }.first
    }

    private static func projectedSubscriptionEnd(start: Date?, until: Date?) -> Date? {
        guard let until else { return nil }
        let now = Date()
        if until > now { return until }

        guard let start else { return nil }
        let interval = until.timeIntervalSince(start)
        guard interval > 0 else { return nil }

        let day: TimeInterval = 86_400
        var candidate = until
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 0) ?? .current

        if interval >= 25 * day, interval <= 35 * day {
            while candidate <= now {
                candidate = calendar.date(byAdding: .month, value: 1, to: candidate) ?? candidate.addingTimeInterval(interval)
            }
            return candidate
        }

        if interval >= 350 * day, interval <= 380 * day {
            while candidate <= now {
                candidate = calendar.date(byAdding: .year, value: 1, to: candidate) ?? candidate.addingTimeInterval(interval)
            }
            return candidate
        }

        let periods = ceil(now.timeIntervalSince(until) / interval)
        return until.addingTimeInterval(max(1, periods) * interval)
    }

    private static func futureDate(_ date: Date?) -> Date? {
        guard let date, date > Date() else { return nil }
        return date
    }

    private static func fetchCodexAccountCheckSubscription(accessToken: String, accountId: String?) async -> (accountId: String?, expiresAt: Date?) {
        var comps = URLComponents(string: "https://chatgpt.com/backend-api/accounts/check/v4-2023-04-27")!
        comps.queryItems = [URLQueryItem(name: "timezone_offset_min", value: "\(-TimeZone.current.secondsFromGMT() / 60)")]
        guard let url = comps.url else { return (nil, nil) }
        let req = codexSubscriptionRequest(url: url, accessToken: accessToken, accountId: nil, targetPath: "/backend-api/accounts/check/v4-2023-04-27")
        guard let json = await getJSON(req) else { return (nil, nil) }
        return parseCodexAccountCheckSubscription(json, preferredAccountId: accountId)
    }

    private static func fetchCodexSubscription(accessToken: String, accountId: String) async -> Date? {
        var comps = URLComponents(string: "https://chatgpt.com/backend-api/subscriptions")!
        comps.queryItems = [URLQueryItem(name: "account_id", value: accountId)]
        guard let url = comps.url else { return nil }
        let req = codexSubscriptionRequest(url: url, accessToken: accessToken, accountId: nil, targetPath: "/backend-api/subscriptions")
        guard let json = await getJSON(req) else { return nil }
        return parseDate(json["active_until"] ?? json["expires_at"])
    }

    private static func codexSubscriptionRequest(url: URL, accessToken: String, accountId: String?, targetPath: String) -> URLRequest {
        var req = URLRequest(url: url)
        req.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        req.setValue("https://chatgpt.com/", forHTTPHeaderField: "Referer")
        req.setValue("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36", forHTTPHeaderField: "User-Agent")
        req.setValue(targetPath, forHTTPHeaderField: "x-openai-target-path")
        req.setValue(targetPath, forHTTPHeaderField: "x-openai-target-route")
        if let accountId { req.setValue(accountId, forHTTPHeaderField: "ChatGPT-Account-Id") }
        return req
    }

    private static func parseCodexAccountCheckSubscription(_ json: [String: Any], preferredAccountId: String?) -> (accountId: String?, expiresAt: Date?) {
        let records = codexAccountCheckRecords(json)
        let selected = records.first { record in
            guard let preferredAccountId else { return false }
            return extractAccountId(record) == preferredAccountId
        } ?? records.first

        guard let selected else { return (nil, nil) }
        let account = (selected["account"] as? [String: Any]) ?? selected
        let entitlement = selected["entitlement"] as? [String: Any]
        let accountId = firstString(account, keys: ["account_id", "id", "chatgpt_account_id", "workspace_id"])
        let expiresAt = parseDate(entitlement?["expires_at"] ?? account["expires_at"])
        return (accountId, expiresAt)
    }

    private static func codexAccountCheckRecords(_ json: [String: Any]) -> [[String: Any]] {
        if let accounts = json["accounts"] as? [[String: Any]] { return accounts }
        if let records = json["records"] as? [[String: Any]] { return records }
        if let items = json["items"] as? [[String: Any]] { return items }
        if let data = json["data"] as? [String: Any] { return codexAccountCheckRecords(data) }
        if let accountItems = json["account_items"] as? [[String: Any]] { return accountItems }

        return json.values.compactMap { value in
            if let object = value as? [String: Any],
               object["account"] != nil || object["entitlement"] != nil || object["expires_at"] != nil {
                return object
            }
            return nil
        }
    }

    private static func extractAccountId(_ record: [String: Any]) -> String? {
        let account = (record["account"] as? [String: Any]) ?? record
        return firstString(account, keys: ["account_id", "id", "chatgpt_account_id", "workspace_id"])
    }

    // MARK: - Helpers (additional)

    private static func readSqliteValue(_ path: String, sql: String) -> String? {
        runProcess("/usr/bin/sqlite3", [path, sql])?.trimmingCharacters(in: .whitespacesAndNewlines)
            .nonEmpty
    }

    private static func getJSON(_ req: URLRequest) async -> [String: Any]? {
        var r = req
        r.timeoutInterval = 10
        guard let (data, resp) = try? await URLSession.shared.data(for: r),
              let http = resp as? HTTPURLResponse, (200...299).contains(http.statusCode),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        return json
    }

    private static func readJSON(_ path: String) -> [String: Any]? {
        guard let data = FileManager.default.contents(atPath: path),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        return json
    }

    private static func numeric(_ v: Any?) -> Double? {
        if let d = v as? Double { return d }
        if let i = v as? Int { return Double(i) }
        if let n = v as? NSNumber { return n.doubleValue }
        if let s = v as? String {
            let trimmed = s.trimmingCharacters(in: .whitespacesAndNewlines)
            if let d = Double(trimmed) { return d }
        }
        return nil
    }

    private static func parseDate(_ v: Any?) -> Date? {
        // Epoch seconds (codex reset_at) or millis.
        if let n = numeric(v), n > 0 {
            let secs = n > 1_000_000_000_000 ? n / 1000 : n
            return Date(timeIntervalSince1970: secs)
        }
        guard let s = v as? String, !s.isEmpty else { return nil }
        let iso = ISO8601DateFormatter()
        if let d = iso.date(from: s) { return d }
        // YYYY-MM-DD
        let f = DateFormatter(); f.dateFormat = "yyyy-MM-dd"; f.timeZone = TimeZone(identifier: "UTC")
        return f.date(from: s)
    }

    private static func firstString(_ object: [String: Any], keys: [String]) -> String? {
        for key in keys {
            if let s = object[key] as? String {
                let trimmed = s.trimmingCharacters(in: .whitespacesAndNewlines)
                if !trimmed.isEmpty { return trimmed }
            } else if let n = object[key] as? NSNumber {
                return n.stringValue
            }
        }
        return nil
    }

    private static func readCockpitAccount(provider: String) -> [String: Any]? {
        let base = "\(NSHomeDirectory())/.antigravity_cockpit"
        let indexPath = "\(base)/\(provider)_accounts.json"
        let accountsDir = "\(base)/\(provider)_accounts"
        if let index = readJSON(indexPath),
           let id = cockpitAccountId(index) {
            if let detail = readJSON("\(accountsDir)/\(id).json") {
                return detail
            }
        }

        guard let files = try? FileManager.default.contentsOfDirectory(
            at: URL(fileURLWithPath: accountsDir),
            includingPropertiesForKeys: [.contentModificationDateKey]
        ) else { return nil }
        let candidates = files
            .filter { $0.pathExtension == "json" }
            .sorted { lhs, rhs in
                let left = (try? lhs.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
                let right = (try? rhs.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
                return left > right
            }
        for url in candidates {
            if let detail = readJSON(url.path) { return detail }
        }
        return nil
    }

    private static func cockpitAccountId(_ index: [String: Any]) -> String? {
        let accounts = (index["accounts"] as? [[String: Any]]) ?? []
        let selected = accounts.max { lhs, rhs in
            (numeric(lhs["last_used_at"] ?? lhs["last_used"] ?? lhs["updated_at"]) ?? 0)
                < (numeric(rhs["last_used_at"] ?? rhs["last_used"] ?? rhs["updated_at"]) ?? 0)
        }
        guard let selected else {
            return firstString(index, keys: ["current_account_id", "active_account_id", "account_id", "id"])
        }
        return firstString(selected, keys: ["id", "account_id", "user_id"])
    }

    private static func readCodebuddyAuth() -> [String: Any]? {
        #if os(macOS)
        let path = "\(NSHomeDirectory())/Library/Application Support/CodeBuddyExtension/Data/Public/auth/Tencent-Cloud.coding-copilot.info"
        #else
        let path = "\(NSHomeDirectory())/.local/share/CodeBuddyExtension/Data/Public/auth/Tencent-Cloud.coding-copilot.info"
        #endif
        return readJSON(path)
    }

    private static func readWorkBuddyAuth() -> [String: Any]? {
        #if os(macOS)
        let path = "\(NSHomeDirectory())/Library/Application Support/CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info"
        #else
        let path = "\(NSHomeDirectory())/.local/share/CodeBuddyExtension/Data/Public/auth/workbuddy-desktop.info"
        #endif
        return readJSON(path)
    }

    private static func refreshWorkBuddyAccount(from auth: [String: Any]) async -> [String: Any]? {
        let authInfo = (auth["auth"] as? [String: Any]) ?? [:]
        let accountInfo = (auth["account"] as? [String: Any]) ?? [:]

        guard let accessToken = firstString(authInfo, keys: ["accessToken", "access_token"]),
              !accessToken.isEmpty else {
            return nil
        }

        let uid = firstString(accountInfo, keys: ["uid", "id"])
        let nickname = firstString(accountInfo, keys: ["nickname", "label"])
        let email = firstString(accountInfo, keys: ["email"]) ?? nickname ?? uid ?? "unknown"
        let enterpriseId = firstString(accountInfo, keys: ["enterpriseId", "enterprise_id"])
        let enterpriseName = firstString(accountInfo, keys: ["enterpriseName", "enterprise_name"])
        let domain = firstString(authInfo, keys: ["domain"])
        let refreshToken = firstString(authInfo, keys: ["refreshToken", "refresh_token"])
        let tokenType = firstString(authInfo, keys: ["tokenType", "token_type"]) ?? "Bearer"
        let expiresAt = numeric(authInfo["expiresAt"] ?? authInfo["expires_at"]).map(Int64.init)

        async let dosage = postWorkBuddyJSON(
            path: "/v2/billing/meter/get-dosage-notify",
            accessToken: accessToken,
            uid: uid,
            enterpriseId: enterpriseId,
            domain: domain,
            body: [:]
        )
        async let payment = postWorkBuddyJSON(
            path: "/v2/billing/meter/get-payment-type",
            accessToken: accessToken,
            uid: uid,
            enterpriseId: enterpriseId,
            domain: domain,
            body: [:]
        )
        async let userResource = postWorkBuddyJSON(
            path: "/v2/billing/meter/get-user-resource",
            accessToken: accessToken,
            uid: uid,
            enterpriseId: enterpriseId,
            domain: domain,
            body: workbuddyUserResourceBody()
        )

        let dosageJSON = await dosage
        let paymentJSON = await payment
        let userResourceJSON = await userResource

        guard dosageJSON != nil || paymentJSON != nil || userResourceJSON != nil else {
            return nil
        }

        let paymentType = nestedString(paymentJSON, paths: [
            ["data", "paymentType"],
            ["data"],
        ])

        var quotaRaw: [String: Any] = [:]
        if let dosageJSON { quotaRaw["dosage"] = dosageJSON }
        if let paymentJSON { quotaRaw["payment"] = paymentJSON }
        if let userResourceJSON { quotaRaw["userResource"] = userResourceJSON }

        let now = Int(Date().timeIntervalSince1970)
        let accountIdSeed = uid ?? email
        let accountId = "workbuddy_" + sanitizeFileComponent(accountIdSeed.isEmpty ? "local" : accountIdSeed)

        var result: [String: Any] = [
            "id": accountId,
            "email": email,
            "access_token": accessToken,
            "token_type": tokenType,
            "last_used": now,
            "created_at": now,
            "status": "normal",
            "auth_raw": auth,
            "profile_raw": accountInfo,
            "usage_updated_at": now,
        ]
        if let uid { result["uid"] = uid }
        if let nickname { result["nickname"] = nickname }
        if let enterpriseId { result["enterprise_id"] = enterpriseId }
        if let enterpriseName { result["enterprise_name"] = enterpriseName }
        if let refreshToken { result["refresh_token"] = refreshToken }
        if let domain { result["domain"] = domain }
        if let expiresAt { result["expires_at"] = expiresAt }
        if let paymentType { result["payment_type"] = paymentType }
        if !quotaRaw.isEmpty { result["quota_raw"] = quotaRaw }
        if let userResourceJSON { result["usage_raw"] = userResourceJSON }
        return result
    }

    private static func postWorkBuddyJSON(
        path: String,
        accessToken: String,
        uid: String?,
        enterpriseId: String?,
        domain: String?,
        body: [String: Any]
    ) async -> [String: Any]? {
        guard let url = URL(string: "https://www.codebuddy.cn\(path)") else { return nil }
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.setValue("application/json, text/plain, */*", forHTTPHeaderField: "Accept")
        if let uid { req.setValue(uid, forHTTPHeaderField: "X-User-Id") }
        if let enterpriseId {
            req.setValue(enterpriseId, forHTTPHeaderField: "X-Enterprise-Id")
            req.setValue(enterpriseId, forHTTPHeaderField: "X-Tenant-Id")
        }
        if let domain { req.setValue(domain, forHTTPHeaderField: "X-Domain") }
        req.httpBody = try? JSONSerialization.data(withJSONObject: body)
        return await getJSON(req)
    }

    private static func workbuddyUserResourceBody() -> [String: Any] {
        let now = Date()
        let end = Calendar(identifier: .gregorian).date(byAdding: .year, value: 101, to: now) ?? now
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH:mm:ss"
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone.current
        return [
            "PageNumber": 1,
            "PageSize": 100,
            "ProductCode": "p_tcaca",
            "Status": [0, 3],
            "PackageEndTimeRangeBegin": formatter.string(from: now),
            "PackageEndTimeRangeEnd": formatter.string(from: end),
        ]
    }

    private static func writeCockpitAccountSnapshot(provider: String, account: [String: Any]) {
        guard let accountId = firstString(account, keys: ["id"]) else { return }
        let base = "\(NSHomeDirectory())/.antigravity_cockpit"
        let accountsDir = "\(base)/\(provider)_accounts"
        let indexPath = "\(base)/\(provider)_accounts.json"
        let detailPath = "\(accountsDir)/\(accountId).json"
        let summary: [String: Any] = [
            "id": accountId,
            "email": firstString(account, keys: ["email"]) ?? accountId,
            "plan_type": firstString(account, keys: ["plan_type", "payment_type"]) as Any,
            "created_at": account["created_at"] ?? Int(Date().timeIntervalSince1970),
            "last_used": account["last_used"] ?? Int(Date().timeIntervalSince1970),
        ].compactMapValues { $0 }
        let index: [String: Any] = [
            "version": "1.0",
            "accounts": [summary],
        ]

        let fm = FileManager.default
        try? fm.createDirectory(atPath: accountsDir, withIntermediateDirectories: true)
        writeJSON(account, to: detailPath)
        writeJSON(index, to: indexPath)
    }

    private static func writeJSON(_ json: [String: Any], to path: String) {
        guard JSONSerialization.isValidJSONObject(json),
              let data = try? JSONSerialization.data(withJSONObject: json, options: [.prettyPrinted]) else { return }
        FileManager.default.createFile(atPath: path, contents: data)
    }

    private static func sanitizeFileComponent(_ raw: String) -> String {
        let cleaned = raw.replacingOccurrences(of: #"[^A-Za-z0-9._-]"#, with: "_", options: .regularExpression)
        return cleaned.isEmpty ? "local" : cleaned
    }

    private static func workbuddyLimit(from account: [String: Any], configured: Bool, providerName: String = "workbuddy", displayPrefix: String = "WorkBuddy") -> ProviderLimit {
        let name = providerName
        let plan = planLabel(nestedString(account, paths: [
            ["payment_type"],
            ["plan_type"],
            ["quota_raw", "payment", "data", "paymentType"],
            ["quota_raw", "payment", "data"],
        ]), displayPrefix)
        let windows = workbuddyWindows(from: account)
        return ProviderLimit(
            name: name,
            planLabel: plan,
            configured: configured,
            error: windows.isEmpty ? "No usage data" : nil,
            quotaResetAt: windows.compactMap(\.resetAt).sorted().first,
            windows: windows
        )
    }

    private static func workbuddyWindows(from account: [String: Any]) -> [LimitWindow] {
        let roots: [Any?] = [
            nestedValue(account, ["usage_raw"]),
            nestedValue(account, ["quota_raw", "userResource"]),
            nestedValue(account, ["quota_raw"]),
        ]

        for root in roots {
            guard let accounts = nestedValue(root, ["data", "Response", "Data", "Accounts"]) as? [[String: Any]] else {
                continue
            }

            var used: Double = 0
            var total: Double = 0
            var resetAt: Date? = nil

            for item in accounts {
                if let status = item["Status"] as? Int, status != 0 && status != 3 { continue }
                let itemTotal = numeric(item["CycleCapacitySizePrecise"] ?? item["CycleCapacitySize"] ?? item["CapacitySizePrecise"] ?? item["CapacitySize"]) ?? 0
                let itemRemain = numeric(item["CycleCapacityRemainPrecise"] ?? item["CycleCapacityRemain"] ?? item["CapacityRemainPrecise"] ?? item["CapacityRemain"]) ?? 0
                guard itemTotal > 0 else { continue }
                total += itemTotal
                used += max(itemTotal - itemRemain, 0)
                let candidate = parseDate(item["CycleEndTime"] ?? item["ExpiredTime"] ?? item["DeductionEndTime"])
                if let candidate {
                    resetAt = nearestFutureDate(current: resetAt, candidate: candidate)
                }
            }

            guard total > 0 else { continue }
            return [LimitWindow(label: "Credits", usedPercent: min(max(used / total * 100, 0), 100), resetAt: resetAt)]
        }

        return []
    }

    private static func nearestFutureDate(current: Date?, candidate: Date) -> Date {
        guard let current else { return candidate }
        let now = Date()
        let currentScore = current >= now ? current.timeIntervalSince(now) : Double.greatestFiniteMagnitude
        let candidateScore = candidate >= now ? candidate.timeIntervalSince(now) : Double.greatestFiniteMagnitude
        if candidateScore < currentScore { return candidate }
        if currentScore == Double.greatestFiniteMagnitude && candidate > current { return candidate }
        return current
    }

    private static func nestedString(_ object: Any?, paths: [[String]]) -> String? {
        for path in paths {
            if let value = nestedValue(object, path) {
                if let s = value as? String {
                    let trimmed = s.trimmingCharacters(in: .whitespacesAndNewlines)
                    if !trimmed.isEmpty { return trimmed }
                }
                if let n = value as? NSNumber { return n.stringValue }
            }
        }
        return nil
    }

    private static func nestedDate(_ object: Any?, paths: [[String]]) -> Date? {
        for path in paths {
            if let date = parseDate(nestedValue(object, path)) {
                return date
            }
        }
        return nil
    }

    private static func nestedValue(_ object: Any?, _ path: [String]) -> Any? {
        var current = object
        for key in path {
            guard let dict = current as? [String: Any] else { return nil }
            current = dict[key]
        }
        return current
    }

    private static func copilotStyleWindows(from account: [String: Any], resetAt: Date?) -> [LimitWindow] {
        var windows: [LimitWindow] = []
        let snapshots = nestedValue(account, ["copilot_quota_snapshots"])
            ?? nestedValue(account, ["public_account", "copilot_quota_snapshots"])
        if let snaps = snapshots as? [String: Any] {
            for (key, label) in [("premium_interactions", "Premium"), ("premium_models", "Premium")] {
                guard let q = snaps[key] as? [String: Any] else { continue }
                if let remaining = numeric(q["percent_remaining"]) {
                    windows.append(LimitWindow(label: label, usedPercent: min(max(100 - remaining, 0), 100), resetAt: resetAt))
                    break
                }
            }
            if let chat = snaps["chat"] as? [String: Any],
               let remaining = numeric(chat["percent_remaining"]) {
                windows.append(LimitWindow(label: "Chat", usedPercent: min(max(100 - remaining, 0), 100), resetAt: resetAt))
            }
        }

        let limited = nestedValue(account, ["copilot_limited_user_quotas"])
            ?? nestedValue(account, ["public_account", "copilot_limited_user_quotas"])
        if let limited = limited as? [String: Any] {
            for (key, label) in [("completions", "Completions"), ("chat", "Chat")] {
                guard let remaining = numeric(limited[key]) else { continue }
                if !windows.contains(where: { $0.label == label }) {
                    windows.append(LimitWindow(label: label, usedPercent: remaining > 0 ? 0 : 100, resetAt: resetAt))
                }
            }
        }
        return windows
    }

    private static func jwtClaim(_ token: String, _ claim: String) -> String? {
        guard let value = jwtClaimValue(token, claim) else { return nil }
        if let s = value as? String { return s }
        if let n = value as? NSNumber { return n.stringValue }
        return nil
    }

    private static func jwtClaimDate(_ token: String, _ claim: String) -> Date? {
        parseDate(jwtClaimValue(token, claim))
    }

    private static func jwtClaimValue(_ token: String, _ claim: String) -> Any? {
        let parts = token.split(separator: ".")
        guard parts.count >= 2 else { return nil }
        var b64 = String(parts[1]).replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        while b64.count % 4 != 0 { b64 += "=" }
        guard let data = Data(base64Encoded: b64),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        if let auth = json["https://api.openai.com/auth"] as? [String: Any], let v = auth[claim] { return v }
        return json[claim]
    }

    private static func planLabel(_ raw: String?, _ prefix: String) -> String? {
        guard var s = raw?.trimmingCharacters(in: .whitespaces), !s.isEmpty else { return nil }
        if s.lowercased().hasPrefix(prefix.lowercased()) {
            s = String(s.dropFirst(prefix.count)).trimmingCharacters(in: .whitespaces)
        }
        if s.isEmpty { return prefix }
        return s.prefix(1).uppercased() + s.dropFirst().lowercased()
    }

    private static func claudeAccessToken() -> String? {
        if let payload = claudeKeychainJSON() ?? readJSON("\(NSHomeDirectory())/.claude/.credentials.json"),
           let oauth = payload["claudeAiOauth"] as? [String: Any] {
            return oauth["accessToken"] as? String
        }
        return nil
    }

    private static func claudeSubscription() -> String? {
        if let payload = claudeKeychainJSON() ?? readJSON("\(NSHomeDirectory())/.claude/.credentials.json"),
           let oauth = payload["claudeAiOauth"] as? [String: Any] {
            return oauth["subscriptionType"] as? String
        }
        return nil
    }

    private static func claudeKeychainJSON() -> [String: Any]? {
        guard let out = runProcess("/usr/bin/security", ["find-generic-password", "-s", "Claude Code-credentials", "-w"]),
              let data = out.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        return json
    }

    private static func copilotToken() -> String? {
        let base = "\(NSHomeDirectory())/.config/github-copilot"
        for file in ["apps.json", "hosts.json"] {
            guard let json = readJSON("\(base)/\(file)") else { continue }
            // Prefer keys starting with github.com
            let sorted = json.keys.sorted { $0.hasPrefix("github.com") && !$1.hasPrefix("github.com") }
            for key in sorted {
                if let entry = json[key] as? [String: Any], let t = entry["oauth_token"] as? String, !t.isEmpty {
                    return t
                }
            }
        }
        return nil
    }

    private static func runKiroUsage() -> String? {
        // Resolve kiro-cli (PATH may not be inherited by the app)
        let candidates = ["\(NSHomeDirectory())/.local/bin/kiro-cli", "/usr/local/bin/kiro-cli", "/opt/homebrew/bin/kiro-cli"]
        let bin = candidates.first { FileManager.default.isExecutableFile(atPath: $0) } ?? "kiro-cli"
        return runProcess(bin, ["chat", "--no-interactive", "/usage"], env: ["TERM": "xterm-256color"])
    }

    private static func runProcess(_ launchPath: String, _ args: [String], env: [String: String]? = nil) -> String? {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: launchPath)
        p.arguments = args
        if let env { var e = ProcessInfo.processInfo.environment; env.forEach { e[$0] = $1 }; p.environment = e }
        let outPipe = Pipe(); let errPipe = Pipe()
        p.standardOutput = outPipe; p.standardError = errPipe
        do { try p.run() } catch { return nil }
        p.waitUntilExit()
        let out = String(data: outPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let err = String(data: errPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        let combined = err.isEmpty ? out : err
        return combined.isEmpty ? nil : combined
    }

    private static func firstMatch(_ text: String, _ pattern: String, group: Int = 1) -> String? {
        guard let re = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive]),
              let m = re.firstMatch(in: text, range: NSRange(text.startIndex..., in: text)),
              m.numberOfRanges > group, let r = Range(m.range(at: group), in: text) else { return nil }
        return String(text[r])
    }

    private static func kiroResetDate(_ out: String) -> Date? {
        for key in ["resetAt", "resetTime", "resetOn", "nextDateReset"] {
            if let raw = firstMatch(out, #""\#(key)"\s*:\s*"([^"]+)""#),
               let date = parseDate(raw) {
                return date
            }
            if let raw = firstMatch(out, #""\#(key)"\s*:\s*(\d+(?:\.\d+)?)"#),
               let date = parseDate(raw) {
                return date
            }
        }

        // "resets on 2026-07-01" (ISO) or "resets on 07/01" (MM/DD)
        if let iso = firstMatch(out, #"resets on (\d{4}-\d{2}-\d{2})"#) {
            let f = DateFormatter(); f.dateFormat = "yyyy-MM-dd"; f.timeZone = TimeZone(identifier: "UTC")
            return f.date(from: iso)
        }
        guard let md = firstMatch(out, #"resets on (\d{2}/\d{2})"#) else { return nil }
        let parts = md.split(separator: "/")
        guard parts.count == 2, let mm = Int(parts[0]), let dd = Int(parts[1]) else { return nil }
        var comps = DateComponents()
        let now = Date(); let cal = Calendar(identifier: .gregorian)
        comps.year = cal.component(.year, from: now); comps.month = mm; comps.day = dd
        comps.timeZone = TimeZone(identifier: "UTC")
        guard var date = cal.date(from: comps) else { return nil }
        if date < now { comps.year! += 1; date = cal.date(from: comps) ?? date }
        return date
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}
