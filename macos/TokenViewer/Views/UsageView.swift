import SwiftUI

/// Brand color constant only — display names and agent colors come from
/// `AgentRegistry.shared`.
enum TVColor {
    static let brand = Color(red: 0.02, green: 0.59, blue: 0.41) // #059669 emerald
}

func tvFormatTokens(_ n: UInt64) -> String {
    let d = Double(n)
    if d >= 1_000_000_000 { return String(format: "%.2fB", d / 1_000_000_000) }
    if d >= 1_000_000 { return String(format: "%.2fM", d / 1_000_000) }
    if d >= 1_000 { return String(format: "%.1fK", d / 1_000) }
    return "\(n)"
}

func tvFormatCost(_ usd: Double) -> String {
    let code = UserDefaults.standard.string(forKey: "currency") ?? "USD"
    let rate = code == "USD" ? 1.0 : UserDefaults.standard.double(forKey: "currencyRate").nonZeroOr(1.0)
    let symbol: String
    switch code {
    case "CNY", "JPY": symbol = "¥"
    case "EUR": symbol = "€"
    case "GBP": symbol = "£"
    case "KRW": symbol = "₩"
    default: symbol = "$"
    }
    let v = usd * rate
    if v <= 0 { return "\(symbol)0.00" }
    if v < 0.01 { return "<\(symbol)0.01" }
    if v >= 1000 { return String(format: "%@%.0f", symbol, v) }
    return String(format: "%@%.2f", symbol, v)
}

private extension Double {
    func nonZeroOr(_ fallback: Double) -> Double { self == 0 ? fallback : self }
}

struct UsageView: View {
    @ObservedObject var viewModel: UsageViewModel
    @ObservedObject private var l10n = L10n.shared
    @ObservedObject private var currency = CurrencyStore.shared

