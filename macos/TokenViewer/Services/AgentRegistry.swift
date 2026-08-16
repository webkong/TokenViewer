import Foundation
import SwiftUI

/// Single source of truth for coding-agent display metadata.
///
/// Loaded from the Rust `tt_skills_list_agents()` FFI which returns the canonical
/// agent config including display names, brand colors, logo filenames, and
/// install status. This replaces the hardcoded `TVColor.sourceDisplayName()`,
/// legacy hardcoded display and logo lookups.
///
/// Falls back to generic formatting when the Rust data isn't available yet
/// (early launch or first load).
@MainActor
final class AgentRegistry: ObservableObject {
    static let shared = AgentRegistry()
    static let defaultAgentSources = ["claude", "codex", "opencode"]
    static let defaultAgentSourcesJSON = "[\"claude\",\"codex\",\"opencode\"]"

    @Published private(set) var allAgents: [AgentConfig] = []
    /// True once `refreshInstallStatus()` has completed at least one detection pass.
    private(set) var hasDetectedInstalls = false

    private var agents: [String: AgentConfig] = [:]
    private var installStatus: [String: Bool] = [:]
    private var loaded = false
    /// Debounce timestamp for `refreshInstallStatus()` (see that method).
    private var lastInstallRefreshAt: Date?

    private init() {}

    /// Load agent data from the Rust core via `skillsListAgents()`.
    /// Idempotent after success; failed attempts remain retryable.
    func loadIfNeeded() {
        guard !loaded else { return }
        loaded = loadFromRust()
    }

    /// Force-reload from the Rust core (e.g. after install status changes).
    func reload() {
        loaded = false
        loadIfNeeded()
    }

    /// Publish an agent snapshot that was loaded off the main thread. Skills
    /// refreshes use this path so directory detection and FFI work never block
    /// tab transitions.
    func applySnapshot(_ refreshedAgents: [AgentConfig]) {
        allAgents = refreshedAgents
        agents = Dictionary(uniqueKeysWithValues: refreshedAgents.map { ($0.source, $0) })
        installStatus = Dictionary(uniqueKeysWithValues: refreshedAgents.map { ($0.source, $0.isInstalled) })
        loaded = true
    }

    /// Canonical list of agents that support subscription/quota tracking.
    var limitSources: [String] {
        loadedAgents.filter(\.hasLimits).map(\.source)
    }

    /// Canonical list of agents available to the skills manager.
    var skillAgents: [AgentConfig] {
        loadedAgents
    }

    /// Installed agents first, then display-name sorted.
    var sortedAgents: [AgentConfig] {
        sortInstalledFirst(loadedAgents)
    }

    /// Skills-capable agents sorted for Settings.
    var sortedSkillAgents: [AgentConfig] {
        sortInstalledFirst(skillAgents)
    }

    /// Limits-capable agents sorted for Settings.
    var sortedLimitAgents: [AgentConfig] {
        sortInstalledFirst(loadedAgents.filter(\.hasLimits))
    }

    // MARK: - Lookups

    /// Display name for an agent source, e.g. "claude" → "Claude Code".
    func displayName(for source: String) -> String {
        loadIfNeeded()
        let key = source.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if let agent = agents[key], !agent.displayName.isEmpty {
            return agent.displayName
        }
        // Fallback for aliases
        let resolved = resolveAlias(key)
        if resolved != key, let agent = agents[resolved], !agent.displayName.isEmpty {
            return agent.displayName
        }
        return Self.prettySourceName(resolved)
    }

    /// Brand color hex string, e.g. "#d97757", for an agent source.
    func brandColorHex(for source: String) -> String {
        loadIfNeeded()
        let key = source.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if let agent = agents[key], !agent.brandColor.isEmpty {
            return agent.brandColor
        }
        let resolved = resolveAlias(key)
        if resolved != key, let agent = agents[resolved], !agent.brandColor.isEmpty {
            return agent.brandColor
        }
        return "#059669"
    }

    /// SwiftUI `Color` for an agent source.
    func brandColor(for source: String) -> Color {
        Color(hex: brandColorHex(for: source))
    }

    /// Logo filename (without extension) for an agent source, e.g. "claude-code".
    func logoFile(for source: String) -> String {
        loadIfNeeded()
        let key = source.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if let agent = agents[key], !agent.logoFile.isEmpty {
            return agent.logoFile
        }
        let resolved = resolveAlias(key)
        if resolved != key, let agent = agents[resolved], !agent.logoFile.isEmpty {
            return agent.logoFile
        }
        return resolved
    }

