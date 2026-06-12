import SwiftUI

/// Brand + provider colors mirrored from the original dashboard design system.
enum TVColor {
    static let brand = Color(red: 0.02, green: 0.59, blue: 0.41) // #059669 emerald

    static func sourceDisplayName(_ source: String) -> String {
        switch source.lowercased() {
        case "claude": return "Claude"
        case "codex": return "Codex"
        case "cursor": return "Cursor"
        case "gemini": return "Gemini"
        case "kiro": return "Kiro"
        case "copilot": return "GitHub Copilot"
        case "kimi": return "Kimi"
        case "antigravity": return "Antigravity"
        case "zed": return "Zed"
        case "trae": return "Trae"
        case "windsurf": return "Windsurf"
        case "qoder": return "Qoder"
        case "codebuddy": return "CodeBuddy"
        case "workbuddy": return "WorkBuddy"
        case "mimocode": return "MiMoCode"
        case "goose": return "Goose"
        case "hermes": return "Hermes"
        case "grok": return "Grok"
        case "opencode": return "OpenCode"
        case "openclaw": return "OpenClaw"
        case "roocode": return "RooCode"
        case "kilocode": return "KiloCode"
        case "kilocli": return "KiloCLI"
        case "ohmypi": return "OhMyPi"
        case "pi": return "Pi"
        case "craft": return "Craft"
        case "everycode": return "EveryCode"
        default: return source.capitalized
        }
    }

    static func provider(_ source: String) -> Color {
        switch source.lowercased() {
        case "claude", "codebuddy": return Color(red: 0.85, green: 0.46, blue: 0.34) // #d97757
        case "codex", "everycode": return Color(red: 0.23, green: 0.51, blue: 0.96) // #3b82f6
        case "opencode", "openclaw": return Color(red: 0.96, green: 0.62, blue: 0.04) // #f59e0b
        case "gemini", "antigravity": return Color(red: 0.13, green: 0.59, blue: 0.95) // #2196f3
        case "kimi": return Color(red: 0.65, green: 0.55, blue: 0.98) // violet
        case "kiro": return brand
        case "cursor": return Color(red: 0.55, green: 0.36, blue: 0.96) // purple
        case "grok": return Color(red: 0.45, green: 0.45, blue: 0.5)
        default: return Color(red: 0.3, green: 0.7, blue: 0.55)
        }
    }
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
                VStack(alignment: .leading, spacing: 18) {
                    header

                    Picker("Range", selection: $viewModel.selectedRange) {
                        ForEach(UsageViewModel.TimeRange.allCases, id: \.self) { range in
                            Text(range.localizedTitle).tag(range)
                        }
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .onChange(of: viewModel.selectedRange) { viewModel.refresh() }

                    if viewModel.selectedRange == .custom {
                        HStack(spacing: 12) {
                            DatePicker(l10n.rangeFrom, selection: $viewModel.customFrom,
                                       in: ...viewModel.customTo, displayedComponents: .date)
                            DatePicker(l10n.rangeTo, selection: $viewModel.customTo,
                                       in: viewModel.customFrom...Date(), displayedComponents: .date)
                            Spacer()
                        }
                        .datePickerStyle(.compact)
                        .font(.system(size: 12))
                        .onChange(of: viewModel.customFrom) { viewModel.refresh() }
                        .onChange(of: viewModel.customTo) { viewModel.refresh() }
                    }

                    if let s = viewModel.summary {
                        // Overview
                        SummaryCardsView(summary: s)
                        TokenTypeBar(summary: s)

                        // Trend (hero, full width)
                        if !viewModel.dailyUsage.isEmpty {
                            TrendChartView(data: viewModel.dailyUsage, hourly: viewModel.isHourlyView)
                        }
                        if !viewModel.heatmap.isEmpty {
                            HeatmapView(points: viewModel.heatmap)
                        }

                        // Composition: providers + models side-by-side when wide
                        if !viewModel.modelBreakdown.isEmpty {
                            pair(wide,
                                 ProviderBreakdownView(models: viewModel.modelBreakdown),
                                 ModelBreakdownView(models: viewModel.modelBreakdown))
                        }

                        // Detail
                        if !viewModel.dailyUsage.isEmpty {
                            DailyTableView(data: viewModel.dailyUsage)
                        }
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

    /// Lay two cards side-by-side (top-aligned) when wide, else stacked.
    @ViewBuilder
    private func pair<A: View, B: View>(_ wide: Bool, _ a: A, _ b: B) -> some View {
        if wide {
            HStack(alignment: .top, spacing: 16) {
                a.frame(maxWidth: .infinity, alignment: .topLeading)
                b.frame(maxWidth: .infinity, alignment: .topLeading)
            }
        } else {
            a
            b
        }
    }

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(l10n.usageTitle)
                    .font(.system(size: 24, weight: .bold))
                Text(l10n.usageSubtitle)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button(action: { AppSyncCoordinator.shared.syncAll() }) {
                Image(systemName: "arrow.triangle.2.circlepath")
                    .font(.system(size: 13, weight: .semibold))
                    .rotationEffect(.degrees(viewModel.isLoading ? 360 : 0))
                    .animation(viewModel.isLoading ? .linear(duration: 1).repeatForever(autoreverses: false) : .default, value: viewModel.isLoading)
            }
            .buttonStyle(.borderless)
            .help(l10n.syncNow)
        }
    }
}

// MARK: - Summary Cards

private struct SummaryCardsView: View {
    let summary: UsageSummary

