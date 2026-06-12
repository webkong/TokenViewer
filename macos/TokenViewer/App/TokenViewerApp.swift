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
        rebuildIfVersionChanged()
        ThemeManager.shared.apply()
        statusBarController = StatusBarController()
        UpdateChecker.shared.startAutoCheck()

        if ProcessInfo.processInfo.environment["TV_OPEN_MAIN_WINDOW"] == "1" {
            DispatchQueue.main.async { [weak self] in
                self?.statusBarController?.openMainWindow()
            }
        }
    }

    private func rebuildIfVersionChanged() {
        let currentVersion = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.0.0"
        let lastVersion = UserDefaults.standard.string(forKey: "lastDataVersion")
        if lastVersion != currentVersion {
            _ = CoreBridge.shared.rebuildAll()
            UserDefaults.standard.set(currentVersion, forKey: "lastDataVersion")
            migrateLimitsVisibility()
        }
    }

    private func migrateLimitsVisibility() {
        let key = "limitsVisibleSources"
        guard let existing = UserDefaults.standard.string(forKey: key), !existing.isEmpty else { return }
        let current = Set(existing.split(separator: ",").map { String($0).trimmingCharacters(in: .whitespaces) })
        let all = Set(LimitsVisibilityStore.allSources)
        let missing = all.subtracting(current)
        if !missing.isEmpty {
            let updated = existing + "," + missing.sorted().joined(separator: ",")
            UserDefaults.standard.set(updated, forKey: key)
        }
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        statusBarController?.openMainWindow()
        return true
    }

    func applicationWillTerminate(_ notification: Notification) {
        CoreBridge.shared.shutdown()
    }
}
