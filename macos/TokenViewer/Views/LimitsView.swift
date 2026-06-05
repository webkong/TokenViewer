import SwiftUI

struct LimitsView: View {
    @ObservedObject var viewModel: LimitsViewModel
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 16) {
                header
                let activeProviders = viewModel.providers.filter { $0.configured && $0.hasLimitDisplay }
                let inactiveProviders = viewModel.providers.filter { !$0.configured || !$0.hasLimitDisplay }
                if viewModel.providers.isEmpty {
                    emptyState
                } else {
                    ForEach(activeProviders) { provider in
                        ProviderLimitCard(provider: provider)
                    }
                    if !inactiveProviders.isEmpty {
                        Divider().padding(.vertical, 2)
                        ForEach(inactiveProviders) { provider in
                            ProviderLimitCard(provider: provider)
                        }
                    }
                }
            }
            .padding(20)
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .onAppear { viewModel.startAutoRefresh() }
        .onDisappear { viewModel.stopAutoRefresh() }
    }

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(l10n.limitsTitle).font(.system(size: 24, weight: .bold))
                Text(l10n.limitsSubtitle)
                    .font(.system(size: 12)).foregroundStyle(.secondary)
            }
            Spacer()
            Button(action: { viewModel.refresh() }) {
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
                Text(provider.name.capitalized).font(.system(size: 15, weight: .semibold))
                if let plan = provider.planLabel {
                    Text(plan)
                        .font(.system(size: 10, weight: .medium))
                        .padding(.horizontal, 6).padding(.vertical, 2)
                        .background(Capsule().fill(TVColor.provider(provider.name).opacity(0.15)))
                        .foregroundStyle(TVColor.provider(provider.name))
                }
                Spacer()
                if let expiry = provider.subscriptionExpiresAt {
                    ProviderDateBadge(label: l10n.expires, date: expiry, tint: TVColor.provider(provider.name))
                } else if let reset = provider.subscriptionResetAt {
                    ProviderDateBadge(label: l10n.subscriptionReset, date: reset, tint: TVColor.provider(provider.name))
                } else if let reset = provider.quotaResetAt {
                    ProviderDateBadge(label: l10n.quotaReset, date: reset, tint: TVColor.provider(provider.name))
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
                    LimitWindowRow(window: window, tint: TVColor.provider(provider.name))
                }
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
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
    let label: String
    let date: Date
    let tint: Color

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: "clock.arrow.circlepath")
                .font(.system(size: 9, weight: .semibold))
            Text(label)
            Text(date, format: .relative(presentation: .named))
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
        HStack(spacing: 3) {
            Text(l10n.resets)
            Text(date, format: .relative(presentation: .named))
        }
        .font(.system(size: 10))
        .foregroundStyle(.secondary)
    }
}