    var body: some View {
        GeometryReader { geo in
            let wide = geo.size.width >= 760
            ScrollView(showsIndicators: false) {
                VStack(alignment: .leading, spacing: 20) {
                    header

                    rangeSelector

                    if let s = viewModel.summary {
                        dashboardModules(summary: s, width: geo.size.width - 40, wide: wide)
                    } else {
                        ProgressView().frame(maxWidth: .infinity, minHeight: 200)
                    }
                }
                .padding(20)
            }
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .onAppear { viewModel.sync() }
    }

    @ViewBuilder
    private func dashboardModules(summary: UsageSummary, width: CGFloat, wide: Bool) -> some View {
        if wide {
            let side = (width - 16) * 0.30
            let main = width - 16 - side
            HStack(alignment: .top, spacing: 16) {
                SummaryCardsView(summary: summary, models: viewModel.modelBreakdown, compact: true)
                    .frame(width: main)
                TokenTypeBar(summary: summary, cardHeight: 130)
                    .frame(width: side)
            }

            HStack(alignment: .top, spacing: 16) {
                if !viewModel.dailyUsage.isEmpty {
                    trendChart(cardHeight: 330).frame(width: main)
                }
                if !viewModel.modelBreakdown.isEmpty {
                    AgentBreakdownView(models: viewModel.modelBreakdown, compact: true, cardHeight: 330)
                        .frame(width: side)
                }
            }

            HStack(alignment: .top, spacing: 16) {
                if !viewModel.heatmap.isEmpty {
                    HeatmapView(points: viewModel.heatmap, availableWidth: main - 36, cardHeight: 250)
                        .frame(width: main)
                }
                if !viewModel.modelBreakdown.isEmpty {
                    ModelBreakdownView(
                        models: viewModel.modelBreakdown,
                        limit: 4,
                        compact: true,
                        cardHeight: 250
                    )
                    .frame(width: side)
                }
            }
            if !viewModel.allDailyUsage.isEmpty || !viewModel.projectUsage.isEmpty {
                UsageDetailsView(daily: viewModel.allDailyUsage, projects: viewModel.projectUsage)
            }
        } else {
            SummaryCardsView(summary: summary, models: viewModel.modelBreakdown)
            TokenTypeBar(summary: summary)
            if !viewModel.dailyUsage.isEmpty { trendChart() }
            if !viewModel.modelBreakdown.isEmpty {
                AgentBreakdownView(models: viewModel.modelBreakdown)
                ModelBreakdownView(models: viewModel.modelBreakdown)
            }
            if !viewModel.heatmap.isEmpty {
                HeatmapView(points: viewModel.heatmap, availableWidth: width - 36)
            }
            if !viewModel.allDailyUsage.isEmpty || !viewModel.projectUsage.isEmpty {
                UsageDetailsView(daily: viewModel.allDailyUsage, projects: viewModel.projectUsage)
            }
        }
    }

    private func trendChart(cardHeight: CGFloat? = nil) -> some View {
        TrendChartView(
            data: viewModel.dailyUsage,
            hourly: viewModel.isHourlyView,
            selectedAgent: $viewModel.selectedTrendAgent,
            agents: viewModel.trendAgents,
            cardHeight: cardHeight
        )
        .onChange(of: viewModel.selectedTrendAgent) { viewModel.refresh() }
    }

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(l10n.usageTitle)
                    .font(.system(size: 26, weight: .bold))
                Text(l10n.usageSubtitle)
                    .font(.system(size: 13))
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button(action: {
                AppSyncCoordinator.shared.syncAll()
                ToastCenter.shared.success(l10n.toastSynced)
            }) {
                Image(systemName: "arrow.triangle.2.circlepath")
                    .font(.system(size: 13, weight: .semibold))
                    .rotationEffect(.degrees(viewModel.isLoading ? 360 : 0))
                    .animation(viewModel.isLoading ? .linear(duration: 1).repeatForever(autoreverses: false) : .default, value: viewModel.isLoading)
            }
            .buttonStyle(.borderless)
            .help(l10n.syncNow)
        }
    }

    private var rangeSelector: some View {
        HStack(alignment: .center, spacing: 10) {
            Picker("Range", selection: $viewModel.selectedRange) {
                ForEach(UsageViewModel.TimeRange.allCases, id: \.self) { range in
                    Text(range.localizedTitle).tag(range)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .controlSize(.large)
            .frame(maxWidth: 560)
            .onChange(of: viewModel.selectedRange) { viewModel.refresh() }

            if viewModel.selectedRange == .custom {
                CustomRangePicker(
                    from: $viewModel.customFrom,
                    to: $viewModel.customTo,
                    onApply: viewModel.refresh
                )
                    .transition(.opacity.combined(with: .move(edge: .leading)))
            }

            Spacer(minLength: 0)
        }
        .animation(.easeInOut(duration: 0.18), value: viewModel.selectedRange)
    }
}

private struct CustomRangePicker: View {
    @Binding var from: Date
    @Binding var to: Date
    let onApply: () -> Void
    @ObservedObject private var l10n = L10n.shared
    @State private var isPresented = false
    @State private var visibleMonth = Date()
    @State private var draftFrom = Date()
    @State private var draftTo = Date()
    @State private var selectingEnd = false

    private var calendar: Calendar { AppTime.localCalendar }
    private let columns = Array(repeating: GridItem(.flexible(), spacing: 4), count: 7)

    var body: some View {
        Button {
            prepareSelection()
            isPresented = true
        } label: {
            HStack(spacing: 7) {
                Image(systemName: "calendar.badge.clock").foregroundStyle(TVColor.brand)
                Text(rangeLabel)
                    .font(.system(size: 12, weight: .medium))
                    .monospacedDigit()
                Image(systemName: "chevron.down")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.tertiary)
            }
            .padding(.horizontal, 11)
            .frame(height: 30)
            .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(.quaternary, lineWidth: 0.5))
        }
        .buttonStyle(.plain)
        .popover(isPresented: $isPresented, arrowEdge: .top) {
            VStack(alignment: .leading, spacing: 12) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(l10n.rangeSelectTitle)
                        .font(.system(size: 14, weight: .semibold))
                    Text(l10n.rangeSelectHint)
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                }

                calendarHeader
                weekdayHeader
                calendarGrid

                Divider()

                HStack {
                    Text(draftRangeLabel)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button(l10n.cancel) { isPresented = false }
                    Button(l10n.apply) { applySelection() }
                        .buttonStyle(.borderedProminent)
                        .tint(TVColor.brand)
                }
            }
            .padding(16)
            .frame(width: 340)
        }
    }

    private var rangeLabel: String { "\(shortDate(from)) – \(shortDate(to))" }

    private var draftRangeLabel: String {
        "\(shortDate(draftFrom)) – \(shortDate(draftTo))"
    }

    private var calendarHeader: some View {
        HStack {
            Button { moveMonth(by: -1) } label: {
                Image(systemName: "chevron.left")
            }
            .buttonStyle(.plain)

            Spacer()
            Text(visibleMonth.formatted(.dateTime.year().month(.wide)))
                .font(.system(size: 13, weight: .semibold))
            Spacer()

            Button { moveMonth(by: 1) } label: {
                Image(systemName: "chevron.right")
            }
            .buttonStyle(.plain)
            .disabled(!canMoveToNextMonth)
        }
        .frame(height: 24)
    }

    private var weekdayHeader: some View {
        LazyVGrid(columns: columns, spacing: 4) {
            ForEach(Array(weekdaySymbols.enumerated()), id: \.offset) { _, symbol in
                Text(symbol)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity)
            }
        }
    }

    private var calendarGrid: some View {
        LazyVGrid(columns: columns, spacing: 4) {
            ForEach(calendarDays, id: \.self) { day in
                dayButton(day)
            }
        }
    }

    private func dayButton(_ day: Date) -> some View {
        let isEndpoint = calendar.isDate(day, inSameDayAs: draftFrom)
            || calendar.isDate(day, inSameDayAs: draftTo)
        let isInRange = day >= draftFrom && day <= draftTo
        let isCurrentMonth = calendar.isDate(day, equalTo: visibleMonth, toGranularity: .month)
        let isFuture = day > calendar.startOfDay(for: Date())

        return Button { select(day) } label: {
            Text("\(calendar.component(.day, from: day))")
                .font(.system(size: 11, weight: isEndpoint ? .semibold : .regular))
                .foregroundStyle(isEndpoint ? Color.white : (isCurrentMonth ? Color.primary : Color.secondary))
                .frame(maxWidth: .infinity, minHeight: 28)
                .background {
                    if isEndpoint {
                        Circle().fill(TVColor.brand)
                    } else if isInRange {
                        RoundedRectangle(cornerRadius: 5).fill(TVColor.brand.opacity(0.13))
                    } else if calendar.isDateInToday(day) {
                        Circle().strokeBorder(TVColor.brand.opacity(0.65), lineWidth: 1)
                    }
                }
                .opacity(isFuture ? 0.3 : 1)
        }
        .buttonStyle(.plain)
        .disabled(isFuture)
    }

    private func shortDate(_ date: Date) -> String {
        date.formatted(.dateTime.year().month(.abbreviated).day())
    }

    private var weekdaySymbols: [String] {
        let symbols = calendar.veryShortStandaloneWeekdaySymbols
        let offset = max(0, calendar.firstWeekday - 1)
        return Array(symbols[offset...] + symbols[..<offset])
    }

    private var calendarDays: [Date] {
        guard let monthStart = calendar.date(
            from: calendar.dateComponents([.year, .month], from: visibleMonth)
        ) else { return [] }
        let weekday = calendar.component(.weekday, from: monthStart)
        let leadingDays = (weekday - calendar.firstWeekday + 7) % 7
        guard let gridStart = calendar.date(byAdding: .day, value: -leadingDays, to: monthStart) else {
            return []
        }
        return (0..<42).compactMap { calendar.date(byAdding: .day, value: $0, to: gridStart) }
    }

    private func prepareSelection() {
        draftFrom = calendar.startOfDay(for: min(from, to))
        draftTo = calendar.startOfDay(for: max(from, to))
        visibleMonth = draftTo
        selectingEnd = false
    }

    private func select(_ date: Date) {
        let day = calendar.startOfDay(for: date)
        if !selectingEnd {
            draftFrom = day
            draftTo = day
            selectingEnd = true
        } else {
            draftFrom = min(draftFrom, day)
            draftTo = max(draftTo, day)
            selectingEnd = false
        }
    }

    private func moveMonth(by value: Int) {
        guard let month = calendar.date(byAdding: .month, value: value, to: visibleMonth) else { return }
        visibleMonth = month
    }

    private var canMoveToNextMonth: Bool {
        guard let next = calendar.date(byAdding: .month, value: 1, to: visibleMonth) else { return false }
        return next <= Date()
    }

    private func applySelection() {
        from = draftFrom
        to = draftTo
        isPresented = false
        onApply()
    }
}


