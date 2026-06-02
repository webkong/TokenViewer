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
    var size: CGFloat = 16

    /// Map a source/model provider to its bundled logo file name (without extension).
    private static let logoMap: [String: String] = [
        "claude": "claude-code", "codebuddy": "codebuddy", "codex": "codex",
        "every-code": "codex", "everycode": "codex", "gemini": "gemini",
        "antigravity": "antigravity", "kiro": "kiro", "opencode": "opencode",
        "openclaw": "openclaw", "cursor": "cursor", "grok": "grok", "kimi": "kimi",
        "copilot": "copilot", "hermes": "hermes", "kilocli": "kilo", "kilocode": "kilo",
    ]

    /// Logos drawn with `currentColor` (monochrome) — tint to adapt to light/dark.
    private static let monoLogos: Set<String> = ["copilot", "cursor", "grok", "kimi", "kiro"]

    var body: some View {
        if let img = logoImage() {
            let mono = Self.logoMap[source.lowercased()].map(Self.monoLogos.contains) ?? false
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
                Text(source.prefix(1).uppercased())
                    .font(.system(size: size * 0.55, weight: .bold))
                    .foregroundStyle(.white)
            }
            .frame(width: size, height: size)
        }
    }

    private func logoImage() -> NSImage? {
        guard let name = Self.logoMap[source.lowercased()],
              let url = Bundle.main.url(forResource: name, withExtension: "svg", subdirectory: "brand-logos"),
              let img = NSImage(contentsOf: url) else { return nil }
        // SVGs load with a 1x1 intrinsic size; give the vector a real size so
        // it rasterizes crisply at Retina scale.
        img.size = NSSize(width: 64, height: 64)
        if Self.monoLogos.contains(name) { img.isTemplate = true }
        return img
    }
}
