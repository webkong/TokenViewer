import SwiftUI

struct LimitsView: View {
    @ObservedObject var viewModel: LimitsViewModel
    @ObservedObject private var l10n = L10n.shared
    @AppStorage("limitsVisibleSources") private var limitsVisibleSources = LimitsVisibilityStore.defaultsValue

    var body: some View {
        GeometryReader { geo in
            let cardW = (geo.size.width - 40 - 12) / 2
            ScrollView(showsIndicators: false) {
                VStack(alignment: .leading, spacing: 16) {
                    header
                    let visibleSet = LimitsVisibilityStore.visibleSet(from: limitsVisibleSources)
                    let providerBySource = Dictionary(uniqueKeysWithValues: viewModel.providers.map { ($0.name, $0) })
                    let visibleProviders = LimitsVisibilityStore.allSources
                        .filter { visibleSet.contains($0) }
                        .map { source in
                            providerBySource[source] ?? ProviderLimit(name: source, planLabel: nil, configured: false, error: nil, windows: [])
                        }
                    let activeProviders = visibleProviders.filter { $0.configured && $0.hasLimitDisplay }
                    let inactiveProviders = visibleProviders.filter { !$0.configured || !$0.hasLimitDisplay }
                    if visibleProviders.isEmpty {
                        emptyState
                    } else {
                        twoColumnSection(providers: activeProviders, cardWidth: cardW)
                        if !inactiveProviders.isEmpty {
                            Divider().padding(.vertical, 2)
                            twoColumnSection(providers: inactiveProviders, cardWidth: cardW)
                        }
                    }
                }
                .padding(20)
            }
            .background(Color(nsColor: .windowBackgroundColor))
            .onAppear { viewModel.startAutoRefresh() }
            .onDisappear { viewModel.stopAutoRefresh() }
        }
    }

    private func twoColumnSection(providers: [ProviderLimit], cardWidth: CGFloat) -> some View {
        ForEach(Array(stride(from: 0, to: providers.count, by: 2)), id: \.self) { i in
            HStack(alignment: .top, spacing: 12) {
                ProviderLimitCard(provider: providers[i])
                    .frame(width: cardWidth)
                if i + 1 < providers.count {
                    ProviderLimitCard(provider: providers[i + 1])
                        .frame(width: cardWidth)
                }
            }
        }
    }

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(l10n.limitsTitle).font(.system(size: 24, weight: .bold))
                Text(l10n.limitsSubtitle)
                    .font(.system(size: 12)).foregroundStyle(.secondary)
            }
            Spacer()
            Button(action: {
                viewModel.refresh()
                ToastCenter.shared.success(l10n.toastRefreshed)
            }) {
                Image(systemName: "arrow.triangle.2.circlepath")
                    .font(.system(size: 13, weight: .semibold))
                    .rotationEffect(.degrees(viewModel.isLoading ? 360 : 0))
                    .animation(viewModel.isLoading ? .linear(duration: 1).repeatForever(autoreverses: false) : .default, value: viewModel.isLoading)
            }
            .buttonStyle(.borderless)
            .disabled(viewModel.isLoading)
            .help(viewModel.isLoading ? l10n.refreshingLimits : l10n.refreshLimits)
        }
    }

    private var emptyState: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(l10n.noLimitsData)
                .font(.system(size: 13, weight: .medium))
            Text(l10n.limitsNoDataDesc)
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(Color(nsColor: .controlBackgroundColor))
                .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(.quaternary, lineWidth: 0.5))
        )
    }
}