// MARK: - Summary Cards

private struct SummaryCardsView: View {
    let summary: UsageSummary
    let models: [ModelEntry]
    var compact = false
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        LazyVGrid(
            columns: compact
                ? Array(repeating: GridItem(.flexible(), spacing: 14), count: 4)
                : [GridItem(.adaptive(minimum: 145), spacing: 14)],
            spacing: 14
        ) {
            MetricCard(title: l10n.usageTotalTokens, value: tvFormatTokens(summary.total_tokens),
                       icon: "number", tint: TVColor.brand)
            CostMetricCard(totalCost: summary.total_cost_usd, models: models)
            MetricCard(title: l10n.usageConversations, value: "\(summary.conversation_count)",
                       icon: "bubble.left.and.bubble.right.fill", tint: .blue)
            MetricCard(title: l10n.usageActiveDaysTitle, value: "\(summary.active_days)",
                       icon: "calendar", tint: .purple)
        }
    }
}

private struct CostMetricCard: View {
    let totalCost: Double
    let models: [ModelEntry]
    @ObservedObject private var l10n = L10n.shared
    @State private var showsBreakdown = false
    @State private var dismissWorkItem: DispatchWorkItem?

    private var costByModel: [ModelEntry] {
        mergedByModel(models)
            .filter { $0.total_cost_usd > 0 }
            .sorted { lhs, rhs in
                if lhs.total_cost_usd == rhs.total_cost_usd {
                    return lhs.model.localizedCaseInsensitiveCompare(rhs.model) == .orderedAscending
                }
                return lhs.total_cost_usd > rhs.total_cost_usd
            }
    }

