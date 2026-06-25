import SwiftUI

struct MainWindowView: View {
    @ObservedObject private var viewModel = UsageViewModel.shared
    @ObservedObject private var l10n = L10n.shared
    @AppStorage("mainWindowTab") private var mainWindowTab = "usage"

    var body: some View {
        TabView(selection: $mainWindowTab) {
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
        .clearInitialFocus(trigger: mainWindowTab)
        .toastOverlay()
    }
}

@MainActor
final class ToastCenter: ObservableObject {
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

    @Published private(set) var message: Message?

    private var hideTask: Task<Void, Never>?

    func success(_ text: String) {
        show(text, style: .success)
    }

    func error(_ text: String) {
        show(text, style: .error)
    }

    private func show(_ text: String, style: Message.Style) {
        hideTask?.cancel()
        message = Message(text: text, style: style)
        hideTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 1_800_000_000)
            guard !Task.isCancelled else { return }
            await MainActor.run {
                withAnimation(.easeOut(duration: 0.16)) {
                    self?.message = nil
                }
            }
        }
    }
}

enum AppFocus {
    @MainActor
    static func clear() {
        NSApp.keyWindow?.makeFirstResponder(nil)
    }
}

private struct ToastOverlayView: View {
    @ObservedObject private var toast = ToastCenter.shared

    var body: some View {
        ZStack(alignment: .top) {
            if let message = toast.message {
                HStack(spacing: 8) {
                    Image(systemName: message.style == .success ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                        .font(.system(size: 13, weight: .semibold))
                    Text(message.text)
                        .font(.system(size: 13, weight: .medium))
                        .lineLimit(2)
                }
                .foregroundStyle(.white)
                .padding(.horizontal, 12)
                .padding(.vertical, 9)
                .background(
                    RoundedRectangle(cornerRadius: 8)
                        .fill(message.style == .success ? TVColor.brand : Color.red)
                        .shadow(color: Color.black.opacity(0.18), radius: 12, y: 5)
                )
                .padding(.top, 16)
                .transition(.move(edge: .top).combined(with: .opacity))
                .zIndex(1000)
                .id(message.id)
            }
        }
        .animation(.spring(response: 0.24, dampingFraction: 0.86), value: toast.message)
        .allowsHitTesting(false)
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

extension View {
    func clearInitialFocus(trigger: String) -> some View {
        background(ClearInitialFocusView(trigger: trigger).frame(width: 0, height: 0))
    }

    func toastOverlay() -> some View {
        overlay(alignment: .top) {
            ToastOverlayView()
        }
    }
}
