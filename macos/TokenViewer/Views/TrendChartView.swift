import SwiftUI

/// Smooth multi-series area/line chart matching the original "Usage Trend":
/// input / output / cache-write / cache-read on the token axis (left),
/// cost on a secondary axis (right, dashed). X axis shows day or hour ticks.
struct TrendChartView: View {
    let data: [DailyPoint]
    let hourly: Bool
    @State private var hoverIndex: Int?

    private struct Series { let name: String; let color: Color; let values: [Double]; let dashed: Bool; let cost: Bool }

    private var tokenSeries: [Series] {
        [
            Series(name: "Input", color: .blue, values: data.map { Double($0.input_tokens) }, dashed: false, cost: false),
            Series(name: "Output", color: .green, values: data.map { Double($0.output_tokens) }, dashed: false, cost: false),
            Series(name: "Cache Write", color: .orange, values: data.map { Double($0.cache_creation_input_tokens) }, dashed: false, cost: false),
            Series(name: "Cache Read", color: .purple, values: data.map { Double($0.cached_input_tokens) }, dashed: false, cost: false),
        ].filter { $0.values.contains { $0 > 0 } }
    }
    private var costSeries: Series {
        Series(name: "Cost", color: .red, values: data.map { $0.total_cost_usd }, dashed: true, cost: true)
    }

    private var tokenMax: Double { max(tokenSeries.flatMap { $0.values }.max() ?? 1, 1) }
    private var costMax: Double { max(costSeries.values.max() ?? 0.0001, 0.0001) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Usage Trend").font(.system(size: 15, weight: .semibold))
                Spacer()
                Text(hourly ? "by hour" : "by day").font(.system(size: 11)).foregroundStyle(.secondary)
            }

            HStack(spacing: 6) {
                yAxisLabels(maxVal: tokenMax, formatter: { tvFormatTokens(UInt64(max($0, 0))) }, align: .trailing)
                GeometryReader { geo in
                    ZStack(alignment: .topLeading) {
                        gridLines(geo)
                        // token series
                        ForEach(Array(tokenSeries.enumerated()), id: \.offset) { idx, s in
                            if idx == 0 { areaPath(s.values, max: tokenMax, in: geo).fill(
                                LinearGradient(colors: [s.color.opacity(0.25), s.color.opacity(0.02)], startPoint: .top, endPoint: .bottom))
                            }
                            smoothPath(s.values, max: tokenMax, in: geo)
                                .stroke(s.color, style: StrokeStyle(lineWidth: 1.8, lineCap: .round, lineJoin: .round))
                        }
                        // cost series (right axis)
                        smoothPath(costSeries.values, max: costMax, in: geo)
                            .stroke(costSeries.color, style: StrokeStyle(lineWidth: 1.6, lineCap: .round, dash: [5, 4]))

                        // hover marker + tooltip
                        if let i = hoverIndex, i < data.count {
                            let x = point(i, 0, 1, geo).x
                            Path { p in p.move(to: CGPoint(x: x, y: 0)); p.addLine(to: CGPoint(x: x, y: geo.size.height)) }
                                .stroke(Color.gray.opacity(0.4), style: StrokeStyle(lineWidth: 1, dash: [3, 3]))
                            tooltip(for: data[i], at: x, width: geo.size.width)
                        }
                    }
                    .contentShape(Rectangle())
                    .onContinuousHover { phase in
                        switch phase {
                        case .active(let loc):
                            let n = max(data.count - 1, 1)
                            let i = Int((loc.x / geo.size.width * CGFloat(n)).rounded())
                            hoverIndex = min(max(i, 0), data.count - 1)
                        case .ended:
                            hoverIndex = nil
                        }
                    }
                }
                .frame(height: 160)
                yAxisLabels(maxVal: costMax, formatter: { tvFormatCost($0) }, align: .leading)
            }