    var body: some View {
        MetricCard(title: l10n.cost, value: tvFormatCost(totalCost),
                   icon: "dollarsign.circle.fill", tint: .orange)
            .contentShape(RoundedRectangle(cornerRadius: 12))
            .onHover { hovering in
                hovering ? presentBreakdown() : scheduleDismiss()
            }
            .popover(isPresented: $showsBreakdown, arrowEdge: .bottom) {
                CostBreakdownTip(models: costByModel, totalCost: totalCost)
                    .onHover { hovering in
                        hovering ? cancelDismiss() : scheduleDismiss()
                    }
            }
    }

    private func presentBreakdown() {
        cancelDismiss()
        showsBreakdown = true
    }

    private func scheduleDismiss() {
        cancelDismiss()
        let item = DispatchWorkItem { showsBreakdown = false }
        dismissWorkItem = item
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.18, execute: item)
    }

    private func cancelDismiss() {
        dismissWorkItem?.cancel()
        dismissWorkItem = nil
    }
}

private struct CostBreakdownTip: View {
    let models: [ModelEntry]
    let totalCost: Double
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(l10n.costByModel)
                .font(.system(size: 13, weight: .semibold))

            if models.isEmpty {
                Text(l10n.noUsageData)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            } else {
                ScrollView(showsIndicators: models.count > 8) {
                    VStack(spacing: 8) {
                        ForEach(models) { entry in
                            HStack(spacing: 8) {
                                ModelProviderIcon(model: entry.model,
                                                  fallbackAgentSource: entry.source,
                                                  size: 14)
                                Text(entry.model)
                                    .font(.system(size: 12, weight: .medium))
                                    .lineLimit(1)
                                Spacer(minLength: 16)
                                Text(tvFormatCost(entry.total_cost_usd))
                                    .font(.system(size: 12, weight: .medium, design: .monospaced))
                                    .monospacedDigit()
                            }
                        }
                    }
                }
                .frame(maxHeight: 240)

                Divider()

                HStack {
                    Text(l10n.total)
                        .font(.system(size: 12, weight: .semibold))
                    Spacer()
                    Text(tvFormatCost(totalCost))
                        .font(.system(size: 12, weight: .semibold, design: .monospaced))
                        .monospacedDigit()
                }
            }
        }
        .padding(12)
        .frame(width: 290)
    }
}

private struct MetricCard: View {
    let title: String
    let value: String
    let icon: String
    let tint: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 5) {
                Image(systemName: icon).font(.system(size: 12)).foregroundStyle(tint)
                Text(title).font(.system(size: 12, weight: .medium)).foregroundStyle(.secondary)
            }
            Text(value)
                .font(.system(size: 25, weight: .bold, design: .rounded))
                .monospacedDigit()
                .minimumScaleFactor(0.6)
                .lineLimit(1)
                .contentTransition(.numericText())
                .animation(.spring(duration: 0.4), value: value)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(17)
        .frame(height: 130, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 14)
                .fill(Color(nsColor: .controlBackgroundColor))
                .overlay(RoundedRectangle(cornerRadius: 14).strokeBorder(.quaternary, lineWidth: 0.5))
        )
    }
}

// MARK: - Daily Chart

private struct DailyChartView: View {
    let data: [DailyPoint]
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(l10n.usageDaily).font(.system(size: 16, weight: .semibold))
            let maxTokens = data.map(\.total_tokens).max() ?? 1
            HStack(alignment: .bottom, spacing: 3) {
                ForEach(data) { point in
                    let h = maxTokens > 0 ? CGFloat(point.total_tokens) / CGFloat(maxTokens) : 0
                    RoundedRectangle(cornerRadius: 3)
                        .fill(TVColor.brand.gradient)
                        .frame(maxWidth: .infinity)
                        .frame(height: max(3, h * 130))
                        .help("\(point.date): \(tvFormatTokens(point.total_tokens)) · \(tvFormatCost(point.total_cost_usd))")
                }
            }
            .frame(height: 130)
        }
        .padding(16)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(Color(nsColor: .controlBackgroundColor))
                .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(.quaternary, lineWidth: 0.5))
        )
    }
}

// MARK: - Model Breakdown

private struct ModelBreakdownView: View {
    let models: [ModelEntry]
    var limit = 8
    var compact = false
    var cardHeight: CGFloat? = nil
    @ObservedObject private var l10n = L10n.shared
    @State private var isExpanded = false

