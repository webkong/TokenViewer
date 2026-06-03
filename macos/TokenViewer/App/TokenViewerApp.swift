import SwiftUI

@main
struct TokenViewerApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate

    var body: some Scene {
        Settings { EmptyView() }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusBarController: StatusBarController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Default sync frequency (30 min) so an unset value isn't read as 0/manual.
        UserDefaults.standard.register(defaults: ["syncFrequencyMinutes": 30])
        // Initialize Rust core early to create database
        _ = CoreBridge.shared
        ThemeManager.shared.apply()
        statusBarController = StatusBarController()
        UpdateChecker.shared.startAutoCheck()
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        statusBarController?.openMainWindow()
        return true
    }

    func applicationWillTerminate(_ notification: Notification) {
        CoreBridge.shared.shutdown()
    }
}
