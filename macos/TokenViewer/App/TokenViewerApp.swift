import SwiftUI

@main
struct TokenViewerApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate

    var body: some Scene {
        Settings { EmptyView() }
            .commands {
                CommandGroup(replacing: .appSettings) {
                    Button(L10n.shared.settings) {
                        StatusBarController.shared.openMainWindow(tab: "settings")
                    }
                    .keyboardShortcut(",", modifiers: .command)
                }
            }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    /// Increment when parser semantics change in a way that requires replaying
    /// raw logs to repair already-aggregated usage.
    private static let parserDataRevision = 1
    private var statusBarController: StatusBarController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Dock icon defaults to hidden; menu-bar icon defaults to shown —
        // the app is menu-bar-first, so an unset value should not hide both.
        UserDefaults.standard.register(defaults: [
            "showDockIcon": false,
            "showMenuBarIcon": true,
        ])
        NSApp.setActivationPolicy(UserDefaults.standard.bool(forKey: "showDockIcon") ? .regular : .accessory)
        // Default sync frequency (10 min) so an unset value isn't read as 0/manual.
        UserDefaults.standard.register(defaults: ["syncFrequencyMinutes": 10])
        // Initialize Rust core early to create database
        _ = CoreBridge.shared
        AgentRegistry.shared.loadIfNeeded()
        AgentRegistry.shared.refreshInstallStatus()
        LimitsVisibilityStore.load()
        seedLimitsVisibilityDefaultAfterDetection()
        rebuildIfVersionChanged()
        ThemeManager.shared.apply()
        LiteLLMPricingService.shared.start()
        statusBarController = StatusBarController.shared
        UpdateChecker.shared.startAutoCheck()

        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            if ProcessInfo.processInfo.environment["TV_OPEN_MAIN_WINDOW"] == "1" || NSApp.isActive {
                self.statusBarController?.openMainWindow()
            }
        }
    }

    private func rebuildIfVersionChanged() {
        let currentVersion = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.0.0"
        let lastVersion = UserDefaults.standard.string(forKey: "lastDataVersion")
        let lastParserRevision = UserDefaults.standard.integer(forKey: "lastParserDataRevision")
        if lastVersion != currentVersion || lastParserRevision != Self.parserDataRevision {
            guard let data = CoreBridge.shared.rebuildAll(),
                  let result = try? JSONDecoder().decode(SyncResult.self, from: data),
                  result.errors.isEmpty else {
                return
            }
            UserDefaults.standard.set(currentVersion, forKey: "lastDataVersion")
            UserDefaults.standard.set(Self.parserDataRevision, forKey: "lastParserDataRevision")
            if lastVersion != currentVersion {
                migrateLimitsVisibility()
            }
        }
    }

    /// On first launch (key not yet set), persist the computed default —
    /// core agents + detected-installed agents — once install detection
    /// finishes, so the menu-bar popover reflects real detection results
    /// without requiring the user to open Settings first.
    private func seedLimitsVisibilityDefaultAfterDetection() {
        let key = "limitsVisibleSources"
        guard UserDefaults.standard.string(forKey: key) == nil else { return }
        Task { @MainActor in
            while !AgentRegistry.shared.hasDetectedInstalls {
                try? await Task.sleep(nanoseconds: 100_000_000)
            }
            guard UserDefaults.standard.string(forKey: key) == nil else { return }
            UserDefaults.standard.set(LimitsVisibilityStore.defaultsValue, forKey: key)
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

    func applicationDidBecomeActive(_ notification: Notification) {
        statusBarController?.openMainWindowForAppActivationIfNeeded()
    }

    func applicationWillTerminate(_ notification: Notification) {
        CoreBridge.shared.shutdown()
    }
}