    private var merged: [ModelEntry] { mergedByModel(models) }
    private var visibleModels: [ModelEntry] {
        isExpanded ? merged : Array(merged.prefix(limit))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(l10n.usageModels).font(.system(size: 16, weight: .semibold))
            ForEach(visibleModels) { entry in
                VStack(spacing: 5) {
                    HStack(spacing: 8) {
                        ModelProviderIcon(model: entry.model, fallbackAgentSource: entry.source, size: 14)
                        Text(entry.model).font(.system(size: 14, weight: .medium)).lineLimit(1)
                        Spacer()
                        if !compact {
                            Text(tvFormatCost(entry.total_cost_usd))
                                .font(.system(size: 12, design: .monospaced)).foregroundStyle(.secondary)
                        }
                        Text(tvFormatTokens(entry.total_tokens))
                            .font(.system(size: 12, design: .monospaced)).foregroundStyle(.primary)
                            .frame(width: 60, alignment: .trailing)
                    }
                    GeometryReader { geo in
                        ZStack(alignment: .leading) {
                            Capsule().fill(.quaternary).frame(height: 5)
                            Capsule().fill(AgentRegistry.shared.brandColor(for: entry.source))
                                .frame(width: max(2, geo.size.width * entry.percentage / 100.0), height: 5)
                        }
                    }
                    .frame(height: 5)
                }
            }

            if merged.count > limit {
                expandButton
            }
        }
        .tvCard(height: isExpanded ? nil : cardHeight)
    }

    private var expandButton: some View {
        Button {
            withAnimation(.easeInOut(duration: 0.2)) { isExpanded.toggle() }
        } label: {
            HStack(spacing: 5) {
                Text(isExpanded ? l10n.showLess : l10n.showAll)
                Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                    .font(.system(size: 9, weight: .semibold))
            }
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(TVColor.brand)
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.plain)
    }
}


// MARK: - Token type breakdown

private struct TokenTypeBar: View {
    let summary: UsageSummary
    var cardHeight: CGFloat? = nil
    @ObservedObject private var l10n = L10n.shared

    /// (stable key, localized label, tokens, color). The key is used as the
    /// ForEach identity so it doesn't change when the display language does.
    private var segments: [(String, String, UInt64, Color)] {
        [
            ("input", l10n.input, summary.input_tokens, Color.blue),
            ("output", l10n.output, summary.output_tokens, Color.green),
            ("cache_read", l10n.cacheRead, summary.cached_input_tokens, Color.orange),
            ("reasoning", l10n.reasoning, summary.reasoning_output_tokens, Color.purple),
        ].filter { $0.2 > 0 }
    }

    private var hitRate: Double? {
        let denom = summary.input_tokens + summary.cached_input_tokens
        guard denom > 0 else { return nil }
        return Double(summary.cached_input_tokens) / Double(denom) * 100
    }

    var body: some View {
        let total = max(segments.reduce(0) { $0 + $1.2 }, 1)
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(l10n.usageTokenBreakdown).font(.system(size: 16, weight: .semibold))
                Spacer()
                if let hr = hitRate {
                    HStack(spacing: 4) {
                        Text(l10n.cacheHit).font(.system(size: 12)).foregroundStyle(.secondary)
                        Text(String(format: "%.1f%%", hr))
                            .font(.system(size: 14, weight: .bold, design: .monospaced))
                            .foregroundStyle(.orange)
                    }
                }
            }
            GeometryReader { geo in
                HStack(spacing: 2) {
                    ForEach(segments, id: \.0) { seg in
                        Rectangle().fill(seg.3)
                            .frame(width: max(2, geo.size.width * CGFloat(seg.2) / CGFloat(total)))
                    }
                }
                .clipShape(RoundedRectangle(cornerRadius: 4))
            }
            .frame(height: 12)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 110), alignment: .leading)], alignment: .leading, spacing: 7) {
                ForEach(segments, id: \.0) { seg in
                    HStack(spacing: 4) {
                        Circle().fill(seg.3).frame(width: 7, height: 7)
                        Text(seg.1).font(.system(size: 12)).foregroundStyle(.secondary)
                        Text(tvFormatTokens(seg.2)).font(.system(size: 12, weight: .medium, design: .monospaced))
                    }
                }
            }
        }
        .tvCard(height: cardHeight)
    }
}

// MARK: - Agent breakdown

private struct AgentBreakdownView: View {
    let models: [ModelEntry]
    var compact = false
    var cardHeight: CGFloat? = nil
    @ObservedObject private var l10n = L10n.shared
    @State private var isExpanded = false

    private struct Row: Identifiable { let id: String; let tokens: UInt64; let cost: Double }

    private var rows: [Row] {
        var map: [String: (UInt64, Double)] = [:]
        for m in models {
            let e = map[m.source] ?? (0, 0)
            map[m.source] = (e.0 + m.total_tokens, e.1 + m.total_cost_usd)
        }
        return map.map { Row(id: $0.key, tokens: $0.value.0, cost: $0.value.1) }
            .sorted { $0.tokens > $1.tokens }
    }

    private var visibleRows: [Row] {
        isExpanded ? rows : Array(rows.prefix(6))
    }