    var body: some View {
        HStack(spacing: 12) {
            MetricCard(title: "Total Tokens", value: tvFormatTokens(summary.total_tokens),
                       icon: "number", tint: TVColor.brand)
            MetricCard(title: "Cost", value: tvFormatCost(summary.total_cost_usd),
                       icon: "dollarsign.circle.fill", tint: .orange)
            MetricCard(title: "Conversations", value: "\(summary.conversation_count)",
                       icon: "bubble.left.and.bubble.right.fill", tint: .blue)
            MetricCard(title: "Active Days", value: "\(summary.active_days)",
                       icon: "calendar", tint: .purple)
        }
    }
}

private struct MetricCard: View {
    let title: String
    let value: String
    let icon: String
    let tint: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 5) {
                Image(systemName: icon).font(.system(size: 11)).foregroundStyle(tint)
                Text(title).font(.system(size: 11, weight: .medium)).foregroundStyle(.secondary)
            }
            Text(value)
                .font(.system(size: 22, weight: .bold, design: .rounded))
                .monospacedDigit()
                .minimumScaleFactor(0.6)
                .lineLimit(1)
                .contentTransition(.numericText())
                .animation(.spring(duration: 0.4), value: value)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(Color(nsColor: .controlBackgroundColor))
                .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(.quaternary, lineWidth: 0.5))
        )
    }
}

// MARK: - Daily Chart

private struct DailyChartView: View {
    let data: [DailyPoint]

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Daily Usage").font(.system(size: 15, weight: .semibold))
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

    private var merged: [ModelEntry] { mergedByModel(models) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Models").font(.system(size: 15, weight: .semibold))
            ForEach(merged.prefix(8)) { entry in
                VStack(spacing: 5) {
                    HStack(spacing: 8) {
                        ProviderIcon(source: entry.source, modelName: entry.model, size: 14)
                        Text(entry.model).font(.system(size: 13, weight: .medium)).lineLimit(1)
                        Spacer()
                        Text(tvFormatCost(entry.total_cost_usd))
                            .font(.system(size: 11, design: .monospaced)).foregroundStyle(.secondary)
                        Text(tvFormatTokens(entry.total_tokens))
                            .font(.system(size: 11, design: .monospaced)).foregroundStyle(.primary)
                            .frame(width: 60, alignment: .trailing)
                    }
                    GeometryReader { geo in
                        ZStack(alignment: .leading) {
                            Capsule().fill(.quaternary).frame(height: 5)
                            Capsule().fill(TVColor.provider(entry.source))
                                .frame(width: max(2, geo.size.width * entry.percentage / 100.0), height: 5)
                        }
                    }
                    .frame(height: 5)
                }
            }
        }
        .padding(16)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(Color(nsColor: .controlBackgroundColor))
                .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(.quaternary, lineWidth: 0.5))
        )
    }
}


