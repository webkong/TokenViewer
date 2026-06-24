import SwiftUI

struct SkillListView: View {
    @ObservedObject var viewModel: SkillManagerViewModel

    var body: some View {
        ScrollView {
            LazyVStack(spacing: 0, pinnedViews: .sectionFooters) {
                ForEach(viewModel.filteredSkills) { skill in
                    HStack(alignment: .top, spacing: 16) {
                        infoColumn(skill)
                        actionColumn(skill)
                        agentColumn(skill)
                    }
                    .padding(.horizontal, 20)
                    .padding(.vertical, 12)
                    Divider().padding(.leading, 20)
                }
            }
        }
    }

    private func infoColumn(_ skill: SkillEntry) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(skill.manifest.name)
                .font(.headline)
                .lineLimit(1)
            Text(skill.manifest.description)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)

            HStack(spacing: 6) {
                if skill.manifest.version != "unknown" {
                    Text("v\(skill.manifest.version)")
                        .font(.caption2.weight(.semibold))
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(.quaternary, in: Capsule())
                }
                Text("Source root")
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.blue)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(Color.blue.opacity(0.08), in: Capsule())
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func actionColumn(_ skill: SkillEntry) -> some View {
        VStack(spacing: 6) {
            Button {
                viewModel.organize(skill: skill)
            } label: {
                Text("Organize")
                    .font(.caption.weight(.semibold))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 5)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.small)

            Button {
                viewModel.restore(skill: skill)
            } label: {
                Text("Restore")
                    .font(.caption.weight(.semibold))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 5)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .tint(.orange)

            Button(role: .destructive) {
                viewModel.delete(skill: skill)
            } label: {
                Text("Delete")
                    .font(.caption.weight(.semibold))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 5)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .frame(width: 100)
    }

    private func agentColumn(_ skill: SkillEntry) -> some View {
        AgentTagClusterView(skill: skill, agents: viewModel.agents)
    }
}

struct AgentTagClusterView: View {
    let skill: SkillEntry
    let agents: [SkillAgent]

    var body: some View {
        let visible = agents.prefix(10)
        let overflow = max(0, agents.count - 10)

        SkillTagFlowLayout(itemSpacing: 6, rowSpacing: 6) {
            ForEach(visible) { agent in
                let isCompatible = skill.manifest.compatibleAgents.contains(agent.id)
                Text(agent.name)
                    .font(.caption2.weight(.semibold))
                    .lineLimit(1)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(isCompatible ? Color.green.opacity(0.14) : Color.secondary.opacity(0.08), in: Capsule())
                    .foregroundStyle(isCompatible ? .green : .secondary)
            }
            if overflow > 0 {
                Text("+\(overflow)")
                    .font(.caption2.weight(.semibold))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(.quaternary, in: Capsule())
            }
        }
        .frame(maxWidth: 280, alignment: .leading)
    }
}

/// Simple flex-wrap layout
struct SkillTagFlowLayout: Layout {
    var itemSpacing: CGFloat = 6
    var rowSpacing: CGFloat = 6

    struct Cache {
        var sizes: [CGSize] = []
    }

    func makeCache(subviews: Subviews) -> Cache {
        Cache(sizes: subviews.map { $0.sizeThatFits(.unspecified) })
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) -> CGSize {
        let width = proposal.width ?? 280
        var y: CGFloat = 0
        var x: CGFloat = 0
        var maxRowH: CGFloat = 0
        for (i, subview) in subviews.enumerated() {
            let size = cache.sizes[safe: i] ?? subview.sizeThatFits(.unspecified)
            if x + size.width > width, x > 0 {
                y += maxRowH + rowSpacing
                x = 0
                maxRowH = 0
            }
            x += size.width + itemSpacing
            maxRowH = max(maxRowH, size.height)
        }
        return CGSize(width: width, height: y + maxRowH)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) {
        let width = bounds.width
        var y: CGFloat = bounds.minY
        var x: CGFloat = bounds.minX
        var maxRowH: CGFloat = 0
        for (i, subview) in subviews.enumerated() {
            let size = cache.sizes[safe: i] ?? subview.sizeThatFits(.unspecified)
            if x + size.width > width + bounds.minX, x > bounds.minX {
                y += maxRowH + rowSpacing
                x = bounds.minX
                maxRowH = 0
            }
            subview.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + itemSpacing
            maxRowH = max(maxRowH, size.height)
        }
    }
}

extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