    var body: some View {
        let total = max(rows.reduce(0) { $0 + $1.tokens }, 1)
        VStack(alignment: .leading, spacing: 12) {
            Text(l10n.usageAgents).font(.system(size: 16, weight: .semibold))
            ForEach(visibleRows) { row in
                VStack(spacing: 5) {
                    HStack(spacing: 8) {
                        AgentIcon(source: row.id, size: 14)
                        Text(AgentRegistry.shared.displayName(for: row.id)).font(.system(size: 14, weight: .medium))
                        Spacer()
                        if !compact {
                            Text(tvFormatCost(row.cost))
                                .font(.system(size: 12, design: .monospaced)).foregroundStyle(.secondary)
                        }
                        Text(tvFormatTokens(row.tokens))
                            .font(.system(size: 12, design: .monospaced))
                            .frame(width: 60, alignment: .trailing)
                    }
                    GeometryReader { geo in
                        ZStack(alignment: .leading) {
                            Capsule().fill(.quaternary).frame(height: 5)
                            Capsule().fill(AgentRegistry.shared.brandColor(for: row.id))
                                .frame(width: max(2, geo.size.width * CGFloat(row.tokens) / CGFloat(total)), height: 5)
                        }
                    }
                    .frame(height: 5)
                }
            }

            if rows.count > 6 {
                expandButton
            }
        }
        .tvCard(height: isExpanded ? nil : cardHeight)
    }

    private var expandButton: some View {
        Button {
            withAnimation(.easeInOut(duration: 0.2)) { isExpanded.toggle() }
        } label: {
            HStack(spacing: 5) {
                Text(isExpanded ? l10n.showLess : l10n.showAll)
                Image(systemName: isExpanded ? "chevron.up" : "chevron.down")
                    .font(.system(size: 9, weight: .semibold))
            }
            .font(.system(size: 11, weight: .medium))
            .foregroundStyle(TVColor.brand)
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Activity heatmap (GitHub-style)

private struct HeatmapView: View {
    let points: [HeatmapPoint]
    /// Real inner width of the card, measured by the parent (see UsageView.body)
    /// and passed down — never self-measured. Self-measuring this view's own
    /// rendered width to size its own cells is a feedback loop (width → cell
    /// size → content size → next measured width) that doesn't reliably
    /// converge, which is why the grid used to either leave a gap on the right
    /// or shrink the visible week range to fit.
    let availableWidth: CGFloat
    var cardHeight: CGFloat? = nil
    @ObservedObject private var l10n = L10n.shared

    private func color(_ level: UInt8) -> Color {
        switch level {
        case 0: return Color.gray.opacity(0.22)
        case 1: return TVColor.brand.opacity(0.35)
        case 2: return TVColor.brand.opacity(0.55)
        case 3: return TVColor.brand.opacity(0.78)
        default: return TVColor.brand
        }
    }

    /// Calendar columns (weeks) spanning `weeks` weeks ending in the current week.
    /// Every day in range gets a Cell (level 0 = no activity, gray) — never nil —
    /// so the grid is always fully populated, with no unfilled cells.
    private struct Cell { let date: Date; let level: UInt8; let count: UInt64 }
    private func buildColumns(weeks: Int) -> [[Cell]] {
        let byDate = Dictionary(uniqueKeysWithValues: points.compactMap { p -> (Date, HeatmapPoint)? in
            AppTime.localDate(fromDayKey: p.date).map { (AppTime.localStartOfDay(for: $0), p) }
        })
        let calendar = AppTime.localCalendar
        let today = calendar.startOfDay(for: Date())
        // Start on the Sunday (weeks-1) weeks before this week's Sunday.
        let weekday = calendar.component(.weekday, from: today) // 1=Sun
        let thisSunday = calendar.date(byAdding: .day, value: -(weekday - 1), to: today)!
        let start = calendar.date(byAdding: .day, value: -(weeks - 1) * 7, to: thisSunday)!

        var columns: [[Cell]] = []
        for w in 0..<weeks {
            var col: [Cell] = []
            for r in 0..<7 {
                let d = calendar.date(byAdding: .day, value: w * 7 + r, to: start)!
                if let p = byDate[d] {
                    col.append(Cell(date: d, level: p.level, count: p.count))
                } else {
                    // No activity (past or future) → lightest gray cell.
                    col.append(Cell(date: d, level: 0, count: 0))
                }
            }
            columns.append(col)
        }
        return columns
    }

    /// Month label per column (shown when month changes).
    private func monthLabel(_ columns: [[Cell]], _ i: Int) -> String? {
        guard let first = columns[i].first?.date else { return nil }
        let m = AppTime.localCalendar.component(.month, from: first)
        let prevM = i > 0 ? columns[i-1].first.map { AppTime.localCalendar.component(.month, from: $0.date) } : nil
        return (i == 0 || m != prevM) ? "\(m)月" : nil
    }

    var body: some View {
        let weekdays = ["日", "一", "二", "三", "四", "五", "六"]
        let labelW: CGFloat = 16
        let sp: CGFloat = 3
        // Always show the full 53-week history; stretch cell size to exactly
        // fill availableWidth (a value the parent measured and passed in, not
        // something this view measures about its own rendered output — see
        // the doc comment on `availableWidth`).
        let weeks = 53
        let n = CGFloat(weeks)
        let cell = max(6, (availableWidth - labelW - (n + 1) * sp) / n)
        let columns = buildColumns(weeks: weeks)
        let activeDays = points.filter { $0.count > 0 }.count

        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(l10n.usageActivity).font(.system(size: 16, weight: .semibold))
                Spacer()
                Text(l10n.usageActiveDays(activeDays)).font(.system(size: 12)).foregroundStyle(.secondary)
            }

            VStack(alignment: .leading, spacing: sp) {
                // Month labels row
                HStack(spacing: sp) {
                    Color.clear.frame(width: labelW)
                    ForEach(Array(columns.enumerated()), id: \.offset) { i, _ in
                        Text(monthLabel(columns, i) ?? "")
                            .font(.system(size: 9)).foregroundStyle(.secondary)
                            .fixedSize()
                            .frame(width: cell, alignment: .leading)
                    }
                }
                // Weekday label column + grid
                HStack(alignment: .top, spacing: sp) {
                    VStack(spacing: sp) {
                        ForEach(0..<7, id: \.self) { r in
                            Text(weekdays[r]).font(.system(size: 8)).foregroundStyle(.tertiary)
                                .frame(width: labelW, height: cell, alignment: .leading)
                        }
                    }
                    ForEach(Array(columns.enumerated()), id: \.offset) { _, week in
                        VStack(spacing: sp) {
                            ForEach(0..<7, id: \.self) { r in
                                let c = week[r]
                                RoundedRectangle(cornerRadius: 2)
                                    .fill(color(c.level))
                                    .frame(width: cell, height: cell)
                                    .help(helpText(c))
                            }
                        }
                    }
                }
            }

            // Legend (centered at bottom)
            HStack(spacing: 4) {
                Spacer()
                Text(l10n.heatmapLess).font(.system(size: 9)).foregroundStyle(.tertiary)
                ForEach(0..<5, id: \.self) { l in
                    RoundedRectangle(cornerRadius: 2).fill(color(UInt8(l))).frame(width: 10, height: 10)
                }
                Text(l10n.heatmapMore).font(.system(size: 9)).foregroundStyle(.tertiary)
                Spacer()
            }
            .frame(maxWidth: .infinity)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .tvCard(height: cardHeight)
    }

    private func helpText(_ cell: Cell) -> String {
        let ds = AppTime.localDayKey(for: cell.date)
        if cell.count > 0 { return "\(ds): \(tvFormatTokens(cell.count))" }
        return "\(ds): 0"
    }
}

// MARK: - All-time daily / project details

private struct UsageDetailsView: View {
    private enum Tab: String, CaseIterable { case daily, projects }

    let daily: [DailyPoint]
    let projects: [ProjectUsageEntry]
    @ObservedObject private var l10n = L10n.shared
    @State private var tab: Tab = .daily
    @State private var projectLimit = 10

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Picker("", selection: $tab) {
                    Text(l10n.usageDailyDetails).tag(Tab.daily)
                    Text(l10n.usageProjectDetails).tag(Tab.projects)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .fixedSize()

                Spacer()

                if tab == .projects {
                    Picker("", selection: $projectLimit) {
                        ForEach([3, 6, 10], id: \.self) { count in
                            Text(l10n.usageProjectTop(count)).tag(count)
                        }
                    }
                    .labelsHidden()
                    .fixedSize()
                }
            }

            if tab == .daily {
                DailyTableView(data: daily)
            } else {
                ProjectUsageList(entries: Array(projects.prefix(projectLimit)))
            }
        }
        .tvCard()
    }
}