// MARK: - Token type breakdown

private struct TokenTypeBar: View {
    let summary: UsageSummary

    private var segments: [(String, UInt64, Color)] {
        [
            ("Input", summary.input_tokens, Color.blue),
            ("Output", summary.output_tokens, Color.green),
            ("Cache Read", summary.cached_input_tokens, Color.orange),
            ("Reasoning", summary.reasoning_output_tokens, Color.purple),
        ].filter { $0.1 > 0 }
    }

    private var hitRate: Double? {
        let denom = summary.input_tokens + summary.cached_input_tokens
        guard denom > 0 else { return nil }
        return Double(summary.cached_input_tokens) / Double(denom) * 100
    }

    var body: some View {
        let total = max(segments.reduce(0) { $0 + $1.1 }, 1)
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Token Breakdown").font(.system(size: 15, weight: .semibold))
                Spacer()
                if let hr = hitRate {
                    HStack(spacing: 4) {
                        Text("Cache hit").font(.system(size: 11)).foregroundStyle(.secondary)
                        Text(String(format: "%.1f%%", hr))
                            .font(.system(size: 13, weight: .bold, design: .monospaced))
                            .foregroundStyle(.orange)
                    }
                }
            }
            GeometryReader { geo in
                HStack(spacing: 2) {
                    ForEach(segments, id: \.0) { seg in
                        Rectangle().fill(seg.2)
                            .frame(width: max(2, geo.size.width * CGFloat(seg.1) / CGFloat(total)))
                    }
                }
                .clipShape(RoundedRectangle(cornerRadius: 4))
            }
            .frame(height: 12)
            HStack(spacing: 14) {
                ForEach(segments, id: \.0) { seg in
                    HStack(spacing: 4) {
                        Circle().fill(seg.2).frame(width: 7, height: 7)
                        Text(seg.0).font(.system(size: 11)).foregroundStyle(.secondary)
                        Text(tvFormatTokens(seg.1)).font(.system(size: 11, weight: .medium, design: .monospaced))
                    }
                }
                Spacer()
            }
        }
        .tvCard()
    }
}

// MARK: - Provider breakdown

private struct ProviderBreakdownView: View {
    let models: [ModelEntry]

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

    var body: some View {
        let total = max(rows.reduce(0) { $0 + $1.tokens }, 1)
        VStack(alignment: .leading, spacing: 12) {
            Text("Providers").font(.system(size: 15, weight: .semibold))
            ForEach(rows) { row in
                VStack(spacing: 5) {
                    HStack(spacing: 8) {
                        ProviderIcon(source: row.id, size: 14)
                        Text(TVColor.sourceDisplayName(row.id)).font(.system(size: 13, weight: .medium))
                        Spacer()
                        Text(tvFormatCost(row.cost))
                            .font(.system(size: 11, design: .monospaced)).foregroundStyle(.secondary)
                        Text(tvFormatTokens(row.tokens))
                            .font(.system(size: 11, design: .monospaced))
                            .frame(width: 60, alignment: .trailing)
                    }
                    GeometryReader { geo in
                        ZStack(alignment: .leading) {
                            Capsule().fill(.quaternary).frame(height: 5)
                            Capsule().fill(TVColor.provider(row.id))
                                .frame(width: max(2, geo.size.width * CGFloat(row.tokens) / CGFloat(total)), height: 5)
                        }
                    }
                    .frame(height: 5)
                }
            }
        }
        .tvCard()
    }
}

