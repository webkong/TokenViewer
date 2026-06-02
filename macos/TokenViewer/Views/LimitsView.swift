import SwiftUI

struct LimitsView: View {
    @ObservedObject var viewModel: LimitsViewModel
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                header
                ForEach(viewModel.providers) { provider in
                    ProviderLimitCard(provider: provider)
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
                Text("Per-agent quota windows with reset countdowns")
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
            .help(viewModel.isLoading ? "Refreshing…" : "Refresh limits")
        }
    }
}

private struct ProviderLimitCard: View {
    let provider: ProviderLimit

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
                if !provider.configured {
                    Text("Not configured").font(.system(size: 11)).foregroundStyle(.tertiary)
                } else if let err = provider.error {
                    Text(err).font(.system(size: 11)).foregroundStyle(.orange)
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
        .opacity(provider.configured ? 1 : 0.55)
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
                    Text("resets \(reset, format: .relative(presentation: .named))")
                        .font(.system(size: 10)).foregroundStyle(.secondary)
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