private struct ProjectUsageList: View {
    let entries: [ProjectUsageEntry]
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        if entries.isEmpty {
            Text(l10n.usageProjectEmpty)
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, minHeight: 70, alignment: .center)
        } else {
            let maximum = entries.map(\.total_tokens).max() ?? 1
            VStack(spacing: 4) {
                ForEach(entries) { entry in
                    projectRow(entry, maximum: maximum)
                }
            }
        }
    }

    private func projectRow(_ entry: ProjectUsageEntry, maximum: UInt64) -> some View {
        let parts = entry.project_key.split(separator: "/", maxSplits: 1).map(String.init)
        let owner = parts.count > 1 ? parts[0] : ""
        let repo = parts.count > 1 ? parts[1] : (parts.first ?? entry.project_key)
        let fraction = maximum > 0 ? CGFloat(entry.total_tokens) / CGFloat(maximum) : 0

        return HStack(spacing: 12) {
            ZStack {
                RoundedRectangle(cornerRadius: 9).fill(Color(nsColor: .controlColor))
                Text(repo.prefix(1).uppercased())
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(.secondary)
            }
            .frame(width: 38, height: 38)

            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 0) {
                    if !owner.isEmpty {
                        Text("\(owner)/").foregroundStyle(.tertiary)
                    }
                    Text(repo).foregroundStyle(.primary)
                }
                .font(.system(size: 13, weight: .medium))
                .lineLimit(1)

                HStack(spacing: 4) {
                    ForEach(entry.sources.prefix(5), id: \.self) { source in
                        AgentIcon(source: source, size: 13)
                    }
                }
            }

            Spacer(minLength: 16)

            VStack(alignment: .trailing, spacing: 6) {
                Text(tvFormatTokens(entry.total_tokens))
                    .font(.system(size: 13, weight: .medium, design: .monospaced))
                    .monospacedDigit()
                GeometryReader { geometry in
                    ZStack(alignment: .leading) {
                        Capsule().fill(Color.secondary.opacity(0.10))
                        Capsule().fill(TVColor.brand)
                            .frame(width: max(entry.total_tokens > 0 ? 3 : 0, geometry.size.width * fraction))
                    }
                }
                .frame(width: 120, height: 4)
            }
        }
        .padding(.vertical, 6)
    }
}

