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

/// Renders a coding-agent product logo.
struct AgentIcon: View {
    let source: String
    var size: CGFloat = 16

    var body: some View {
        let key = source.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        BrandIcon(
            key: key,
            logoName: AgentRegistry.shared.logoFile(for: key),
            color: AgentRegistry.shared.brandColor(for: key),
            size: size
        )
    }
}

/// Renders the vendor of a model, falling back to the agent product when the
/// model vendor is unknown.
struct ModelProviderIcon: View {
    let model: String
    let fallbackAgentSource: String
    var size: CGFloat = 16

    var body: some View {
        if let provider = ModelProviderRegistry.resolve(model: model) {
            BrandIcon(
                key: provider.id,
                logoName: provider.logoFile,
                color: Color(hex: provider.brandColor),
                size: size
            )
        } else {
            AgentIcon(source: fallbackAgentSource, size: size)
        }
    }
}

private struct BrandIcon: View {
    let key: String
    let logoName: String
    let color: Color
    let size: CGFloat

    private static let monoLogos: Set<String> = ["copilot", "cursor", "grok", "kimi", "kiro", "mimo", "aider", "yi", "perplexity", "hy3", "roocode", "longcat", "hunyuan", "chatgpt"]

    var body: some View {
        if !logoName.isEmpty, let img = loadImage(named: logoName) {
            let mono = Self.monoLogos.contains(logoName)
            Image(nsImage: img)
                .resizable()
                .interpolation(.high)
                .renderingMode(mono ? .template : .original)
                .scaledToFit()
                .foregroundStyle(.primary)
                .frame(width: size, height: size)
        } else {
            ZStack {
                Circle().fill(color)
                Text(key.prefix(1).uppercased())
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
