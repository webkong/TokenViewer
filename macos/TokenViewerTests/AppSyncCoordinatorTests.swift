import XCTest
@testable import TokenViewer

@MainActor
final class AppSyncCoordinatorTests: XCTestCase {
    func testSyncAllTriggersUsageAndLimits() {
        var usageCalls = 0
        var limitsCalls = 0

        let coordinator = AppSyncCoordinator(
            usageSync: { usageCalls += 1 },
            limitsRefresh: { limitsCalls += 1 }
        )

        coordinator.syncAll()

        XCTAssertEqual(usageCalls, 1)
        XCTAssertEqual(limitsCalls, 1)
    }
}
