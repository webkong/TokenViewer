import Foundation

@MainActor
final class AppSyncCoordinator {
    static let shared = AppSyncCoordinator()

    private let usageSync: @MainActor () -> Void
    private let limitsRefresh: @MainActor () -> Void

    init(
        usageSync: @escaping @MainActor () -> Void = { UsageViewModel.shared.sync() },
        limitsRefresh: @escaping @MainActor () -> Void = { LimitsViewModel.shared.refresh(force: true, showToast: true) }
    ) {
        self.usageSync = usageSync
        self.limitsRefresh = limitsRefresh
    }

    func syncAll() {
        usageSync()
        limitsRefresh()
    }
}
