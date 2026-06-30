import Foundation
import SwiftUI

/// Single source of truth for provider/agent display metadata.
///
/// Loaded from the Rust `tt_skills_list_agents()` FFI which returns the canonical
/// provider config including display names, brand colors, logo filenames, and
/// install status. This replaces the hardcoded `TVColor.sourceDisplayName()`,
/// `TVColor.provider()`, and `ProviderIcon.logoMap` lookups.
///
/// Falls back to generic formatting when the Rust data isn't available yet
/// (early launch or first load).
@MainActor
final class ProviderRegistry {
    static let shared = ProviderRegistry()

    private var providers: [String: SkillProvider] = [:]
    private var loaded = false

    private init() {}

    /// Load provider data from the Rust core via `skillsListAgents()`.
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

    /// Returns all cached providers.
    var allProviders: [SkillProvider] {
        loadIfNeeded()
        return Array(providers.values)
    }

    // MARK: - Lookups

    /// Display name for a provider source, e.g. "claude" → "Claude Code".
    func displayName(for source: String) -> String {
        loadIfNeeded()
        let key = source.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if let provider = providers[key], !provider.displayName.isEmpty {
            return provider.displayName
        }
        // Fallback for aliases
        let resolved = resolveAlias(key)
        if resolved != key, let provider = providers[resolved], !provider.displayName.isEmpty {
            return provider.displayName
        }
        return Self.prettySourceName(resolved)
    }

    /// Brand color hex string, e.g. "#d97757", for a provider source.
    func brandColorHex(for source: String) -> String {
        loadIfNeeded()
        let key = source.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if let provider = providers[key], !provider.brandColor.isEmpty {
            return provider.brandColor
        }
        let resolved = resolveAlias(key)
        if resolved != key, let provider = providers[resolved], !provider.brandColor.isEmpty {
            return provider.brandColor
        }
        return "#059669"
    }

    /// SwiftUI `Color` for a provider source.
    func brandColor(for source: String) -> Color {
        Color(hex: brandColorHex(for: source))
    }

    /// Logo filename (without extension) for a provider source, e.g. "claude-code".
    func logoFile(for source: String) -> String {
        loadIfNeeded()
        let key = source.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if let provider = providers[key], !provider.logoFile.isEmpty {
            return provider.logoFile
        }
        let resolved = resolveAlias(key)
        if resolved != key, let provider = providers[resolved], !provider.logoFile.isEmpty {
            return provider.logoFile
        }
        return resolved
    }

    /// Install status for a provider source.
    func isInstalled(for source: String) -> Bool {
        loadIfNeeded()
        let key = source.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if let provider = providers[key] { return provider.isInstalled }
        return false
    }

    // MARK: - Load

    private func loadFromRust() -> Bool {
        guard let data = CoreBridge.shared.skillsListAgents() else { return false }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let decoded = try? decoder.decode([SkillProvider].self, from: data) else { return false }
        providers = Dictionary(uniqueKeysWithValues: decoded.map { ($0.source, $0) })
        return true
    }

    // MARK: - Alias Resolution

    /// Resolve known source aliases to canonical keys used in the provider registry.
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
        source
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

private extension Color {
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