// MARK: - Activity heatmap (GitHub-style)

private struct HeatmapView: View {
    let points: [HeatmapPoint]
    @State private var gridWidth: CGFloat = 600
    @ObservedObject private var l10n = L10n.shared

    private static let parser: DateFormatter = {
        let f = DateFormatter(); f.dateFormat = "yyyy-MM-dd"; f.timeZone = TimeZone.current
        f.locale = Locale(identifier: "en_US_POSIX"); return f
    }()
    private static var cal: Calendar {
        var c = Calendar(identifier: .gregorian); c.timeZone = TimeZone.current; return c
    }

    private func color(_ level: UInt8) -> Color {
        switch level {
        case 0: return Color.gray.opacity(0.12)
        case 1: return TVColor.brand.opacity(0.35)
        case 2: return TVColor.brand.opacity(0.55)
        case 3: return TVColor.brand.opacity(0.78)
        default: return TVColor.brand
        }
    }

    /// Calendar columns (weeks) spanning a fixed 6-month window ending today.
    /// nil = future day (blank); present days carry a level (0 = no activity, gray).
    private struct Cell { let date: Date; let level: UInt8; let count: UInt64 }
    private func buildColumns() -> [[Cell?]] {
        let byDate = Dictionary(uniqueKeysWithValues: points.compactMap { p -> (Date, HeatmapPoint)? in
            Self.parser.date(from: p.date).map { (Self.cal.startOfDay(for: $0), p) }
        })
        let weeks = 53
        let today = Self.cal.startOfDay(for: Date())
        // Start on the Sunday (weeks-1) weeks before this week's Sunday.
        let weekday = Self.cal.component(.weekday, from: today) // 1=Sun
        let thisSunday = Self.cal.date(byAdding: .day, value: -(weekday - 1), to: today)!
        let start = Self.cal.date(byAdding: .day, value: -(weeks - 1) * 7, to: thisSunday)!

        var columns: [[Cell?]] = []
        for w in 0..<weeks {
            var col: [Cell?] = []
            for r in 0..<7 {
                let d = Self.cal.date(byAdding: .day, value: w * 7 + r, to: start)!
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
    private func monthLabel(_ columns: [[Cell?]], _ i: Int) -> String? {
        guard let first = columns[i].compactMap({ $0?.date }).first else { return nil }
        let m = Self.cal.component(.month, from: first)
        let prevM = i > 0 ? columns[i-1].compactMap({ $0?.date }).first.map { Self.cal.component(.month, from: $0) } : nil
        return (i == 0 || m != prevM) ? "\(m)月" : nil
    }

    var body: some View {
        let columns = buildColumns()
        let activeDays = points.filter { $0.count > 0 }.count
        let weekdays = ["日", "一", "二", "三", "四", "五", "六"]
        let n = max(columns.count, 1)
        let labelW: CGFloat = 16
        let sp: CGFloat = 3
        // Cell size stretches so n columns + labels fill the measured width.
        let cell = max(6, (gridWidth - labelW - CGFloat(n + 1) * sp) / CGFloat(n))

        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Activity").font(.system(size: 15, weight: .semibold))
                Spacer()
                Text("\(activeDays) active days").font(.system(size: 11)).foregroundStyle(.secondary)
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
                                if let c = week[r] {
                                    RoundedRectangle(cornerRadius: 2)
                                        .fill(color(c.level))
                                        .frame(width: cell, height: cell)
                                        .help(helpText(c))
                                } else {
                                    Color.clear.frame(width: cell, height: cell)
                                }
                            }
                        }
                    }
                }
            }
            .background(GeometryReader { g in
                Color.clear.preference(key: HeatmapWidthKey.self, value: g.size.width)
            })
            .onPreferenceChange(HeatmapWidthKey.self) { gridWidth = $0 }

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
        .tvCard()
    }

    private func helpText(_ cell: Cell) -> String {
        let ds = Self.parser.string(from: cell.date)
        if cell.count > 0 { return "\(ds): \(tvFormatTokens(cell.count))" }
        return "\(ds): 0"
    }
}