private struct ProviderLimitCard: View {
    let provider: ProviderLimit
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                ProviderIcon(source: provider.name, size: 16)
                Text(ProviderRegistry.shared.displayName(for: provider.name)).font(.system(size: 15, weight: .semibold))
                if let plan = provider.planLabel {
                    Text(plan)
                        .font(.system(size: 10, weight: .medium))
                        .padding(.horizontal, 6).padding(.vertical, 2)
                        .background(Capsule().fill(ProviderRegistry.shared.brandColor(for: provider.name).opacity(0.15)))
                        .foregroundStyle(ProviderRegistry.shared.brandColor(for: provider.name))
                }
                Spacer()
                if let expiry = provider.subscriptionExpiresAt {
                    ProviderDateBadge(kind: .expires, date: expiry, tint: ProviderRegistry.shared.brandColor(for: provider.name))
                } else if let reset = provider.subscriptionResetAt {
                    ProviderDateBadge(kind: .subscriptionReset, date: reset, tint: ProviderRegistry.shared.brandColor(for: provider.name))
                } else if let reset = provider.quotaResetAt {
                    ProviderDateBadge(kind: .quotaReset, date: reset, tint: ProviderRegistry.shared.brandColor(for: provider.name))
                }
                if !provider.configured {
                    Text(l10n.notConfigured).font(.system(size: 11)).foregroundStyle(.tertiary)
                } else if let err = provider.error {
                    Text(err).font(.system(size: 11)).foregroundStyle(.orange)
                } else if provider.windows.isEmpty {
                    Text(l10n.noUsageData).font(.system(size: 11)).foregroundStyle(.tertiary)
                }
            }

            if provider.configured && !provider.windows.isEmpty {
                ForEach(provider.windows) { window in
                    LimitWindowRow(window: window, tint: ProviderRegistry.shared.brandColor(for: provider.name))
                }
            }
        }
        .padding(16)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(Color(nsColor: .controlBackgroundColor))
                .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(.quaternary, lineWidth: 0.5))
        )
        .opacity(provider.configured && provider.hasLimitDisplay ? 1 : 0.55)
    }
}

private struct LimitWindowRow: View {
    let window: LimitWindow
    let tint: Color

    var body: some View {
        VStack(spacing: 5) {
            HStack {
                Text(window.label).font(.system(size: 12, weight: .medium))
                Spacer()
                if let reset = window.resetAt {
                    ResetInlineText(date: reset)
                }
                Text(String(format: "%.0f%%", window.usedPercent))
                    .font(.system(size: 12, weight: .semibold, design: .monospaced))
                    .foregroundStyle(barColor)
            }
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(.quaternary).frame(height: 6)
                    Capsule().fill(barColor)
                        .frame(width: max(2, geo.size.width * min(window.usedPercent, 100) / 100.0), height: 6)
                }
            }
            .frame(height: 6)
        }
    }

    private var barColor: Color {
        if window.usedPercent >= 90 { return .red }
        if window.usedPercent >= 70 { return .orange }
        return tint
    }
}

private struct ProviderDateBadge: View {
    let kind: ProviderCountdownKind
    let date: Date
    let tint: Color
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: "clock.arrow.circlepath")
                .font(.system(size: 9, weight: .semibold))
            Text(kind.text(date: date, l10n: l10n))
        }
        .font(.system(size: 10, weight: .medium))
        .foregroundStyle(tint)
        .lineLimit(1)
        .padding(.horizontal, 7)
        .padding(.vertical, 3)
        .background(Capsule().fill(tint.opacity(0.12)))
    }
}

private struct ResetInlineText: View {
    let date: Date
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        Text(l10n.resetsIn(l10n.countdownText(until: date)))
        .font(.system(size: 10))
        .foregroundStyle(.secondary)
    }
}

enum ProviderCountdownKind {
    case expires
    case subscriptionReset
    case quotaReset

    func text(date: Date, l10n: L10n) -> String {
        let days = date.tvCountdownDaysFromNow
        switch self {
        case .expires:
            return l10n.expiresInDays(days)
        case .subscriptionReset:
            return l10n.subscriptionResetsInDays(days)
        case .quotaReset:
            return l10n.quotaResetsInDays(days)
        }
    }
}

extension Date {
    var tvCountdownDaysFromNow: Int {
        let seconds = timeIntervalSince(Date())
        if seconds <= 0 { return 0 }
        return max(1, Int(ceil(seconds / 86_400)))
    }
}
