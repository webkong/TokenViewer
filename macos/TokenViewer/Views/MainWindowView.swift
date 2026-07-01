import SwiftUI

@MainActor
final class MainWindowRouter: ObservableObject {
    static let shared = MainWindowRouter()

    @Published var selectedTab = "usage"

    private init() {}
}

struct MainWindowView: View {
    @ObservedObject private var viewModel = UsageViewModel.shared
    @ObservedObject private var l10n = L10n.shared
    @ObservedObject private var router = MainWindowRouter.shared

    var body: some View {
        TabView(selection: $router.selectedTab) {
            UsageView(viewModel: viewModel)
                .tag("usage")
                .tabItem { Label(l10n.usage, systemImage: "chart.bar.fill") }

            SkillManagerView()
                .tag("skills")
                .tabItem { Label(l10n.skills, systemImage: "puzzlepiece.extension.fill") }

            LimitsView(viewModel: LimitsViewModel.shared)
                .tag("limits")
                .tabItem { Label(l10n.limits, systemImage: "gauge.with.dots.needle.50percent") }

            SettingsView()
                .tag("settings")
                .tabItem { Label(l10n.settings, systemImage: "gear") }

            AboutView()
                .tag("about")
                .tabItem { Label(l10n.about, systemImage: "info.circle") }
        }
        .frame(minWidth: 600, minHeight: 480)
        .clearInitialFocus(trigger: router.selectedTab)
        .clearFocusOnOutsideClick()
    }
}

@MainActor
final class ToastCenter {
    static let shared = ToastCenter()

    struct Message: Identifiable, Equatable {
        enum Style: Equatable {
            case success
            case error
        }

        let id = UUID()
        let text: String
        let style: Style
    }

    private var hideTask: Task<Void, Never>?
    private var panel: NSPanel?
    private var hostingView: NSHostingView<GlobalToastView>?
    private var presentationSerial = 0

    func success(_ text: String) {
        show(text, style: .success)
    }

    func error(_ text: String) {
        show(text, style: .error)
    }

    private func show(_ text: String, style: Message.Style) {
        hideTask?.cancel()
        presentationSerial += 1
        let serial = presentationSerial
        render(Message(text: text, style: style))
        hideTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 1_800_000_000)
            guard !Task.isCancelled else { return }
            await MainActor.run {
                self?.hide(serial: serial)
            }
        }
    }

    private func render(_ message: Message) {
        let rootView = GlobalToastView(message: message)
        let hostingView: NSHostingView<GlobalToastView>

        if let currentHostingView = self.hostingView {
            currentHostingView.rootView = rootView
            hostingView = currentHostingView
        } else {
            hostingView = NSHostingView(rootView: rootView)
            hostingView.translatesAutoresizingMaskIntoConstraints = false
            hostingView.wantsLayer = true
            hostingView.layer?.backgroundColor = NSColor.clear.cgColor
            self.hostingView = hostingView
        }

        let panel = self.panel ?? makePanel()
        if panel.contentView !== hostingView {
            panel.contentView = hostingView
        }
        self.panel = panel

        let fittingSize = hostingView.fittingSize
        let size = NSSize(width: max(fittingSize.width, 1), height: max(fittingSize.height, 1))
        let finalFrame = frame(for: size)

        if panel.isVisible {
            panel.alphaValue = 1
            panel.setFrame(finalFrame, display: true)
        } else {
            panel.alphaValue = 0
            panel.setFrame(finalFrame.offsetBy(dx: 0, dy: 12), display: true)
            panel.orderFrontRegardless()

            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.18
                context.timingFunction = CAMediaTimingFunction(name: .easeOut)
                panel.animator().alphaValue = 1
                panel.animator().setFrame(finalFrame, display: true)
            }
        }
    }

    private func makePanel() -> NSPanel {
        let panel = NSPanel(
            contentRect: .zero,
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.contentView?.wantsLayer = true
        panel.contentView?.layer?.backgroundColor = NSColor.clear.cgColor
        panel.hasShadow = false
        panel.ignoresMouseEvents = true
        panel.hidesOnDeactivate = false
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.canJoinAllSpaces, .transient, .fullScreenAuxiliary]
        panel.level = .popUpMenu
        return panel
    }

    private func frame(for size: NSSize) -> NSRect {
        let frame = targetFrame()
        let origin = NSPoint(
            x: frame.midX - size.width / 2,
            y: frame.maxY - size.height - 16
        )
        return NSRect(origin: origin, size: size)
    }

    private func targetFrame() -> NSRect {
        if let window = NSApp.keyWindow ?? NSApp.mainWindow {
            return window.frame
        }
        if let window = NSApp.windows.first(where: { $0.isVisible && !($0 is NSPanel) }) {
            return window.frame
        }
        return NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 800, height: 600)
    }

    private func hide(serial: Int) {
        guard serial == presentationSerial, let panel, panel.isVisible else { return }
        let hiddenFrame = panel.frame.offsetBy(dx: 0, dy: 12)

        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.16
            context.timingFunction = CAMediaTimingFunction(name: .easeIn)
            panel.animator().alphaValue = 0
            panel.animator().setFrame(hiddenFrame, display: true)
        } completionHandler: { [weak self, weak panel] in
            Task { @MainActor in
                guard serial == self?.presentationSerial else { return }
                panel?.orderOut(nil)
            }
        }
    }
}

