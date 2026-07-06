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

    func testForcedLimitsRefreshQueuesAnotherFetchWhileLoading() async {
        let firstStarted = expectation(description: "first limits fetch started")
        let probe = LimitsFetchProbe {
            firstStarted.fulfill()
        }
        var toastCount = 0
        let viewModel = LimitsViewModel(
            fetchAll: { await probe.fetch() },
            onRefreshCompleted: { toastCount += 1 },
            cacheKey: "limitsCache.test.\(UUID().uuidString)"
        )

        viewModel.refresh(force: true, showToast: true)
        await fulfillment(of: [firstStarted], timeout: 1)

        viewModel.refresh(force: true, showToast: true)
        let callsBeforeFirstFetchFinishes = await probe.callCount()
        XCTAssertEqual(callsBeforeFirstFetchFinishes, 1)
        XCTAssertEqual(toastCount, 0)

        await probe.finishFirstFetch()
        await waitUntil {
            await probe.callCount() == 2 && !viewModel.isLoading
        }

        let finalForcedCallCount = await probe.callCount()
        XCTAssertEqual(finalForcedCallCount, 2)
        XCTAssertEqual(viewModel.providers.first?.windows.first?.usedPercent, 2)
        XCTAssertEqual(toastCount, 1)
    }

    func testNonForcedLimitsRefreshIsIgnoredWhileLoading() async {
        let firstStarted = expectation(description: "first limits fetch started")
        let probe = LimitsFetchProbe {
            firstStarted.fulfill()
        }
        let viewModel = LimitsViewModel(
            fetchAll: { await probe.fetch() },
            onRefreshCompleted: {},
            cacheKey: "limitsCache.test.\(UUID().uuidString)"
        )

        viewModel.refresh()
        await fulfillment(of: [firstStarted], timeout: 1)

        viewModel.refresh()
        await probe.finishFirstFetch()
        await waitUntil {
            await probe.callCount() == 1 && !viewModel.isLoading
        }

        let finalNonForcedCallCount = await probe.callCount()
        XCTAssertEqual(finalNonForcedCallCount, 1)
        XCTAssertEqual(viewModel.providers.first?.windows.first?.usedPercent, 1)
    }

    private func waitUntil(
        timeout: TimeInterval = 1,
        condition: @escaping () async -> Bool
    ) async {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if await condition() { return }
            try? await Task.sleep(nanoseconds: 10_000_000)
        }
        XCTFail("Timed out waiting for condition")
    }
}

private actor LimitsFetchProbe {
    private var calls = 0
    private var firstContinuation: CheckedContinuation<Void, Never>?
    private let onFirstStarted: @MainActor () -> Void

    init(onFirstStarted: @escaping @MainActor () -> Void) {
        self.onFirstStarted = onFirstStarted
    }

    func fetch() async -> [ProviderLimit] {
        calls += 1
        let currentCall = calls
        if currentCall == 1 {
            await MainActor.run {
                onFirstStarted()
            }
            await withCheckedContinuation { continuation in
                firstContinuation = continuation
            }
        }
        return [
            ProviderLimit(
                name: "codex",
                planLabel: nil,
                configured: true,
                error: nil,
                windows: [
                    LimitWindow(label: "Quota", usedPercent: Double(currentCall), resetAt: nil),
                ]
            ),
        ]
    }

    func finishFirstFetch() {
        firstContinuation?.resume()
        firstContinuation = nil
    }

    func callCount() -> Int {
        calls
    }
}
