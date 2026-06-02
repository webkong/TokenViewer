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
        return await [claude, codex, copilot, kiro]
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
        let plan = planLabel(firstMatch(out, #"Plan:\s*(.+)"#) ?? firstMatch(out, #"\|\s*(KIRO\s+\w+)"#), "Kiro")
        var windows: [LimitWindow] = []
        if let pct = firstMatch(out, #"█+\s*(\d+)%"#).flatMap({ Double($0) }) {
            windows.append(LimitWindow(label: "Credits", usedPercent: pct, resetAt: kiroResetDate(out)))
        } else if let used = firstMatch(out, #"\((\d+(?:\.\d+)?)\s+of\s+(\d+(?:\.\d+)?)\s+covered"#, group: 1).flatMap({ Double($0) }),
                  let total = firstMatch(out, #"\((\d+(?:\.\d+)?)\s+of\s+(\d+(?:\.\d+)?)\s+covered"#, group: 2).flatMap({ Double($0) }), total > 0 {
            windows.append(LimitWindow(label: "Credits", usedPercent: used / total * 100, resetAt: kiroResetDate(out)))
        }
        return ProviderLimit(name: name, planLabel: plan, configured: !windows.isEmpty, error: windows.isEmpty ? "No usage data" : nil, windows: windows)
    }

    // MARK: - Helpers

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
