import SwiftUI

struct LimitsView: View {
    @ObservedObject var viewModel: LimitsViewModel
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        GeometryReader { geo in
            let cardW = (geo.size.width - 40 - 12) / 2
            ScrollView(showsIndicators: false) {
                VStack(alignment: .leading, spacing: 16) {
                    header
                    // The Limits page always shows every agent that supports quota
                    // tracking, regardless of the menu-bar popover visibility toggle
                    // (Settings > Menu Bar), which only affects the popover cards.
                    let agentBySource = Dictionary(uniqueKeysWithValues: viewModel.agents.map { ($0.name, $0) })
                    let allAgents = LimitsVisibilityStore.allSources
                        .map { source in
                            agentBySource[source] ?? AgentLimit(name: source, planLabel: nil, configured: false, error: nil, windows: [])
                        }
                    let activeAgents = allAgents.filter { $0.configured && $0.hasLimitDisplay }
                    let inactiveAgents = allAgents.filter { !$0.configured || !$0.hasLimitDisplay }
                    if allAgents.isEmpty {
                        emptyState
                    } else {
                        twoColumnSection(agents: activeAgents, cardWidth: cardW)
                        if !inactiveAgents.isEmpty {
                            Divider().padding(.vertical, 2)
                            twoColumnSection(agents: inactiveAgents, cardWidth: cardW)
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

    private func twoColumnSection(agents: [AgentLimit], cardWidth: CGFloat) -> some View {
        ForEach(Array(stride(from: 0, to: agents.count, by: 2)), id: \.self) { i in
            HStack(alignment: .top, spacing: 12) {
                AgentLimitCard(agent: agents[i])
                    .frame(width: cardWidth)
                if i + 1 < agents.count {
                    AgentLimitCard(agent: agents[i + 1])
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
                viewModel.refresh(force: true, showToast: true)
            }) {
                Image(systemName: "arrow.triangle.2.circlepath")
                    .font(.system(size: 13, weight: .semibold))
                    .rotationEffect(.degrees(viewModel.isLoading ? 360 : 0))
                    .animation(viewModel.isLoading ? .linear(duration: 1).repeatForever(autoreverses: false) : .default, value: viewModel.isLoading)
            }
            .buttonStyle(.borderless)
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

private struct AgentLimitCard: View {
    let agent: AgentLimit
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                AgentIcon(source: agent.name, size: 16)
                Text(AgentRegistry.shared.displayName(for: agent.name)).font(.system(size: 15, weight: .semibold))
                if let plan = agent.planLabel {
                    Text(plan)
                        .font(.system(size: 10, weight: .medium))
                        .padding(.horizontal, 6).padding(.vertical, 2)
                        .background(Capsule().fill(AgentRegistry.shared.brandColor(for: agent.name).opacity(0.15)))
                        .foregroundStyle(AgentRegistry.shared.brandColor(for: agent.name))
                }
                Spacer()
                if let expiry = agent.subscriptionExpiresAt {
                    AgentDateBadge(kind: .expires, date: expiry, tint: AgentRegistry.shared.brandColor(for: agent.name))
                } else if let reset = agent.subscriptionResetAt {
                    AgentDateBadge(kind: .subscriptionReset, date: reset, tint: AgentRegistry.shared.brandColor(for: agent.name))
                } else if let reset = agent.quotaResetAt {
                    AgentDateBadge(kind: .quotaReset, date: reset, tint: AgentRegistry.shared.brandColor(for: agent.name))
                }
                if !agent.configured {
                    Text(l10n.notConfigured).font(.system(size: 11)).foregroundStyle(.tertiary)
                } else if let err = agent.error {
                    Text(err).font(.system(size: 11)).foregroundStyle(.orange)
                } else if agent.windows.isEmpty {
                    Text(l10n.noUsageData).font(.system(size: 11)).foregroundStyle(.tertiary)
                }
            }

            if agent.configured && !agent.windows.isEmpty {
                ForEach(agent.windows) { window in
                    LimitWindowRow(window: window, tint: AgentRegistry.shared.brandColor(for: agent.name))
                }
            }
        }
        .padding(16)
        .frame(minHeight: agent.configured && agent.hasLimitDisplay ? kLimitCardMinHeight : 0, alignment: .top)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(Color(nsColor: .controlBackgroundColor))
                .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(.quaternary, lineWidth: 0.5))
        )
        .opacity(agent.configured && agent.hasLimitDisplay ? 1 : 0.55)
    }
}

/// Fixed height for one LimitWindowRow so every bar row is exactly the same.
/// text(~16pt) + spacing(5pt) + bar(6pt) ≈ 27pt.
private let kLimitRowHeight: CGFloat = 27
/// Card minHeight that reserves space for two rows:
/// padding(32) + header(~18) + gaps(2×10) + 2 rows(2×27) ≈ 124pt.
private let kLimitCardMinHeight: CGFloat = 124

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
        .frame(height: kLimitRowHeight)
    }

    private var barColor: Color {
        if window.usedPercent >= 90 { return .red }
        if window.usedPercent >= 70 { return .orange }
        return tint
    }
}

private struct AgentDateBadge: View {
    let kind: AgentCountdownKind
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

enum AgentCountdownKind {
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
