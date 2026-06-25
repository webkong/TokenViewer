import Foundation

/// Visibility toggles for provider limit cards in the menu bar popover.
/// Sources are loaded dynamically from the Rust core's `has_limits` provider list,
/// keeping the limits panel in sync with the unified agent registry.
enum LimitsVisibilityStore {
    // MARK: - Fallback (used before Rust core is available)

    private static let fallbackSources: [String] = [
        "claude", "codex", "cursor", "gemini", "kiro", "copilot",
        "kimi", "antigravity", "zed", "trae", "windsurf", "qoder",
        "codebuddy", "workbuddy",
    ]

    private static let fallbackDisplayNames: [String: String] = [
        "claude": "Claude", "codex": "Codex", "cursor": "Cursor",
        "gemini": "Gemini", "kiro": "Kiro", "copilot": "GitHub Copilot",
        "kimi": "Kimi", "antigravity": "Antigravity", "zed": "Zed",
        "trae": "Trae", "windsurf": "Windsurf", "qoder": "Qoder",
        "codebuddy": "CodeBuddy", "workbuddy": "WorkBuddy",
    ]

    // MARK: - Cached Rust data

    private static var cachedSources: [String]?
    private static var cachedDisplayNames: [String: String]?
    private static var loadAttempted = false

    /// All provider sources that have subscription/quota tracking (`has_limits: true`).
    /// Loaded from the Rust core; falls back to a hardcoded list during early launch.
    static var allSources: [String] {
        if let cached = cachedSources { return cached }
        loadFromRust()
        return cachedSources ?? fallbackSources
    }

    /// Default value for the `limitsVisibleSources` UserDefaults key.
    /// Always returns the full set joined by comma so that new providers
    /// are visible by default on first launch.
    static var defaultsValue: String { allSources.joined(separator: ",") }

    // MARK: - Load

    /// Call from `onAppear` to eagerly populate the cache so UI renders without a flash.
    static func load() {
        loadFromRust()
    }

    private static func loadFromRust() {
        guard !loadAttempted else { return }
        loadAttempted = true

        guard let data = CoreBridge.shared.skillsListAgents() else { return }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let providers = try? decoder.decode([SkillProvider].self, from: data) else { return }

        let hasLimits = providers.filter(\.hasLimits)
        cachedSources = hasLimits.map(\.source)
        cachedDisplayNames = Dictionary(uniqueKeysWithValues: hasLimits.map { ($0.source, $0.displayName) })
    }

    // MARK: - Helpers

    static func visibleSet(from raw: String) -> Set<String> {
        let parts = raw
            .split(separator: ",")
            .map { String($0).trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        return Set(parts)
    }

    static func rawValue(from visible: Set<String>) -> String {
        allSources.filter { visible.contains($0) }.joined(separator: ",")
    }

    static func displayName(for source: String) -> String {
        if let names = cachedDisplayNames, let name = names[source] {
            return name
        }
        return fallbackDisplayNames[source] ?? source.capitalized
    }
}