    /// Install status for an agent source.
    func isInstalled(for source: String) -> Bool {
        loadIfNeeded()
        let key = source.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if let agent = agents[key] { return agent.isInstalled }
        return false
    }

    /// Refresh install status through the Rust core and publish one consistent agent snapshot.
    ///
    /// The Rust endpoint deliberately bypasses the 1-hour status cache (it is
    /// the explicit refresh path), so collapse redundant triggers — Settings
    /// fires `.onAppear` for both the window and the skills section — with a
    /// short in-memory debounce instead of re-spawning `which` for every agent
    /// on each one.
    func refreshInstallStatus() {
        let now = Date()
        if let last = lastInstallRefreshAt, now.timeIntervalSince(last) < 5 {
            return
        }
        lastInstallRefreshAt = now

        Task.detached {
            var installMap: [String: Bool] = [:]
            if let detectData = CoreBridge.shared.skillsDetectInstalled(),
               let detected = try? JSONDecoder().decode([String: Bool].self, from: detectData) {
                installMap = detected
            }

            let finalInstallMap = installMap
            await MainActor.run {
                self.installStatus = finalInstallMap
                self.applyInstallStatus(finalInstallMap)
                self.hasDetectedInstalls = true
            }
        }
    }

    // MARK: - Load

    private func loadFromRust() -> Bool {
        guard let data = CoreBridge.shared.skillsListAgents() else { return false }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let decoded = try? decoder.decode([AgentConfig].self, from: data) else { return false }
        allAgents = decoded.map { agent in
            var updated = agent
            if let isInstalled = installStatus[agent.source] {
                updated.isInstalled = isInstalled
            }
            return updated
        }
        agents = Dictionary(uniqueKeysWithValues: allAgents.map { ($0.source, $0) })
        return true
    }

    private var loadedAgents: [AgentConfig] {
        loadIfNeeded()
        return allAgents
    }

    private func applyInstallStatus(_ installMap: [String: Bool]) {
        guard !installMap.isEmpty else { return }
        allAgents = allAgents.map { agent in
            var updated = agent
            if let isInstalled = installMap[agent.source] {
                updated.isInstalled = isInstalled
            }
            return updated
        }
        agents = Dictionary(uniqueKeysWithValues: allAgents.map { ($0.source, $0) })
    }

    private func sortInstalledFirst(_ items: [AgentConfig]) -> [AgentConfig] {
        items.sorted { lhs, rhs in
            if lhs.isInstalled != rhs.isInstalled {
                return lhs.isInstalled && !rhs.isInstalled
            }
            return lhs.displayName.localizedCaseInsensitiveCompare(rhs.displayName) == .orderedAscending
        }
    }

    // MARK: - Alias Resolution

    /// Resolve known source aliases to canonical keys used in the agent registry.
    private func resolveAlias(_ source: String) -> String {
        switch source {
        case "claude-code": return "claude"
        case "mimo-code": return "mimocode"
        case "kilo": return "kilocode"
        case "kilo-cli": return "kilocli"
        case "kiro-ide": return "kiro"
        case "every-code": return "everycode"
        default: return source
        }
    }

    // MARK: - Generic Fallback

    private static func prettySourceName(_ source: String) -> String {
        if source == "codex" { return "ChatGPT" }
        return source
            .split(separator: "-")
            .map { part in
                let lower = part.lowercased()
                if lower == "cli" { return "CLI" }
                return lower.prefix(1).uppercased() + lower.dropFirst()
            }
            .joined(separator: " ")
    }
}

// MARK: - Color hex extension

extension Color {
    init(hex: String) {
        let hex = hex.trimmingCharacters(in: CharacterSet.alphanumerics.inverted)
        var int: UInt64 = 0
        Scanner(string: hex).scanHexInt64(&int)
        let a, r, g, b: UInt64
        switch hex.count {
        case 6:
            (a, r, g, b) = (255, (int >> 16) & 0xFF, (int >> 8) & 0xFF, int & 0xFF)
        case 8:
            (a, r, g, b) = ((int >> 24) & 0xFF, (int >> 16) & 0xFF, (int >> 8) & 0xFF, int & 0xFF)
        default:
            (a, r, g, b) = (255, 0, 0, 0)
        }
        self.init(
            .sRGB,
            red: Double(r) / 255,
            green: Double(g) / 255,
            blue: Double(b) / 255,
            opacity: Double(a) / 255
        )
    }
}
