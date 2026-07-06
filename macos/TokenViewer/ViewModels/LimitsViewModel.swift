import Foundation

@MainActor
final class LimitsViewModel: ObservableObject {
    static let shared = LimitsViewModel()

    @Published var providers: [ProviderLimit] = []
    @Published var isLoading = false
    @Published var lastFetched: Date?

    private let fetchAll: () async -> [ProviderLimit]
    private let onRefreshCompleted: @MainActor () -> Void
    private let cacheKey: String
    private var timer: Timer?
    private var refreshAgainAfterCurrentFetch = false

    init(
        fetchAll: @escaping () async -> [ProviderLimit] = { await LimitsService.fetchAll() },
        onRefreshCompleted: @escaping @MainActor () -> Void = {
            ToastCenter.shared.success(L10n.shared.toastRefreshed)
        },
        cacheKey: String = "limitsCache"
    ) {
        self.fetchAll = fetchAll
        self.onRefreshCompleted = onRefreshCompleted
        self.cacheKey = cacheKey
        loadCache()
    }

    /// Called when the page appears: show cache, refresh if stale, then poll
    /// every 10 min while the page stays visible (network-frugal).
    func startAutoRefresh() {
        refreshIfStale()
        timer?.invalidate()
        timer = Timer.scheduledTimer(withTimeInterval: 600, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refresh() }
        }
    }

    /// Refresh only if cached data is older than 30s (used by panel + page entry).
    func refreshIfStale() {
        let stale = lastFetched.map { Date().timeIntervalSince($0) > 30 } ?? true
        if stale { refresh() }
    }

    func stopAutoRefresh() {
        timer?.invalidate()
        timer = nil
    }

    func refresh(force: Bool = false, showToast: Bool = false) {
        guard !isLoading else {
            if force {
                refreshAgainAfterCurrentFetch = true
            }
            return
        }
        isLoading = true
        Task { [weak self] in
            guard let self else { return }
            let result = await self.fetchAll()
            await MainActor.run { [weak self] in
                guard let self else { return }
                self.providers = result.sorted { $0.configured && !$1.configured }
                self.lastFetched = Date()
                self.isLoading = false
                self.saveCache()
                if self.refreshAgainAfterCurrentFetch {
                    self.refreshAgainAfterCurrentFetch = false
                    self.refresh(force: true, showToast: showToast)
                } else if showToast {
                    self.onRefreshCompleted()
                }
            }
        }
    }

    private func saveCache() {
        if let data = try? JSONEncoder().encode(providers) {
            UserDefaults.standard.set(data, forKey: cacheKey)
        }
    }

    private func loadCache() {
        guard let data = UserDefaults.standard.data(forKey: cacheKey),
              let decoded = try? JSONDecoder().decode([ProviderLimit].self, from: data) else { return }
        providers = decoded
    }
}