// MARK: - Daily details table

private struct DailyTableView: View {
    let data: [DailyPoint]

    private static let parser: DateFormatter = {
        let f = DateFormatter(); f.dateFormat = "yyyy-MM-dd"; f.timeZone = TimeZone(identifier: "UTC")
        f.locale = Locale(identifier: "en_US_POSIX"); return f
    }()

    /// Build a contiguous descending date list; days without a record are nil ("—").
    private func rows() -> [(date: String, point: DailyPoint?)] {
        let byDate = Dictionary(uniqueKeysWithValues: data.map { ($0.date, $0) })
        guard let maxStr = data.map({ $0.date }).max(),
              let maxDate = Self.parser.date(from: maxStr) else { return [] }
        var cal = Calendar(identifier: .gregorian); cal.timeZone = TimeZone(identifier: "UTC")!
        var out: [(String, DailyPoint?)] = []
        var d = maxDate
        for _ in 0..<14 {
            let key = Self.parser.string(from: d)
            out.append((key, byDate[key]))
            d = cal.date(byAdding: .day, value: -1, to: d)!
        }
        return out
    }

    private func cacheTotal(_ p: DailyPoint) -> UInt64 { p.cached_input_tokens + p.cache_creation_input_tokens }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Daily Details").font(.system(size: 15, weight: .semibold))
            headerRow
            Divider()
            ForEach(rows(), id: \.date) { row in
                dataRow(row.date, row.point)
            }
        }
        .tvCard()
    }

    private var headerRow: some View {
        HStack(spacing: 0) {
            cell("Date", width: nil, align: .leading, header: true)
            cell("Total", width: col, align: .trailing, header: true)
            cell("Input", width: col, align: .trailing, header: true)
            cell("Output", width: col, align: .trailing, header: true)
            cell("Cache", width: col, align: .trailing, header: true)
            cell("Reason", width: col, align: .trailing, header: true)
            cell("Convs", width: convCol, align: .trailing, header: true)
        }
    }

    private func dataRow(_ date: String, _ p: DailyPoint?) -> some View {
        HStack(spacing: 0) {
            cell(date, width: nil, align: .leading)
            cell(num(p?.total_tokens), width: col, align: .trailing)
            cell(num(p?.input_tokens), width: col, align: .trailing)
            cell(num(p?.output_tokens), width: col, align: .trailing)
            cell(p.map { num(cacheTotal($0)) } ?? "—", width: col, align: .trailing)
            cell(num(p?.reasoning_output_tokens), width: col, align: .trailing)
            cell(p.map { "\($0.conversation_count)" } ?? "—", width: convCol, align: .trailing)
        }
    }

    private let col: CGFloat = 78
    private let convCol: CGFloat = 52

    private func num(_ v: UInt64?) -> String {
        guard let v else { return "—" }
        return v.formatted(.number.grouping(.automatic))
    }

    private func cell(_ text: String, width: CGFloat?, align: Alignment, header: Bool = false) -> some View {
        Text(text)
            .font(.system(size: header ? 11 : 12, weight: header ? .medium : .regular, design: header ? .default : .monospaced))
            .foregroundStyle(header ? AnyShapeStyle(.secondary) : (text == "—" ? AnyShapeStyle(.tertiary) : AnyShapeStyle(.primary)))
            .lineLimit(1)
            .frame(width: width, alignment: align)
            .frame(maxWidth: width == nil ? .infinity : nil, alignment: align)
    }
}


private struct HeatmapWidthKey: PreferenceKey {
    static var defaultValue: CGFloat = 600
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = nextValue() }
}
