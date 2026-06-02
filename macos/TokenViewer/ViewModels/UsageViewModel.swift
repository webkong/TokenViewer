import Foundation

struct UsageSummary: Codable {
    let total_tokens: UInt64
    let total_cost_usd: Double
    let input_tokens: UInt64
    let output_tokens: UInt64
    let cached_input_tokens: UInt64
    let reasoning_output_tokens: UInt64
    let conversation_count: UInt32
    let active_days: UInt32
}

struct HeatmapPoint: Codable, Identifiable {
    var id: String { date }
    let date: String
    let count: UInt64
    let level: UInt8   // 0-4
}

/// A summary card for the menu-bar panel (Today / 7D / 30D / Total).
struct PanelCard: Identifiable {
    let id = UUID()
    let title: String
    let value: String
    let subtitle: String
}

struct DailyPoint: Codable, Identifiable {
    var id: String { date }
    let date: String
    let total_tokens: UInt64
    let total_cost_usd: Double
    let input_tokens: UInt64
    let output_tokens: UInt64
    let cached_input_tokens: UInt64
    let cache_creation_input_tokens: UInt64
    let reasoning_output_tokens: UInt64
    let conversation_count: UInt32
}

struct ModelEntry: Codable, Identifiable {
    var id: String { model }
    let model: String
    let source: String
    let total_tokens: UInt64
    let total_cost_usd: Double
    let percentage: Double
}

struct SyncResult: Codable {
    let providers_synced: Int
    let records_added: Int
    let errors: [String]
}

struct ProviderStatus: Codable, Identifiable {
    var id: String { name }
    let name: String
    let record_count: Int64
    let status: String
}

@MainActor
class UsageViewModel: ObservableObject {
    /// Shared instance so popover and main window stay in sync (H3).
    static let shared = UsageViewModel()

    @Published var summary: UsageSummary?
    @Published var dailyUsage: [DailyPoint] = []
    @Published var modelBreakdown: [ModelEntry] = []
    @Published var heatmap: [HeatmapPoint] = []
    @Published var panelCards: [PanelCard] = []
    @Published var isLoading = false
    @Published var selectedRange: TimeRange = .week

    enum TimeRange: String, CaseIterable {
        case today = "Today"
        case week = "Week"
        case month = "Month"
        case all = "All"
    }

    private let decoder = JSONDecoder()

    private static let dayFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        f.timeZone = TimeZone(identifier: "UTC")
        return f
    }()

    init() {
        // Auto-sync on first load
        sync()
        startAutoSync()
    }

    private var syncTimer: Timer?

    /// (Re)start the local-sync timer from the user's syncFrequencyMinutes
    /// setting. 0 = manual only. Local parse is cheap, so a short interval is fine.
    func startAutoSync() {
        syncTimer?.invalidate()
        syncTimer = nil
        let minutes = UserDefaults.standard.integer(forKey: "syncFrequencyMinutes")
        guard minutes > 0 else { return }
        syncTimer = Timer.scheduledTimer(withTimeInterval: Double(minutes) * 60, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.sync() }
        }
    }

    /// Query the database off the main thread, then publish on the main actor (H7).
    func refresh() {
        isLoading = true
        let (from, to) = dateRange(for: selectedRange)
        let useHourly = selectedRange == .today
        Task.detached { [weak self] in
            let summaryData = CoreBridge.shared.querySummary(from: from, to: to)
            let dailyData = useHourly
                ? CoreBridge.shared.queryHourly(from: from, to: to)
                : CoreBridge.shared.queryDaily(from: from, to: to)
            let modelData = CoreBridge.shared.queryModelBreakdown(from: from, to: to)
            let heatmapData = CoreBridge.shared.queryHeatmap(weeks: 53)
            let cards = Self.fetchPanelCards()
            await MainActor.run { [weak self] in
                guard let self else { return }
                if let d = summaryData { self.summary = try? self.decoder.decode(UsageSummary.self, from: d) }
                if let d = dailyData { self.dailyUsage = (try? self.decoder.decode([DailyPoint].self, from: d)) ?? [] }
                if let d = modelData { self.modelBreakdown = (try? self.decoder.decode([ModelEntry].self, from: d)) ?? [] }
                if let d = heatmapData { self.heatmap = (try? self.decoder.decode([HeatmapPoint].self, from: d)) ?? [] }
                self.panelCards = cards
                self.isLoading = false
            }
        }
    }

    /// Build the 4 period summary cards (Today / 7D / 30D / Total) for the menu-bar panel.
    nonisolated private static func fetchPanelCards() -> [PanelCard] {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        f.timeZone = TimeZone(identifier: "UTC")
        let today = Date()
        let cal = Calendar.current
        let tomorrow = f.string(from: cal.date(byAdding: .day, value: 1, to: today)!) + "T00:00:00Z"
        func from(daysAgo: Int) -> String {
            f.string(from: cal.date(byAdding: .day, value: -daysAgo, to: today)!) + "T00:00:00Z"
        }
        func summary(_ fromStr: String) -> UsageSummary? {
            CoreBridge.shared.querySummary(from: fromStr, to: tomorrow)
                .flatMap { try? JSONDecoder().decode(UsageSummary.self, from: $0) }
        }
        let todayS = summary(from(daysAgo: 0))
        let d7 = summary(from(daysAgo: 7))
        let d30 = summary(from(daysAgo: 29))
        let total = summary("2020-01-01T00:00:00Z")
        return [
            PanelCard(title: "Today", value: tvFormatTokens(todayS?.total_tokens ?? 0), subtitle: tvFormatCost(todayS?.total_cost_usd ?? 0)),
            PanelCard(title: "7 Days", value: tvFormatTokens(d7?.total_tokens ?? 0), subtitle: "\(d7?.active_days ?? 0) active"),
            PanelCard(title: "30 Days", value: tvFormatTokens(d30?.total_tokens ?? 0), subtitle: "~\(tvFormatTokens((d30?.total_tokens ?? 0) / 30))/day"),
            PanelCard(title: "Total", value: tvFormatTokens(total?.total_tokens ?? 0), subtitle: tvFormatCost(total?.total_cost_usd ?? 0)),
        ]
    }

    func sync() {
        isLoading = true
        Task.detached { [weak self] in
            let result = CoreBridge.shared.syncAll()
            await MainActor.run { [weak self] in
                guard let self else { return }
                if result != nil {
                    self.refresh()
                } else {
                    self.isLoading = false
                }
            }
        }
    }

    private func dateRange(for range: TimeRange) -> (String, String) {
        let f = Self.dayFormatter
        let today = Date()
        // 'to' must be tomorrow to include today's data (ISO comparison)
        let tomorrow = Calendar.current.date(byAdding: .day, value: 1, to: today)!
        let to = f.string(from: tomorrow) + "T00:00:00Z"

        let from: String
        switch range {
        case .today:
            from = f.string(from: today) + "T00:00:00Z"
        case .week:
            from = f.string(from: Calendar.current.date(byAdding: .day, value: -7, to: today)!) + "T00:00:00Z"
        case .month:
            from = f.string(from: Calendar.current.date(byAdding: .month, value: -1, to: today)!) + "T00:00:00Z"
        case .all:
            from = "2020-01-01T00:00:00Z"
        }
        return (from, to)
    }
}