            xAxisLabels()
            legend()
        }
        .tvCard()
    }

    // MARK: chart paths

    /// Detail tooltip for the hovered point, including cache hit rate.
    private func tooltip(for d: DailyPoint, at x: CGFloat, width: CGFloat) -> some View {
        let cache = d.cached_input_tokens + d.cache_creation_input_tokens
        let denom = d.input_tokens + d.cached_input_tokens
        let hit = denom > 0 ? Double(d.cached_input_tokens) / Double(denom) * 100 : 0
        return VStack(alignment: .leading, spacing: 2) {
            Text(formatTick(d.date)).font(.system(size: 10, weight: .bold))
            row("Input", d.input_tokens, .blue)
            row("Output", d.output_tokens, .green)
            row("Cache", cache, .orange)
            if d.reasoning_output_tokens > 0 { row("Reason", d.reasoning_output_tokens, .purple) }
            HStack(spacing: 6) {
                Circle().fill(.red).frame(width: 6, height: 6)
                Text("Cost").font(.system(size: 9)).foregroundStyle(.secondary)
                Text(tvFormatCost(d.total_cost_usd)).font(.system(size: 9, design: .monospaced))
            }
            HStack(spacing: 6) {
                Text("Cache hit").font(.system(size: 9)).foregroundStyle(.secondary)
                Text(String(format: "%.1f%%", hit)).font(.system(size: 9, weight: .bold, design: .monospaced)).foregroundStyle(.orange)
            }
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color(nsColor: .windowBackgroundColor))
            .shadow(color: .black.opacity(0.15), radius: 4))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(.quaternary, lineWidth: 0.5))
        .fixedSize()
        .offset(x: min(max(x - 50, 0), width - 110), y: 4)
    }

    private func row(_ label: String, _ v: UInt64, _ color: Color) -> some View {
        HStack(spacing: 6) {
            Circle().fill(color).frame(width: 6, height: 6)
            Text(label).font(.system(size: 9)).foregroundStyle(.secondary)
            Text(tvFormatTokens(v)).font(.system(size: 9, design: .monospaced))
        }
    }

    private func point(_ i: Int, _ v: Double, _ maxV: Double, _ geo: GeometryProxy) -> CGPoint {
        let n = max(data.count - 1, 1)
        let x = geo.size.width * CGFloat(i) / CGFloat(n)
        let y = geo.size.height * (1 - CGFloat(v / maxV))
        return CGPoint(x: x, y: y)
    }

    private func smoothPath(_ values: [Double], max maxV: Double, in geo: GeometryProxy) -> Path {
        let pts = values.enumerated().map { point($0.offset, $0.element, maxV, geo) }
        return catmullRom(pts)
    }

    private func areaPath(_ values: [Double], max maxV: Double, in geo: GeometryProxy) -> Path {
        var p = catmullRom(values.enumerated().map { point($0.offset, $0.element, maxV, geo) })
        guard let first = values.first, !values.isEmpty else { return p }
        _ = first
        p.addLine(to: CGPoint(x: geo.size.width, y: geo.size.height))
        p.addLine(to: CGPoint(x: 0, y: geo.size.height))
        p.closeSubpath()
        return p
    }

    /// Catmull-Rom spline through points → smooth Bezier path.
    private func catmullRom(_ pts: [CGPoint]) -> Path {
        var path = Path()
        guard pts.count > 1 else {
            if let p = pts.first { path.move(to: p); path.addLine(to: p) }
            return path
        }
        path.move(to: pts[0])
        for i in 0..<pts.count - 1 {
            let p0 = i == 0 ? pts[i] : pts[i - 1]
            let p1 = pts[i]
            let p2 = pts[i + 1]
            let p3 = i + 2 < pts.count ? pts[i + 2] : p2
            let c1 = CGPoint(x: p1.x + (p2.x - p0.x) / 6, y: p1.y + (p2.y - p0.y) / 6)
            let c2 = CGPoint(x: p2.x - (p3.x - p1.x) / 6, y: p2.y - (p3.y - p1.y) / 6)
            path.addCurve(to: p2, control1: c1, control2: c2)
        }
        return path
    }

    private func gridLines(_ geo: GeometryProxy) -> some View {
        Path { p in
            for i in 0...4 {
                let y = geo.size.height * CGFloat(i) / 4
                p.move(to: CGPoint(x: 0, y: y)); p.addLine(to: CGPoint(x: geo.size.width, y: y))
            }
        }.stroke(Color.gray.opacity(0.12), lineWidth: 0.5)
    }

    // MARK: axes & legend

    private func yAxisLabels(maxVal: Double, formatter: @escaping (Double) -> String, align: HorizontalAlignment) -> some View {
        VStack(alignment: align, spacing: 0) {
            ForEach(0..<5) { i in
                Text(formatter(maxVal * Double(4 - i) / 4))
                    .font(.system(size: 9, design: .monospaced)).foregroundStyle(.tertiary)
                if i < 4 { Spacer() }
            }
        }
        .frame(width: 42, height: 160)
    }

    private func xAxisLabels() -> some View {
        let labels = tickLabels()
        return HStack {
            ForEach(Array(labels.enumerated()), id: \.offset) { _, label in
                Text(label).font(.system(size: 9)).foregroundStyle(.tertiary)
                    .frame(maxWidth: .infinity, alignment: .center)
            }
        }
        .padding(.leading, 48).padding(.trailing, 48)
    }

    private func tickLabels() -> [String] {
        guard !data.isEmpty else { return [] }
        let count = min(data.count, 6)
        let step = max(data.count / count, 1)
        var out: [String] = []
        var i = 0
        while i < data.count {
            out.append(formatTick(data[i].date))
            i += step
        }
        return out
    }

    private func formatTick(_ raw: String) -> String {
        if hourly {
            // "YYYY-MM-DDTHH" → "HH:00"
            if let t = raw.split(separator: "T").last { return "\(t):00" }
            return raw
        }
        // "YYYY-MM-DD" → "MM/DD"
        let parts = raw.split(separator: "-")
        return parts.count == 3 ? "\(parts[1])/\(parts[2])" : raw
    }

    private func legend() -> some View {
        HStack(spacing: 14) {
            ForEach(Array((tokenSeries + [costSeries]).enumerated()), id: \.offset) { _, s in
                HStack(spacing: 4) {
                    Circle().fill(s.color).frame(width: 7, height: 7)
                    Text(s.name).font(.system(size: 10)).foregroundStyle(.secondary)
                }
            }
            Spacer()
        }
    }
}
