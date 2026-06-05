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

    /// Derive the icon source from model name first (overrides the source field
    /// when a model clearly belongs to a different provider, e.g. claude-* used
    /// via Codex CLI should still show the Claude logo).
    private var resolvedSource: String {
        let m = (modelName ?? "").lowercased()
        if m.hasPrefix("claude") { return "claude" }
        if m.hasPrefix("gpt-") || m.hasPrefix("o3") || m.hasPrefix("o4") || m.hasPrefix("codex") { return "codex" }
        if m.hasPrefix("gemini") { return "gemini" }
        if m.hasPrefix("grok") { return "grok" }
        if m.hasPrefix("deepseek") { return "opencode" }  // no dedicated logo, opencode closest
        if m.hasPrefix("kimi") || m.hasPrefix("moonshot") { return "kimi" }
        if m.hasPrefix("qwen") || m.hasPrefix("glm") || m.hasPrefix("minimax") || m.hasPrefix("mimo") { return "opencode" }
        return source
    }

    /// Map a source/model provider to its bundled logo file name (without extension).
    private static let logoMap: [String: String] = [
        "claude": "claude-code", "codebuddy": "codebuddy", "codex": "codex",
        "every-code": "codex", "everycode": "codex", "gemini": "gemini",
        "antigravity": "antigravity", "kiro": "kiro", "kiro-ide": "kiro", "opencode": "opencode",
        "openclaw": "openclaw", "cursor": "cursor", "grok": "grok", "kimi": "kimi",
        "copilot": "copilot", "hermes": "hermes", "kilocli": "kilo", "kilocode": "kilo",
        "qoder": "qoder", "trae": "trae", "windsurf": "windsurf", "zed": "zed",
    ]

    /// Logos drawn with `currentColor` (monochrome) — tint to adapt to light/dark.
    private static let monoLogos: Set<String> = ["copilot", "cursor", "grok", "kimi", "kiro", "kiro-ide"]

    var body: some View {
        if let img = logoImage() {
            let mono = Self.logoMap[resolvedSource.lowercased()].map(Self.monoLogos.contains) ?? false
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
                Circle().fill(TVColor.provider(source))
                Text(resolvedSource.prefix(1).uppercased())
                    .font(.system(size: size * 0.55, weight: .bold))
                    .foregroundStyle(.white)
            }
            .frame(width: size, height: size)
        }
    }

    private func logoImage() -> NSImage? {
        guard let name = Self.logoMap[resolvedSource.lowercased()] else { return nil }
        let url = Bundle.main.url(forResource: name, withExtension: "svg")
            ?? Bundle.main.url(forResource: name, withExtension: "png")
        guard let url, let img = NSImage(contentsOf: url) else { return nil }
        // SVGs load with a 1x1 intrinsic size; give the vector a real size so
        // it rasterizes crisply at Retina scale.
        img.size = NSSize(width: 64, height: 64)
        if Self.monoLogos.contains(name) { img.isTemplate = true }
        return img
    }
}