private struct GlobalToastView: View {
    let message: ToastCenter.Message

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: message.style == .success ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                .font(.system(size: 13, weight: .semibold))
            Text(message.text)
                .font(.system(size: 13, weight: .medium))
                .lineLimit(1)
                .layoutPriority(1)
        }
        .foregroundStyle(.white)
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(message.style == .success ? TVColor.brand : Color.red)
        )
        .background(Color.clear)
        .fixedSize(horizontal: true, vertical: true)
    }
}

enum AppFocus {
    @MainActor
    static func clear() {
        NSApp.keyWindow?.makeFirstResponder(nil)
    }
}

private struct ClearInitialFocusView: NSViewRepresentable {
    let trigger: String

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> NSView {
        NSView(frame: .zero)
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        guard context.coordinator.lastTrigger != trigger else { return }
        context.coordinator.lastTrigger = trigger

        clearFocus(in: nsView, delay: 0)
        clearFocus(in: nsView, delay: 0.08)
    }

    private func clearFocus(in view: NSView, delay: TimeInterval) {
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) {
            view.window?.makeFirstResponder(nil)
        }
    }

    final class Coordinator {
        var lastTrigger: String?
    }
}

private struct ClearFocusOnOutsideClickView: NSViewRepresentable {
    func makeNSView(context: Context) -> FocusDismissHostView {
        FocusDismissHostView()
    }

    func updateNSView(_ nsView: FocusDismissHostView, context: Context) {
        nsView.refreshMonitor()
    }

    final class FocusDismissHostView: NSView {
        private var monitor: Any?
        private weak var monitoredWindow: NSWindow?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            refreshMonitor()
        }

        deinit {
            removeMonitor()
        }

        func refreshMonitor() {
            guard monitoredWindow !== window else { return }
            removeMonitor()
            monitoredWindow = window
            guard let window else { return }

            monitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown]) { [weak self, weak window] event in
                guard let self, let window, event.window === window else { return event }
                guard !self.clickedTextInput(event, in: window) else { return event }
                window.makeFirstResponder(nil)
                return event
            }
        }

        private func removeMonitor() {
            if let monitor {
                NSEvent.removeMonitor(monitor)
                self.monitor = nil
            }
            monitoredWindow = nil
        }

        private func clickedTextInput(_ event: NSEvent, in window: NSWindow) -> Bool {
            guard let contentView = window.contentView else { return false }
            let point = contentView.convert(event.locationInWindow, from: nil)
            guard let hitView = contentView.hitTest(point) else { return false }
            return hasTextInputAncestor(hitView)
        }

        private func hasTextInputAncestor(_ view: NSView) -> Bool {
            var current: NSView? = view
            while let candidate = current {
                if candidate is NSTextField || candidate is NSTextView {
                    return true
                }
                current = candidate.superview
            }
            return false
        }
    }
}

extension View {
    func clearInitialFocus(trigger: String) -> some View {
        background(ClearInitialFocusView(trigger: trigger).frame(width: 0, height: 0))
    }

    func clearFocusOnOutsideClick() -> some View {
        background(ClearFocusOnOutsideClickView().frame(width: 0, height: 0))
    }
}