private struct DailyTableView: View {
    let data: [DailyPoint]
    @ObservedObject private var l10n = L10n.shared

    /// The chart uses hourly points for Today/Yesterday (`YYYY-MM-DDTHH`), while
    /// this table always presents daily totals. Normalize both hourly and daily
    /// query results to day keys before building the rows.
    private var dailyData: [DailyPoint] {
        var totals: [String: DailyPoint] = [:]
        for point in data {
            let dayKey = String(point.date.prefix(10))
            guard AppTime.localDate(fromDayKey: dayKey) != nil else { continue }
            let previous = totals[dayKey]
            totals[dayKey] = DailyPoint(
                date: dayKey,
                total_tokens: (previous?.total_tokens ?? 0) + point.total_tokens,
                total_cost_usd: (previous?.total_cost_usd ?? 0) + point.total_cost_usd,
                input_tokens: (previous?.input_tokens ?? 0) + point.input_tokens,
                output_tokens: (previous?.output_tokens ?? 0) + point.output_tokens,
                cached_input_tokens: (previous?.cached_input_tokens ?? 0) + point.cached_input_tokens,
                cache_creation_input_tokens: (previous?.cache_creation_input_tokens ?? 0) + point.cache_creation_input_tokens,
                reasoning_output_tokens: (previous?.reasoning_output_tokens ?? 0) + point.reasoning_output_tokens,
                conversation_count: (previous?.conversation_count ?? 0) + point.conversation_count
            )
        }
        return totals.values.sorted { $0.date < $1.date }
    }

    /// Every day with recorded usage, newest first. This list intentionally uses
    /// the all-time query and is independent of the dashboard range selector.
    private func rows() -> [(date: String, point: DailyPoint?)] {
        dailyData.sorted { $0.date > $1.date }.map { ($0.date, Optional($0)) }
    }

    private func cacheTotal(_ p: DailyPoint) -> UInt64 { p.cached_input_tokens + p.cache_creation_input_tokens }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            headerRow
            Divider()
            ScrollView(showsIndicators: true) {
                LazyVStack(spacing: 7) {
                    ForEach(rows(), id: \.date) { row in
                        dataRow(row.date, row.point)
                    }
                }
            }
            .frame(maxHeight: 380)
        }
    }

    private var headerRow: some View {
        HStack(spacing: 0) {
            cell(l10n.usageColDate, align: .leading, header: true)
            cell(l10n.usageColTotal, align: .trailing, header: true)
            cell(l10n.usageColInput, align: .trailing, header: true)
            cell(l10n.usageColOutput, align: .trailing, header: true)
            cell(l10n.usageColCache, align: .trailing, header: true)
            cell(l10n.usageColReason, align: .trailing, header: true)
            cell(l10n.usageColConvs, align: .trailing, header: true)
        }
    }

    private func dataRow(_ date: String, _ p: DailyPoint?) -> some View {
        HStack(spacing: 0) {
            cell(date, align: .leading)
            cell(num(p?.total_tokens), align: .trailing)
            cell(num(p?.input_tokens), align: .trailing)
            cell(num(p?.output_tokens), align: .trailing)
            cell(p.map { num(cacheTotal($0)) } ?? "—", align: .trailing)
            cell(num(p?.reasoning_output_tokens), align: .trailing)
            cell(p.map { "\($0.conversation_count)" } ?? "—", align: .trailing)
        }
    }

    private func num(_ v: UInt64?) -> String {
        guard let v else { return "—" }
        return v.formatted(.number.grouping(.automatic))
    }

    private func cell(_ text: String, align: Alignment, header: Bool = false) -> some View {
        Text(text)
            .font(.system(size: header ? 11 : 12, weight: header ? .medium : .regular, design: header ? .default : .monospaced))
            .foregroundStyle(header ? AnyShapeStyle(.secondary) : (text == "—" ? AnyShapeStyle(.tertiary) : AnyShapeStyle(.primary)))
            .lineLimit(1)
            .frame(maxWidth: .infinity, alignment: align)
    }
}
