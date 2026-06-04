import SwiftUI

struct PopoverView: View {
    @ObservedObject private var viewModel = UsageViewModel.shared
    @ObservedObject private var limitsVM = LimitsViewModel.shared
    @ObservedObject private var currency = CurrencyStore.shared
    @ObservedObject private var l10n = L10n.shared
    var onOpenMainWindow: (() -> Void)?
    var onClose: (() -> Void)?

    // Section visibility (user-configurable in Settings → Menu Bar Panel)
    @AppStorage("panelShowSummary") private var showSummary = true
    @AppStorage("panelShowLimits") private var showLimits = true
    @AppStorage("panelShowHeatmap") private var showHeatmap = true
    @AppStorage("panelShowTrend") private var showTrend = true
    @AppStorage("panelShowModels") private var showModels = true

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            ScrollView(showsIndicators: false) {
                VStack(alignment: .leading, spacing: 14) {
                    if viewModel.summary == nil {
                        loading
                    } else {
                        if showSummary { summarySection }
                        if showLimits { limitsSection }
                        if showTrend && !viewModel.dailyUsage.isEmpty { trendSection }
                        if showHeatmap && !viewModel.heatmap.isEmpty { heatmapSection }
                        if showModels && !viewModel.modelBreakdown.isEmpty { modelsSection }
                    }
                }
                .frame(width: 392)
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
            }
            .frame(width: 420, alignment: .center)
            .clipped()
            Divider()
            footer
        }
        .frame(width: 420, height: 620)
        .onKeyPress(.escape) { onClose?(); return .handled }
        .onAppear {
            viewModel.sync()
            limitsVM.refreshIfStale()
        }
    }

    // MARK: Header / Footer

    private var header: some View {
        HStack {
            ProviderIconLogo()
            Text("Token Viewer").font(.system(size: 13, weight: .bold))
            Spacer()
            Button(action: { viewModel.sync() }) {
                Image(systemName: "arrow.triangle.2.circlepath")
                    .font(.system(size: 11))
                    .rotationEffect(.degrees(viewModel.isLoading ? 360 : 0))
                    .animation(viewModel.isLoading ? .linear(duration: 1).repeatForever(autoreverses: false) : .default, value: viewModel.isLoading)
                    .foregroundColor(.secondary)
            }
            .buttonStyle(.plain).help(l10n.syncNow)
        }
        .padding(.horizontal, 14).padding(.vertical, 12)
    }

    private var footer: some View {
        HStack(spacing: 12) {
            Button(action: { onOpenMainWindow?() }) {
                Label(l10n.dashboard, systemImage: "macwindow").font(.system(size: 11, weight: .medium))
            }.buttonStyle(.plain).foregroundColor(TVColor.brand)
            Spacer()
            Button(action: { NSApplication.shared.terminate(nil) }) {
                Label(l10n.quit, systemImage: "power").font(.system(size: 11)).foregroundColor(.secondary)
            }.buttonStyle(.plain)
        }
        .padding(.horizontal, 14).padding(.vertical, 8)
    }

    private var loading: some View {
        VStack(spacing: 8) {
            ProgressView().scaleEffect(0.8)
            Text("Loading…").font(.system(size: 11)).foregroundColor(.secondary)
        }.frame(maxWidth: .infinity).padding(.vertical, 40)
    }

    // MARK: Summary (4 period cards with tinted backgrounds)

    private let cardColors: [Color] = [TVColor.brand, .orange, .blue, .purple]

    private var summarySection: some View {
        let cards = viewModel.panelCards
        return LazyVGrid(columns: [GridItem(.flexible(), spacing: 6), GridItem(.flexible(), spacing: 6)], spacing: 6) {
            ForEach(Array(cards.enumerated()), id: \.offset) { i, c in
                let tint = cardColors[i % cardColors.count]
                VStack(alignment: .leading, spacing: 3) {
                    Text(c.title).font(.system(size: 10, weight: .medium)).foregroundStyle(tint.opacity(0.9))
                    Text(c.value)
                        .font(.system(size: 17, weight: .bold, design: .monospaced))
                        .lineLimit(1).minimumScaleFactor(0.6)
                        .contentTransition(.numericText())
                        .animation(.spring(duration: 0.4), value: c.value)
                    Text(c.subtitle)
                        .font(.system(size: 9)).foregroundStyle(.secondary).lineLimit(1)
                        .contentTransition(.numericText())
                        .animation(.spring(duration: 0.4), value: c.subtitle)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(10)
                .background(
                    RoundedRectangle(cornerRadius: 10)
                        .fill(tint.opacity(0.1))
                        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(tint.opacity(0.2), lineWidth: 0.5))
                )
            }
        }
    }

    // MARK: Limits (compact)

    private var limitsSection: some View {
        let active = limitsVM.providers.filter { $0.configured && !$0.windows.isEmpty }
        return Group {
            if !active.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    sectionHeader(l10n.limits)
                    ForEach(active) { p in
                        if let w = p.windows.first {
                            HStack(spacing: 6) {
                                ProviderIcon(source: p.name, size: 13)
                                Text(p.name.capitalized).font(.system(size: 11)).lineLimit(1)
                                Spacer(minLength: 4)
                                GeometryReader { geo in
                                    ZStack(alignment: .leading) {
                                        Capsule().fill(.quaternary).frame(height: 4)
                                        Capsule().fill(barColor(w.usedPercent))
                                            .frame(width: max(2, geo.size.width * min(w.usedPercent, 100) / 100), height: 4)
                                    }
                                }.frame(width: 90, height: 4)
                                Text(String(format: "%.0f%%", w.usedPercent))
                                    .font(.system(size: 10, design: .monospaced)).foregroundStyle(.secondary)
                                    .frame(width: 34, alignment: .trailing)
                            }
                        }
                    }
                }
            }
        }
    }

    // MARK: Trend (period picker + chart)

    private var trendSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                sectionHeader(l10n.trend)
                Spacer()
                Picker("", selection: $viewModel.selectedRange) {
                    ForEach(UsageViewModel.TimeRange.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented).labelsHidden().frame(width: 180).controlSize(.mini)
                .onChange(of: viewModel.selectedRange) { viewModel.refresh() }
            }
            TrendChartView(data: viewModel.dailyUsage, hourly: viewModel.selectedRange == .today)
        }
    }

    // MARK: Heatmap (compact, last ~16 weeks)

    private var heatmapSection: some View {
        let cellSize: CGFloat = 8
        let gap: CGFloat = 2
        let labelW: CGFloat = 10
        let cal = { var c = Calendar(identifier: .gregorian); c.timeZone = TimeZone.current; return c }()
        let today = cal.startOfDay(for: Date())
        let weekday = cal.component(.weekday, from: today)
        let thisSunday = cal.date(byAdding: .day, value: -(weekday - 1), to: today)!
        let pf = DateFormatter(); pf.dateFormat = "yyyy-MM-dd"; pf.timeZone = TimeZone.current; pf.locale = Locale(identifier: "en_US_POSIX")
        let mf = DateFormatter(); mf.dateFormat = "M月"; mf.timeZone = TimeZone.current
        let byDate = Dictionary(uniqueKeysWithValues: viewModel.heatmap.compactMap { p -> (Date, HeatmapPoint)? in
            pf.date(from: p.date).map { (cal.startOfDay(for: $0), p) }
        })
        let wdLabels = ["日","一","二","三","四","五","六"]
        let totalH: CGFloat = 10 + gap + 7 * cellSize + 6 * gap   // 82pt

        // Popover is 420pt wide, padding 14*2 = 28, effective grid width ≈ 392.
        let gridWidth: CGFloat = 392
        return VStack(alignment: .leading, spacing: 4) {
            sectionHeader(l10n.activity)
            let numWeeks = max(4, Int((gridWidth - labelW - gap) / (cellSize + gap)))
            let start = cal.date(byAdding: .day, value: -(numWeeks - 1) * 7, to: thisSunday)!
            HStack(alignment: .top, spacing: 0) {
                    // Y-axis
                    VStack(spacing: gap) {
                        Color.clear.frame(width: labelW, height: 10)
                        ForEach(0..<7, id: \.self) { r in
                            Text(r % 2 == 1 ? wdLabels[r] : "")
                                .font(.system(size: 6)).foregroundStyle(.tertiary)
                                .frame(width: labelW, height: cellSize, alignment: .leading)
                        }
                    }
                    .frame(width: labelW)
                    ForEach(0..<numWeeks, id: \.self) { w in
                        VStack(spacing: gap) {
                            let d0 = cal.date(byAdding: .day, value: w * 7, to: start)!
                            let m = cal.component(.month, from: d0)
                            let prevM = w > 0 ? cal.component(.month, from: cal.date(byAdding: .day, value: (w-1)*7, to: start)!) : -1
                            Text(m != prevM ? mf.string(from: d0) : "")
                                .font(.system(size: 7)).foregroundStyle(.secondary)
                                .frame(width: cellSize + gap, height: 10, alignment: .leading)
                                .clipped()
                            ForEach(0..<7, id: \.self) { r in
                                let d = cal.date(byAdding: .day, value: w * 7 + r, to: start)!
                                RoundedRectangle(cornerRadius: 2).fill(heatColor(byDate[d]?.level ?? 0))
                                    .frame(width: cellSize, height: cellSize)
                            }
                        }
                        .frame(width: cellSize + gap)
                    }
                }
            .frame(height: totalH)
        }
    }

    // MARK: Top models

    private var modelsSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            sectionHeader(l10n.topModels)
            ForEach(Array(viewModel.modelBreakdown.prefix(4).enumerated()), id: \.offset) { _, m in
                HStack(spacing: 6) {
                    ProviderIcon(source: m.source, modelName: m.model, size: 13)
                    Text(m.model).font(.system(size: 11)).lineLimit(1).truncationMode(.middle)
                    Spacer(minLength: 4)
                    Text(tvFormatTokens(m.total_tokens)).font(.system(size: 10, design: .monospaced)).foregroundStyle(.secondary)
                    Text(String(format: "%.0f%%", m.percentage)).font(.system(size: 10, design: .monospaced)).foregroundStyle(.tertiary)
                        .frame(width: 34, alignment: .trailing)
                }
            }
        }
    }

    // MARK: Helpers

    private func sectionHeader(_ t: String) -> some View {
        Text(t).font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary)
    }
    private func barColor(_ p: Double) -> Color { p >= 90 ? .red : (p >= 70 ? .orange : TVColor.brand) }
    private func heatColor(_ l: UInt8) -> Color {
        switch l { case 0: return .gray.opacity(0.12); case 1: return TVColor.brand.opacity(0.35)
        case 2: return TVColor.brand.opacity(0.55); case 3: return TVColor.brand.opacity(0.78); default: return TVColor.brand }
    }
}

/// Small app logo for the panel header.
private struct ProviderIconLogo: View {
    var body: some View {
        if let img = logo() {
            Image(nsImage: img).resizable().interpolation(.high).scaledToFit().frame(width: 16, height: 16)
        } else {
            Image(systemName: "chart.bar.fill").foregroundColor(TVColor.brand).font(.system(size: 14, weight: .semibold))
        }
    }

    private func logo() -> NSImage? {
        guard let url = Bundle.main.url(forResource: "AppLogo", withExtension: "svg"),
              let img = NSImage(contentsOf: url) else { return nil }
        img.size = NSSize(width: 32, height: 32)
        return img
    }
}
