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
    /// True until the initial range auto-selection (today vs yesterday, based on
    /// whether today has any data yet) has run once. Prevents that one-time
    /// auto-selection from overriding a range the user has since chosen manually.
    private var hasAppliedDefaultRange = false
    /// Custom range bounds (local calendar days), used when selectedRange == .custom.
    @Published var customFrom: Date = AppTime.localCalendar.date(byAdding: .day, value: -29, to: AppTime.localStartOfDay(for: Date())) ?? Date()
    @Published var customTo: Date = Date()

    enum TimeRange: String, CaseIterable {
        case today = "Today"
        case yesterday = "Yesterday"
        case week = "Week"
        case month = "Month"
        case all = "All"
        case custom = "Custom"

        var localizedTitle: String {
            let l = L10n.shared
            switch self {
            case .today: return l.rangeToday
            case .yesterday: return l.rangeYesterday
            case .week: return l.rangeWeek
            case .month: return l.rangeMonth
            case .all: return l.rangeAll
            case .custom: return l.rangeCustom
            }
        }
    }

    /// Use the hourly trend granularity for single-day windows.
    var isHourlyView: Bool {
        selectedRange == .today
            || selectedRange == .yesterday
            || (selectedRange == .custom && AppTime.isSameLocalDay(customFrom, customTo))
    }

    private let decoder = JSONDecoder()

    init() {
        // Auto-sync on first load
        sync()
        startAutoSync()
    }

    private var syncTimer: Timer?

    /// Timestamp of the last sync that actually ran. Used by `syncIfStale()`
    /// to throttle the implicit sync triggered when the menu panel opens, so
    /// rapidly reopening the panel doesn't re-parse on every open.
    private var lastSyncedAt: Date?
    /// Minimum gap between panel-open auto-syncs. The manual refresh button
    /// bypasses this (calls `sync()` directly).
    private static let syncStaleInterval: TimeInterval = 60

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
    /// A generation token guards against out-of-order results when the range is
    /// switched rapidly (e.g. week→month→week): only the latest refresh applies.
    private var refreshToken = 0
    func refresh() {
        isLoading = true
        refreshToken &+= 1
        let token = refreshToken
        let needsDefaultRangeCheck = !hasAppliedDefaultRange
        Task.detached { [weak self] in
            // One-time default range selection: prefer "Today" once today has any
            // data, otherwise fall back to "Yesterday" so the dashboard doesn't
            // open on an empty day right after midnight. Runs once per app launch,
            // resolved before the main query so we never fetch a range we're about
            // to discard, and never overrides a range the user has since picked.
            var resolvedDefaultRange: UsageViewModel.TimeRange?
            if needsDefaultRangeCheck {
                let todayRange = AppTime.trailingLocalDays(1)
                let todayData = CoreBridge.shared.querySummary(from: todayRange.from, to: todayRange.to)
                let todaySummary = todayData.flatMap { try? JSONDecoder().decode(UsageSummary.self, from: $0) }
                let hasData = (todaySummary?.total_tokens ?? 0) > 0
                resolvedDefaultRange = hasData ? .today : .yesterday
            }

            let (_, from, to, useHourly) = await MainActor.run { [weak self] () -> (UsageViewModel.TimeRange, String, String, Bool) in
                guard let self else { return (.week, "", "", false) }
                if let defaultRange = resolvedDefaultRange, !self.hasAppliedDefaultRange {
                    self.hasAppliedDefaultRange = true
                    self.selectedRange = defaultRange
                }
                let range = self.selectedRange
                let (from, to) = self.dateRange(for: range)
                return (range, from, to, self.isHourlyView)
            }

            let summaryData = CoreBridge.shared.querySummary(from: from, to: to)
            let dailyData = useHourly
                ? CoreBridge.shared.queryHourly(from: from, to: to)
                : CoreBridge.shared.queryDaily(from: from, to: to)
            let modelData = CoreBridge.shared.queryModelBreakdown(from: from, to: to)
            let heatmapData = CoreBridge.shared.queryHeatmap(weeks: 53)
            let cards = Self.fetchPanelCards()
            await MainActor.run { [weak self] in
                guard let self else { return }
                // Discard stale results from a superseded refresh.
                guard token == self.refreshToken else { return }
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
        let now = Date()
        func summary(_ range: UsageQueryRange) -> UsageSummary? {
            CoreBridge.shared.querySummary(from: range.from, to: range.to)
                .flatMap { try? JSONDecoder().decode(UsageSummary.self, from: $0) }
        }
        let todayS = summary(AppTime.trailingLocalDays(1, now: now))
        let d7 = summary(AppTime.trailingLocalDays(7, now: now))
        let d30 = summary(AppTime.trailingLocalDays(30, now: now))
        let total = summary(AppTime.allUsage(through: now))
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

    /// Sync only if the last sync is older than `syncStaleInterval`. Used on
    /// panel open so frequent reopens don't keep pulling. The manual refresh
    /// button calls `sync()` directly to force a pull regardless.
    func syncIfStale() {
        let stale = lastSyncedAt.map { Date().timeIntervalSince($0) > Self.syncStaleInterval } ?? true
        if stale { sync() }
    }

    func sync() {
        guard !isLoading else { return }
        #if DEBUG
        if ProcessInfo.processInfo.environment["TV_SKIP_SYNC"] != nil {
            refresh(); return
        }
        #endif
        isLoading = true
        lastSyncedAt = Date()
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
        let queryRange: UsageQueryRange
        switch range {
        case .today:
            queryRange = AppTime.trailingLocalDays(1)
        case .yesterday:
            queryRange = AppTime.yesterdayLocalDay()
        case .week:
            queryRange = AppTime.trailingLocalDays(7)
        case .month:
            queryRange = AppTime.trailingLocalDays(30)
        case .all:
            queryRange = AppTime.allUsage()
        case .custom:
            queryRange = AppTime.inclusiveLocalDays(from: customFrom, through: customTo)
        }
        return (queryRange.from, queryRange.to)
    }
}
