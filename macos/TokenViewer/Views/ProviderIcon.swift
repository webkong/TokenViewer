import SwiftUI

/// Shared rounded-card background used across dashboard sections.
struct CardBackground: ViewModifier {
    func body(content: Content) -> some View {
        content
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 12)
                    .fill(Color(nsColor: .controlBackgroundColor))
                    .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(.quaternary, lineWidth: 0.5))
            )
    }
}
extension View { func tvCard() -> some View { modifier(CardBackground()) } }

/// Renders a provider/agent brand logo (bundled SVG) with a colored-circle fallback.
struct ProviderIcon: View {
    let source: String
    var modelName: String? = nil
    var size: CGFloat = 16

    private var normalizedSource: String {
        source.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    }

    /// Derive the icon source from model name first (overrides the source field
    /// when a model clearly belongs to a different provider, e.g. claude-* used
    /// via Codex CLI should still show the Claude logo).
    private var resolvedSource: String {
        let m = (modelName ?? "").lowercased()
        let s = normalizedSource
        // Branded products that use another vendor's models under the hood —
        // always show the product's own logo, not the underlying model's.
        // ZCode uses GLM models but should display the ZCode brand.
        if s == "zcode" { return "zcode" }
        if m.hasPrefix("claude") { return "claude" }
        if m.hasPrefix("provider:anthropic") || m == "anthropic" { return "claude" }
        if m.hasPrefix("gpt-") || m.hasPrefix("o1") || m.hasPrefix("o3") || m.hasPrefix("o4") || m.hasPrefix("o5") || m.hasPrefix("codex") || m == "openai" || m.hasPrefix("provider:openai") {
            return "codex"
        }
        if m.hasPrefix("provider:google") { return "gemini" }
        if m.hasPrefix("gemini") { return "gemini" }
        if m.hasPrefix("provider:xai") { return "grok" }
        if m.hasPrefix("grok") { return "grok" }
        if m.contains("deepseek") || s.contains("deepseek") { return "deepseek" }
        if m.hasPrefix("provider:moonshot") { return "kimi" }
        if m.hasPrefix("kimi") || m.hasPrefix("moonshot") { return "kimi" }
        if m.hasPrefix("minimax") || m.hasPrefix("provider:minimax") || s == "minimax" { return "minimax" }
        if m.hasPrefix("qwen") || m.hasPrefix("provider:qwen") || m.hasPrefix("provider:alibaba") || m.hasPrefix("provider:dashscope") { return "qwen" }
        if m.hasPrefix("glm") || m.hasPrefix("chatglm") || m.hasPrefix("provider:zhipu") { return "glm" }
        if m.hasPrefix("mimo") || m.hasPrefix("provider:xiaomi") { return "mimo" }
        // Fall back to the provider registry for non-model-based sources.
        let registryLogo = ProviderRegistry.shared.logoFile(for: s)
        if !registryLogo.isEmpty { return s }
        return normalizedSource
    }

    /// Resolved logo filename (without extension) from the provider registry.
    private var logoName: String {
        ProviderRegistry.shared.logoFile(for: resolvedSource.lowercased())
    }

    /// Logos drawn with `currentColor` (monochrome) — tint to adapt to light/dark.
    private static let monoLogos: Set<String> = ["copilot", "cursor", "grok", "kimi", "kiro", "mimo", "aider"]

    var body: some View {
        let resolved = resolvedSource.lowercased()
        let logo = ProviderRegistry.shared.logoFile(for: resolved)
        if !logo.isEmpty, let img = loadImage(named: logo) {
            let mono = Self.monoLogos.contains(logo)
            Image(nsImage: img)
                .resizable()
                .interpolation(.high)
                .renderingMode(mono ? .template : .original)
                .scaledToFit()
                .foregroundStyle(.primary)
                .frame(width: size, height: size)
        } else {
            // Fallback: colored circle with first letter.
            ZStack {
                Circle().fill(ProviderRegistry.shared.brandColor(for: resolved))
                Text(resolved.prefix(1).uppercased())
                    .font(.system(size: size * 0.55, weight: .bold))
                    .foregroundStyle(.white)
            }
            .frame(width: size, height: size)
        }
    }

    private func loadImage(named name: String) -> NSImage? {
        if let img = NSImage(named: NSImage.Name(name)) {
            if img.size.width <= 1 || img.size.height <= 1 {
                img.size = NSSize(width: 64, height: 64)
            }
            if Self.monoLogos.contains(name) { img.isTemplate = true }
            return img
        }
        for ext in ["svg", "png"] {
            guard let url = Bundle.main.url(forResource: name, withExtension: ext),
                  let img = NSImage(contentsOf: url) else { continue }
            if img.size.width <= 1 || img.size.height <= 1 {
                img.size = NSSize(width: 64, height: 64)
            }
            if Self.monoLogos.contains(name) { img.isTemplate = true }
            return img
        }
        return nil
    }
}
