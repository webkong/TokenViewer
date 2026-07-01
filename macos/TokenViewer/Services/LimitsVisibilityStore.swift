import Foundation

/// Visibility toggles for provider limit cards in the menu bar popover.
/// Sources are loaded dynamically from the Rust core's `has_limits` provider list,
/// keeping the limits panel in sync with the unified agent registry.
@MainActor
enum LimitsVisibilityStore {
    /// All provider sources that have subscription/quota tracking (`has_limits: true`).
    /// Loaded from the Rust core through `ProviderRegistry`.
    static var allSources: [String] {
        ProviderRegistry.shared.limitSources
    }

    /// Sources shown by default in the menu-bar popover: the core agents
    /// (Claude Code, Codex, Gemini, Kiro) plus any other limit-capable agent
    /// detected as installed on this machine (config dir / CLI present).
    static let alwaysOnSources = ["claude", "codex", "gemini", "kiro"]

    /// Default value for the `limitsVisibleSources` UserDefaults key.
    /// Always includes `alwaysOnSources`, plus any other limits-capable agent
    /// that's detected as installed. Falls back to all sources if install
    /// status hasn't been detected yet (e.g. very first read before
    /// `refreshInstallStatus()` completes), so nothing is hidden prematurely.
    static var defaultsValue: String {
        let registry = ProviderRegistry.shared
        let detected = allSources.filter { source in
            alwaysOnSources.contains(source) || registry.isInstalled(for: source)
        }
        // Guard against reading before install status is ready: if detection
        // hasn't found anything beyond the always-on set, and there are more
        // limits-capable sources available, don't prematurely narrow the list.
        if detected.count <= alwaysOnSources.count && allSources.count > alwaysOnSources.count
            && !registry.hasDetectedInstalls {
            return allSources.joined(separator: ",")
        }
        return detected.joined(separator: ",")
    }

    // MARK: - Load

    /// Eagerly load the provider registry before dependent views render.
    static func load() {
        ProviderRegistry.shared.loadIfNeeded()
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
