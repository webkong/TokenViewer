import SwiftUI

/// Shared rounded-card background used across dashboard sections.
struct CardBackground: ViewModifier {
    var height: CGFloat?

    func body(content: Content) -> some View {
        content
            .padding(18)
            .frame(maxWidth: .infinity, minHeight: height, maxHeight: height, alignment: .topLeading)
            .background(
                RoundedRectangle(cornerRadius: 14)
                    .fill(Color(nsColor: .controlBackgroundColor))
                    .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(.quaternary, lineWidth: 0.5))
            )
    }
}
extension View {
    func tvCard(height: CGFloat? = nil) -> some View {
        modifier(CardBackground(height: height))
    }
}

/// Shared in-content tab switcher. It intentionally avoids the native macOS
/// segmented picker so dashboard and settings tabs keep the same pill shape,
/// selected fill, typography, and hit target across OS versions.
struct TVSegmentedPicker<Value: Hashable>: View {
    @Binding var selection: Value
    let options: [(value: Value, title: String)]
    var itemWidth: CGFloat = 96
    var height: CGFloat = 34

    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        HStack(spacing: 0) {
            ForEach(Array(options.enumerated()), id: \.offset) { index, option in
                if index > 0 {
                    Divider()
                        .frame(height: 18)
                        .opacity(separatorVisible(at: index) ? 0.65 : 0)
                }

                Button {
                    withAnimation(.easeOut(duration: 0.16)) {
                        selection = option.value
                    }
                } label: {
                    Text(option.title)
                        .font(.system(size: 13, weight: selection == option.value ? .semibold : .medium))
                        .foregroundStyle(selection == option.value ? Color.white : Color.primary)
                        .frame(width: itemWidth, height: height - 6)
                        .background(
                            RoundedRectangle(cornerRadius: (height - 6) / 2)
                                .fill(selection == option.value ? TVColor.brand : Color.clear)
                        )
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(3)
        .background(
            RoundedRectangle(cornerRadius: height / 2)
                .fill(Color.primary.opacity(colorScheme == .dark ? 0.14 : 0.07))
        )
        .frame(height: height)
        .accessibilityElement(children: .contain)
    }

    private func separatorVisible(at index: Int) -> Bool {
        selection != options[index].value && selection != options[index - 1].value
    }
}

// MARK: - Shared control styles

enum TVActionButtonRole: Equatable {
    case primary
    case secondary
    case warning
    case destructive
}

struct TVActionButtonStyle: ButtonStyle {
    let role: TVActionButtonRole
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(foregroundColor)
            .padding(.horizontal, 13)
            .frame(minHeight: 32)
            .background(
                RoundedRectangle(cornerRadius: 9)
                    .fill(backgroundColor.opacity(configuration.isPressed ? 0.78 : 1))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 9)
                    .strokeBorder(borderColor, lineWidth: role == .secondary ? 0.7 : 0)
            )
            .contentShape(RoundedRectangle(cornerRadius: 9))
            .opacity(isEnabled ? 1 : 0.48)
            .scaleEffect(configuration.isPressed ? 0.98 : 1)
            .animation(.easeOut(duration: 0.12), value: configuration.isPressed)
    }

    private var backgroundColor: Color {
        switch role {
        case .primary: TVColor.brand
        case .secondary: Color(nsColor: .controlBackgroundColor)
        case .warning: Color(red: 0.92, green: 0.35, blue: 0.05) // #EA580C orange-red
        case .destructive: .red
        }
    }

    private var foregroundColor: Color {
        role == .secondary ? .primary : .white
    }

    private var borderColor: Color {
        role == .secondary ? Color.primary.opacity(0.14) : .clear
    }
}

struct TVIconButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(Color.secondary)
            .frame(width: 34, height: 34)
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(Color(nsColor: .controlBackgroundColor).opacity(configuration.isPressed ? 0.72 : 1))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .strokeBorder(Color.primary.opacity(0.12), lineWidth: 0.7)
            )
            .contentShape(RoundedRectangle(cornerRadius: 10))
            .opacity(isEnabled ? 1 : 0.42)
            .scaleEffect(configuration.isPressed ? 0.96 : 1)
    }
}

/// Static chrome for icon-only `Menu` labels, where `ButtonStyle` is not
/// consistently honored by AppKit's menu bridge.
struct TVIconButtonLabel: View {
    let name: String
    var color: Color = .secondary

    var body: some View {
        TVSymbol(name: name, color: color)
            .frame(width: 34, height: 34)
            .background(
                RoundedRectangle(cornerRadius: 10)
                    .fill(Color(nsColor: .controlBackgroundColor))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 10)
                    .strokeBorder(Color.primary.opacity(0.12), lineWidth: 0.7)
            )
            .contentShape(RoundedRectangle(cornerRadius: 10))
    }
}

private struct TVSelectChrome: ViewModifier {
    let width: CGFloat?

    func body(content: Content) -> some View {
        content
            .labelsHidden()
            .buttonStyle(.plain)
            .font(.system(size: 12, weight: .medium))
            .padding(.horizontal, 10)
            .frame(width: width, height: 32)
            .background(
                RoundedRectangle(cornerRadius: 9)
                    .fill(Color(nsColor: .controlBackgroundColor))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 9)
                    .strokeBorder(Color.primary.opacity(0.12), lineWidth: 0.7)
            )
            .overlay(alignment: .trailing) {
                TVSymbol(name: "chevron.down", size: 9, weight: .semibold, color: .secondary)
                    .padding(.trailing, 7)
                    .allowsHitTesting(false)
            }
    }
}

struct TVSymbol: View {
    let name: String
    var size: CGFloat = 13
    var weight: Font.Weight = .semibold
    var color: Color = .secondary

    var body: some View {
        Image(systemName: name)
            .font(.system(size: size, weight: weight))
            .foregroundStyle(color)
            .frame(width: max(16, size + 3), height: max(16, size + 3))
    }
}

extension View {
    func tvActionButton(_ role: TVActionButtonRole = .secondary) -> some View {
        buttonStyle(TVActionButtonStyle(role: role))
    }

    func tvIconButton() -> some View {
        buttonStyle(TVIconButtonStyle())
    }

    func tvSelect(width: CGFloat? = nil) -> some View {
        modifier(TVSelectChrome(width: width))
    }
}

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
