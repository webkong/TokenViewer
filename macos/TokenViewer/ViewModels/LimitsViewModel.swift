import Foundation

@MainActor
final class LimitsViewModel: ObservableObject {
    static let shared = LimitsViewModel()

    @Published var providers: [ProviderLimit] = []
    @Published var isLoading = false
    @Published var lastFetched: Date?

    private let cacheKey = "limitsCache"
    private var timer: Timer?

    init() { loadCache() }

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

    func refresh() {
        guard !isLoading else { return }   // ignore while a fetch is in flight
        isLoading = true
        Task { [weak self] in
            let result = await LimitsService.fetchAll()
            await MainActor.run { [weak self] in
                guard let self else { return }
                self.providers = result.sorted { $0.configured && !$1.configured }
                self.lastFetched = Date()
                self.isLoading = false
                self.saveCache()
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
