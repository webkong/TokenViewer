import SwiftUI

struct QuickHelpModifier: ViewModifier {
    let text: String

    func body(content: Content) -> some View {
        content
            .onHover { hovering in
                if hovering {
                    showTooltip(text)
                } else {
                    hideTooltip()
                }
            }
            .onDisappear {
                hideTooltip()
            }
    }

    private func showTooltip(_ text: String) {
        guard let app = NSApp, app.keyWindow != nil else { return }
        let mouseLocation = NSEvent.mouseLocation

        hideTooltip()

        let tooltipWindow = NSPanel(
            contentRect: .zero,
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        tooltipWindow.isOpaque = false
        tooltipWindow.backgroundColor = .clear
        tooltipWindow.hasShadow = true
        tooltipWindow.level = .popUpMenu
        tooltipWindow.collectionBehavior = [.canJoinAllSpaces, .transient]
        tooltipWindow.ignoresMouseEvents = true

        let hostingView = NSHostingView(rootView: QuickHelpLabel(text: text))
        hostingView.layout()
        let size = hostingView.fittingSize
        let origin = NSPoint(x: mouseLocation.x - size.width / 2, y: mouseLocation.y + 12)
        tooltipWindow.setFrame(NSRect(origin: origin, size: size), display: false)
        tooltipWindow.contentView = hostingView
        tooltipWindow.orderFront(nil)

        objc_setAssociatedObject(
            app,
            &QuickHelpModifier.tooltipKey,
            tooltipWindow,
            .OBJC_ASSOCIATION_RETAIN_NONATOMIC
        )
    }

    private func hideTooltip() {
        guard let app = NSApp else { return }
        if let existing = objc_getAssociatedObject(app, &QuickHelpModifier.tooltipKey) as? NSPanel {
            existing.close()
            objc_setAssociatedObject(app, &QuickHelpModifier.tooltipKey, nil, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        }
    }

    private static var tooltipKey: UInt8 = 0
}

private struct QuickHelpLabel: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.caption2)
            .foregroundStyle(.primary)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 6))
            .overlay(
                RoundedRectangle(cornerRadius: 6)
                    .strokeBorder(.primary.opacity(0.12), lineWidth: 0.5)
            )
            .fixedSize()
    }
}

extension View {
    func quickHelp(_ text: String) -> some View {
        modifier(QuickHelpModifier(text: text))
    }
}
