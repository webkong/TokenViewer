import Foundation

/// Visibility toggles for provider limit cards in the menu bar popover.
/// Sources are loaded dynamically from the Rust core's `has_limits` provider list,
/// keeping the limits panel in sync with the unified agent registry.
@MainActor
enum LimitsVisibilityStore {
    // MARK: - Fallback (used only before Rust core is available)

    private static let fallbackSources: [String] = [
        "claude", "codex", "cursor", "gemini", "kiro", "copilot",
        "kimi", "antigravity", "zed", "trae", "windsurf", "qoder",
        "codebuddy", "workbuddy", "zcode",
    ]

    // MARK: - Cached Rust data

    private static var cachedSources: [String]?

    /// All provider sources that have subscription/quota tracking (`has_limits: true`).
    /// Loaded from the Rust core; falls back to a hardcoded list during early launch.
    static var allSources: [String] {
        if let cached = cachedSources { return cached }
        load()
        return cachedSources ?? fallbackSources
    }

    /// Default value for the `limitsVisibleSources` UserDefaults key.
    /// Always returns the full set joined by comma so that new providers
    /// are visible by default on first launch.
    static var defaultsValue: String { allSources.joined(separator: ",") }

    // MARK: - Load

    /// Call from `onAppear` to eagerly populate the cache so UI renders without a flash.
    static func load() {
        guard cachedSources == nil else { return }
        ProviderRegistry.shared.loadIfNeeded()
        let hasLimits = ProviderRegistry.shared.allProviders.filter(\.hasLimits)
        if !hasLimits.isEmpty {
            cachedSources = hasLimits.map(\.source)
        }
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
        ProviderRegistry.shared.displayName(for: source)
    }
}
