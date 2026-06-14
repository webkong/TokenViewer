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

    func testCodexLimitsIncludesSparkAdditionalRateLimits() {
        let json: [String: Any] = [
            "rate_limit": [
                "primary_window": [
                    "limit_window_seconds": 18_000,
                    "used_percent": 12.4,
                    "reset_at": 1_735_690_000,
                ],
                "secondary_window": [
                    "limit_window_seconds": 604_800,
                    "used_percent": 34.6,
                    "reset_at": 1_735_700_000,
                ],
            ],
            "additional_rate_limits": [[
                "limit_name": "codex_spark",
                "rate_limit": [
                    "primary_window": [
                        "used_percent": 56.2,
                        "reset_at": 1_735_710_000,
                    ],
                    "secondary_window": [
                        "limit_window_seconds": 18_000,
                        "used_percent": 78.8,
                        "reset_at": 1_735_720_000,
                    ],
                ],
            ]],
        ]

        let windows = LimitsService.codexLimitWindows(from: json)

        XCTAssertEqual(windows.map(\.label), ["5 Hour", "Weekly", "Spark 5h", "Spark 7d"])
        XCTAssertEqual(windows.map(\.usedPercent), [12, 35, 79, 56])
    }
}
