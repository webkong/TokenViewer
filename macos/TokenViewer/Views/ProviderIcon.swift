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
        let s = source.lowercased()
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
        if Self.logoMap[s] != nil { return s }
        return source
    }

    /// Map a source/model provider to its bundled logo file name (without extension).
    private static let logoMap: [String: String] = [
        "claude": "claude-code", "codebuddy": "codebuddy", "codex": "codex",
        "every-code": "codex", "everycode": "codex", "gemini": "gemini",
        "antigravity": "antigravity", "kiro": "kiro", "kiro-ide": "kiro", "opencode": "opencode",
        "openclaw": "openclaw", "cursor": "cursor", "deepseek": "deepseek", "grok": "grok", "kimi": "kimi",
        "minimax": "minimax", "qwen": "qwen", "glm": "glm", "mimo": "mimo",
        "copilot": "copilot", "hermes": "hermes", "kilocli": "kilo", "kilo-cli": "kilo", "kilocode": "kilo",
        "mimocode": "mimo",
        "qoder": "qoder", "trae": "trae", "windsurf": "windsurf", "zed": "zed", "workbuddy": "workbuddy",
    ]

    /// Logos drawn with `currentColor` (monochrome) — tint to adapt to light/dark.
    private static let monoLogos: Set<String> = ["copilot", "cursor", "grok", "kimi", "kiro", "kiro-ide", "mimo", "mimocode"]

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
        for ext in ["svg", "png"] {
            guard let url = Bundle.main.url(forResource: name, withExtension: ext),
                  let img = NSImage(contentsOf: url)
            else { continue }
            // SVGs load with a 1x1 intrinsic size; give the vector a real size so
            // it rasterizes crisply at Retina scale.
            img.size = NSSize(width: 64, height: 64)
            if Self.monoLogos.contains(name) { img.isTemplate = true }
            return img
        }
        return nil
    }
}
