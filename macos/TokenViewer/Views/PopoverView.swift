import SwiftUI

struct PopoverView: View {
    @ObservedObject private var viewModel = UsageViewModel.shared
    @ObservedObject private var currency = CurrencyStore.shared
    var onOpenMainWindow: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack {
                Image(systemName: "chart.bar.fill")
                    .foregroundColor(.green)
                    .font(.system(size: 14, weight: .semibold))
                Text("Token Viewer")
                    .font(.system(size: 13, weight: .bold))
                Spacer()
                Button(action: { viewModel.sync() }) {
                    Image(systemName: "arrow.triangle.2.circlepath")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)
                }
                .buttonStyle(.plain)
                .help("Sync")
            }
            .padding(.horizontal, 16)
            .padding(.top, 14)
            .padding(.bottom, 10)

            Divider().padding(.horizontal, 12)

            // Stats cards
            if let summary = viewModel.summary {
                VStack(spacing: 8) {
                    HStack(spacing: 8) {
                        StatCard(title: "Tokens", value: formatCompact(summary.total_tokens), icon: "number", color: .green)
                        StatCard(title: "Cost", value: formatCost(summary.total_cost_usd), icon: "dollarsign.circle", color: .orange)
                    }
                    HStack(spacing: 8) {
                        StatCard(title: "Conversations", value: "\(summary.conversation_count)", icon: "bubble.left.and.bubble.right", color: .blue)
                        StatCard(title: "Active Days", value: "\(summary.active_days)", icon: "calendar", color: .purple)
                    }
                }
                .padding(.horizontal, 12)
                .padding(.top, 12)
            } else {
                VStack(spacing: 8) {
                    ProgressView()
                        .scaleEffect(0.8)
                    Text("Loading...")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 24)
            }

            // Mini trend sparkline
            if !viewModel.dailyUsage.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Last 7 Days")
                        .font(.system(size: 10, weight: .medium))
                        .foregroundColor(.secondary)
                    SparklineView(data: viewModel.dailyUsage.suffix(7).map { Double($0.total_tokens) })
                        .frame(height: 32)
                }
                .padding(.horizontal, 12)
                .padding(.top, 12)
            }

            // Top models
            if !viewModel.modelBreakdown.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Top Models")
                        .font(.system(size: 10, weight: .medium))
                        .foregroundColor(.secondary)
                    ForEach(viewModel.modelBreakdown.prefix(3)) { entry in
                        HStack(spacing: 6) {
                            Circle()
                                .fill(modelColor(entry.source))
                                .frame(width: 6, height: 6)
                            Text(entry.model)
                                .font(.system(size: 11))
                                .lineLimit(1)
                            Spacer()
                            Text(String(format: "%.0f%%", entry.percentage))
                                .font(.system(size: 10, weight: .medium, design: .monospaced))
                                .foregroundColor(.secondary)
                        }
                    }
                }
                .padding(.horizontal, 12)
                .padding(.top, 10)
            }

            Spacer(minLength: 8)

            Divider().padding(.horizontal, 12)

            // Footer buttons
            HStack(spacing: 12) {
                Button(action: { onOpenMainWindow?() }) {
                    Label("Dashboard", systemImage: "rectangle.on.rectangle")
                        .font(.system(size: 11))
                }
                .buttonStyle(.plain)
                .foregroundColor(.primary)

                Spacer()

                Button(action: { NSApplication.shared.terminate(nil) }) {
                    Text("Quit")
                        .font(.system(size: 11))
                        .foregroundColor(.secondary)
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
        }
        .frame(width: 280, height: 380)
        .onAppear { viewModel.refresh() }
    }

    private func formatCompact(_ value: UInt64) -> String {
        if value >= 1_000_000_000 { return String(format: "%.1fB", Double(value) / 1_000_000_000) }
        if value >= 1_000_000 { return String(format: "%.1fM", Double(value) / 1_000_000) }
        if value >= 1_000 { return String(format: "%.1fK", Double(value) / 1_000) }
        return "\(value)"
    }

    private func formatCost(_ value: Double) -> String {
        tvFormatCost(value)
    }

    private func modelColor(_ source: String) -> Color {
        switch source {
        case "claude": return .orange
        case "kiro": return .green
        case "codex": return .blue
        case "cursor": return .purple
        case "gemini": return .cyan
        default: return .gray
        }
    }
}

struct StatCard: View {
    let title: String
    let value: String
    let icon: String
    let color: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 4) {
                Image(systemName: icon)
                    .font(.system(size: 9))
                    .foregroundColor(color.opacity(0.8))
                Text(title)
                    .font(.system(size: 9, weight: .medium))
                    .foregroundColor(.secondary)
            }
            Text(value)
                .font(.system(size: 15, weight: .bold, design: .monospaced))
                .foregroundColor(.primary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .background(
            RoundedRectangle(cornerRadius: 8)
                .fill(color.opacity(0.06))
                .overlay(
                    RoundedRectangle(cornerRadius: 8)
                        .strokeBorder(color.opacity(0.12), lineWidth: 0.5)
                )
        )
    }
}

struct SparklineView: View {
    let data: [Double]

    var body: some View {
        GeometryReader { geo in
            let maxVal = data.max() ?? 1
            let minVal: Double = 0
            let range = max(maxVal - minVal, 1)
            Path { path in
                for (i, val) in data.enumerated() {
                    let x = geo.size.width * CGFloat(i) / CGFloat(max(data.count - 1, 1))
                    let y = geo.size.height * (1 - CGFloat((val - minVal) / range))
                    if i == 0 { path.move(to: CGPoint(x: x, y: y)) }
                    else { path.addLine(to: CGPoint(x: x, y: y)) }
                }
            }
            .stroke(Color.green, style: StrokeStyle(lineWidth: 1.5, lineCap: .round, lineJoin: .round))
        }
    }
}
