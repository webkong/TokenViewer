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
    var id: String { "\(source):\(model)" }
    let model: String
    let source: String
    let total_tokens: UInt64
    let total_cost_usd: Double
    let percentage: Double
}

/// Merge ModelEntry list by model name (ignoring source), recalculating percentages.
func mergedByModel(_ entries: [ModelEntry]) -> [ModelEntry] {
    var map: [(String, UInt64, Double)] = [] // (model, tokens, cost)
    var order: [String] = []
    var dict: [String: Int] = [:]
    for e in entries {
        if let idx = dict[e.model] {
            map[idx].1 += e.total_tokens
            map[idx].2 += e.total_cost_usd
        } else {
            dict[e.model] = map.count
            order.append(e.model)
            map.append((e.model, e.total_tokens, e.total_cost_usd))
        }
    }
    let grand = map.reduce(0 as UInt64) { $0 + $1.1 }
    return map.sorted { $0.1 > $1.1 }.map { item in
        ModelEntry(model: item.0, source: entries.first { $0.model == item.0 }?.source ?? "",
                   total_tokens: item.1, total_cost_usd: item.2,
                   percentage: grand > 0 ? Double(item.1) / Double(grand) * 100 : 0)
    }
}

struct SyncResult: Codable {
    let providers_synced: Int
    let records_added: Int
    let errors: [String]
}

struct ProviderStatus: Codable, Identifiable {
    var id: String { source }
    let source: String
    let record_count: Int64
    let installed: Bool
    let last_sync: String?
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
        f.dateFormat = "yyyy-MM-dd'T'HH:mm:ss'Z'"
        f.timeZone = TimeZone(identifier: "UTC")
        return f
    }()

    /// Convert a local calendar date to its UTC ISO start string (e.g. local 06-03 00:00 CST → 2026-06-02T16:00:00Z)
    private static func utcISO(for date: Date) -> String {
        dayFormatter.string(from: date)
    }

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
        let cal = Calendar.current
        let now = Date()
        let todayStart = cal.startOfDay(for: now)
        let tomorrowStart = cal.date(byAdding: .day, value: 1, to: todayStart)!
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd'T'HH:mm:ss'Z'"
        f.timeZone = TimeZone(identifier: "UTC")
        let tomorrow = f.string(from: tomorrowStart)
        func from(daysAgo: Int) -> String {
            f.string(from: cal.date(byAdding: .day, value: -daysAgo, to: todayStart)!)
        }
        func summary(_ fromStr: String) -> UsageSummary? {
            CoreBridge.shared.querySummary(from: fromStr, to: tomorrow)
                .flatMap { try? JSONDecoder().decode(UsageSummary.self, from: $0) }
        }
        let todayS = summary(from(daysAgo: 0))
        let d7 = summary(from(daysAgo: 7))
        let d30 = summary(from(daysAgo: 29))
        let total = summary("2020-01-01T00:00:00Z")
        let isZh = Locale.current.language.languageCode?.identifier.hasPrefix("zh") ?? false
        let langCode = UserDefaults.standard.string(forKey: "appLanguage") ?? "system"
        let zh = langCode == "zh" || (langCode == "system" && isZh)
        return [
            PanelCard(title: zh ? "今天" : "Today", value: tvFormatTokens(todayS?.total_tokens ?? 0), subtitle: tvFormatCost(todayS?.total_cost_usd ?? 0)),
            PanelCard(title: zh ? "7 天" : "7 Days", value: tvFormatTokens(d7?.total_tokens ?? 0), subtitle: "\(d7?.active_days ?? 0) \(zh ? "天活跃" : "active")"),
            PanelCard(title: zh ? "30 天" : "30 Days", value: tvFormatTokens(d30?.total_tokens ?? 0), subtitle: "~\(tvFormatTokens((d30?.total_tokens ?? 0) / 30))\(zh ? "/天" : "/day")"),
            PanelCard(title: zh ? "总计" : "Total", value: tvFormatTokens(total?.total_tokens ?? 0), subtitle: tvFormatCost(total?.total_cost_usd ?? 0)),
        ]
    }

    func sync() {
        guard !isLoading else { return }
        #if DEBUG
        if ProcessInfo.processInfo.environment["TV_SKIP_SYNC"] != nil {
            refresh(); return
        }
        #endif
        isLoading = true
        let startTime = Date()
        Task.detached { [weak self] in
            _ = CoreBridge.shared.syncAll()
            // Ensure at least 1s spinner so user perceives the sync
            let elapsed = Date().timeIntervalSince(startTime)
            if elapsed < 1.0 {
                try? await Task.sleep(nanoseconds: UInt64((1.0 - elapsed) * 1_000_000_000))
            }
            await MainActor.run { [weak self] in
                guard let self else { return }
                self.refresh()
            }
        }
    }

    func rebuildData() {
        guard !isLoading else { return }
        isLoading = true
        let startTime = Date()
        Task.detached { [weak self] in
            _ = CoreBridge.shared.rebuildAll()
            let elapsed = Date().timeIntervalSince(startTime)
            if elapsed < 1.0 {
                try? await Task.sleep(nanoseconds: UInt64((1.0 - elapsed) * 1_000_000_000))
            }
            await MainActor.run { [weak self] in
                guard let self else { return }
                self.refresh()
            }
        }
    }

    private func dateRange(for range: TimeRange) -> (String, String) {
        let cal = Calendar.current
        let todayStart = cal.startOfDay(for: Date())
        let tomorrowStart = cal.date(byAdding: .day, value: 1, to: todayStart)!
        let to = Self.utcISO(for: tomorrowStart)

        let from: String
        switch range {
        case .today:
            from = Self.utcISO(for: todayStart)
        case .week:
            from = Self.utcISO(for: cal.date(byAdding: .day, value: -7, to: todayStart)!)
        case .month:
            from = Self.utcISO(for: cal.date(byAdding: .month, value: -1, to: todayStart)!)
        case .all:
            from = "2020-01-01T00:00:00Z"
        }
        return (from, to)
        // Debug: print("dateRange(\(range)): from=\(from) to=\(to)")
    }
}
