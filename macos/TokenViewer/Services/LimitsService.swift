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
    let windows: [LimitWindow]
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
        return await [claude, codex, copilot, kiro, cursor, gemini, kimi, antigravity]
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
        let plan = planLabel(jwtClaim(accessToken, "chatgpt_plan_type"), "Codex")
        let accountId = tokens["account_id"] as? String ?? jwtClaim(accessToken, "chatgpt_account_id")

        var req = URLRequest(url: URL(string: "https://chatgpt.com/backend-api/wham/usage")!)
        req.setValue("Bearer \(accessToken)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        if let acc = accountId { req.setValue(acc, forHTTPHeaderField: "ChatGPT-Account-Id") }

        guard let json = await getJSON(req), let rl = json["rate_limit"] as? [String: Any] else {
            return ProviderLimit(name: name, planLabel: plan, configured: true, error: "Request failed", windows: [])
        }
        var windows: [LimitWindow] = []
        for key in ["primary_window", "secondary_window"] {
            guard let w = rl[key] as? [String: Any] else { continue }
            let secs = (w["limit_window_seconds"] as? Int) ?? 0
            let label = secs >= 604800 ? "Weekly" : (secs >= 18000 ? "5 Hour" : "Window")
            let used = (w["used_percent"] as? Double) ?? Double(w["used_percent"] as? Int ?? 0)
            windows.append(LimitWindow(label: label, usedPercent: used, resetAt: parseDate(w["reset_at"])))
        }
        return ProviderLimit(name: name, planLabel: plan, configured: true, error: nil, windows: windows)
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
        return ProviderLimit(name: name, planLabel: plan, configured: true, error: nil, windows: windows)
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
        var windows: [LimitWindow] = []
        // "1850.54 of 2000 covered" or "1850 of 2000 covered"
        if let used = firstMatch(clean, #"(\d+(?:\.\d+)?)\s+of\s+(\d+(?:\.\d+)?)\s+covered"#, group: 1).flatMap(Double.init),
           let total = firstMatch(clean, #"(\d+(?:\.\d+)?)\s+of\s+(\d+(?:\.\d+)?)\s+covered"#, group: 2).flatMap(Double.init), total > 0 {
            windows.append(LimitWindow(label: "Credits", usedPercent: used / total * 100, resetAt: kiroResetDate(clean)))
        } else if let pct = firstMatch(clean, #"█+\s*(\d+)%"#).flatMap({ Double($0) }) {
            windows.append(LimitWindow(label: "Credits", usedPercent: pct, resetAt: kiroResetDate(clean)))
        }
        return ProviderLimit(name: name, planLabel: plan, configured: !windows.isEmpty, error: windows.isEmpty ? "No usage data" : nil, windows: windows)
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
        return ProviderLimit(name: name, planLabel: plan, configured: true, error: windows.isEmpty ? "No usage data" : nil, windows: windows)
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
        return ProviderLimit(name: name, planLabel: nil, configured: true, error: nil, windows: windows)
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

    private static func jwtClaim(_ token: String, _ claim: String) -> String? {
        let parts = token.split(separator: ".")
        guard parts.count >= 2 else { return nil }
        var b64 = String(parts[1]).replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        while b64.count % 4 != 0 { b64 += "=" }
        guard let data = Data(base64Encoded: b64),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        if let auth = json["https://api.openai.com/auth"] as? [String: Any], let v = auth[claim] as? String { return v }
        return json[claim] as? String
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
