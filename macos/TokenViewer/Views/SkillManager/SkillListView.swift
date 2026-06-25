import AppKit
import SwiftUI

struct SkillListView: View {
    @ObservedObject var viewModel: SkillManagerViewModel
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        List(filteredSkills) { skill in
            SkillRowView(skill: skill, viewModel: viewModel)
                .padding(.vertical, 2)
        }
        .listStyle(.inset)
    }

    private var filteredSkills: [SkillEntry] {
        let skills = viewModel.filteredSkills
        if viewModel.selectedFilter == "all" { return skills }
        // Already filtered by viewModel.filteredSkills
        return skills
    }
}

// MARK: - Skill Row

struct SkillRowView: View {
    let skill: SkillEntry
    @ObservedObject var viewModel: SkillManagerViewModel
    @ObservedObject private var l10n = L10n.shared

    private let actionColumnWidth: CGFloat = 112
    private let agentsColumnWidth: CGFloat = 300

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            skillInfo
                .frame(maxWidth: .infinity, alignment: .leading)
                .layoutPriority(1)

            actionButtons
                .frame(width: actionColumnWidth, alignment: .leading)

            agentLinkTags
                .frame(width: agentsColumnWidth, alignment: .leading)
        }
        .padding(.vertical, 4)
        .alignmentGuide(.listRowSeparatorLeading) { _ in 0 }
        .alignmentGuide(.listRowSeparatorTrailing) { dimensions in dimensions[.trailing] }
    }

    private var skillInfo: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text(skill.manifest.name)
                    .fontWeight(.medium)
                    .lineLimit(1)
                if skill.manifest.version != "unknown" {
                    Text("v\(skill.manifest.version)")
                        .font(.caption2)
                        .padding(.horizontal, 4).padding(.vertical, 1)
                        .background(.quaternary, in: Capsule())
                        .foregroundStyle(.secondary)
                }
                sourceBadge
            }

            Text(skill.manifest.description)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)

            if !skill.manifest.tags.isEmpty {
                HStack(spacing: 4) {
                    ForEach(Array(skill.manifest.tags.prefix(5)), id: \.self) { tag in
                        Text(tag)
                            .font(.caption2)
                            .padding(.horizontal, 5).padding(.vertical, 1)
                            .background(.blue.opacity(0.08), in: Capsule())
                            .foregroundStyle(.blue)
                    }
                }
            }
        }
    }

    // MARK: - Source Badge

    @ViewBuilder
    private var sourceBadge: some View {
        if viewModel.isInSourceRoot(skill) {
            Text(l10n.skillGlobalBadge)
                .font(.caption2)
                .padding(.horizontal, 5).padding(.vertical, 1)
                .background(.blue.opacity(0.1), in: Capsule())
                .foregroundStyle(.blue)
        }
        if let sourceAgent = viewModel.sourceAgent(for: skill) {
            if !viewModel.isInSourceRoot(skill) {
                Text(TVColor.sourceDisplayName(sourceAgent))
                    .font(.caption2)
                    .padding(.horizontal, 5).padding(.vertical, 1)
                    .background(.gray.opacity(0.1), in: Capsule())
                    .foregroundStyle(.secondary)
            }
            if sourceAgent == "claude" {
                Text(TVColor.sourceDisplayName("claude"))
                    .font(.caption2)
                    .padding(.horizontal, 5).padding(.vertical, 1)
                    .background(.orange.opacity(0.1), in: Capsule())
                    .foregroundStyle(.orange)
            }
        }
    }

    // MARK: - Action Buttons

    private var actionButtons: some View {
        VStack(alignment: .leading, spacing: 6) {
            if !viewModel.isInSourceRoot(skill), let sourceAgent = viewModel.sourceAgent(for: skill) {
                let displayName = TVColor.sourceDisplayName(sourceAgent)
                Button {
                    viewModel.organize(skill: skill, agentID: sourceAgent)
                } label: {
                    Label(L10n.shared.skillOrganize, systemImage: "arrow.triangle.swap")
                        .font(.caption.weight(.medium))
                        .padding(.horizontal, 10).padding(.vertical, 5)
                        .foregroundStyle(.blue)
                        .background(.blue.opacity(0.10), in: Capsule())
                        .overlay(Capsule().strokeBorder(.blue.opacity(0.18), lineWidth: 0.5))
                }
                .buttonStyle(.plain)
                .quickHelp(l10n.skillOrganizeTip(displayName))
            } else if viewModel.isInSourceRoot(skill), let sourceAgent = viewModel.sourceAgent(for: skill) {
                let displayName = TVColor.sourceDisplayName(sourceAgent)
                Button {
                    viewModel.restore(skill: skill, agentID: sourceAgent)
                } label: {
                    Label(L10n.shared.skillRestore, systemImage: "arrow.uturn.backward")
                        .font(.caption.weight(.medium))
                        .padding(.horizontal, 10).padding(.vertical, 5)
                        .foregroundStyle(.orange)
                        .background(.orange.opacity(0.10), in: Capsule())
                        .overlay(Capsule().strokeBorder(.orange.opacity(0.18), lineWidth: 0.5))
                }
                .buttonStyle(.plain)
                .quickHelp(l10n.skillRestoreTip(displayName))
            }
            Button(role: .destructive) {
                viewModel.delete(skill: skill)
            } label: {
                Label(L10n.shared.skillDelete, systemImage: "trash")
                    .font(.caption.weight(.medium))
                    .padding(.horizontal, 10).padding(.vertical, 5)
                    .foregroundStyle(.red)
                    .background(.red.opacity(0.10), in: Capsule())
                    .overlay(Capsule().strokeBorder(.red.opacity(0.18), lineWidth: 0.5))
            }
            .buttonStyle(.plain)
            .quickHelp(l10n.skillDeleteTip)
        }
    }

    // MARK: - Agent Link Tags

    private var agentLinkTags: some View {
        let agents = viewModel.visibleProviders
        let linked = agents.filter { viewModel.isSkillLinked(skillID: skill.id, agentID: $0.source) }
        let unlinked = agents.filter { !linked.contains($0) }
        let sourceAgent = viewModel.sourceAgent(for: skill)

        return Group {
            if agents.isEmpty {
                Text(l10n.skillNoAgentsEnabled).font(.caption2).foregroundStyle(.secondary)
            } else {
                AgentTagClusterView(
                    agents: linked + unlinked,
                    maxWidth: agentsColumnWidth
                ) { agent in
                    agentLinkChip(
                        agent: agent,
                        isLinked: linked.contains(agent),
                        isSource: agent.source == sourceAgent && !linked.contains(agent)
                    )
                }
            }
        }
    }

    private struct AgentTagClusterView<Content: View>: View {
        let agents: [SkillProvider]
        let maxWidth: CGFloat
        let tagContent: (SkillProvider) -> Content

        private let itemSpacing: CGFloat = 4
        private let rowSpacing: CGFloat = 4

        var body: some View {
            let layout = AgentTagClusterLayout(
                agents: agents,
                maxWidth: maxWidth,
                itemSpacing: itemSpacing
            )

            VStack(alignment: .leading, spacing: rowSpacing) {
                ForEach(Array(layout.rows.enumerated()), id: \.offset) { index, rowAgents in
                    HStack(spacing: itemSpacing) {
                        ForEach(rowAgents) { agent in
                            tagContent(agent)
                        }

                        if index == layout.rows.count - 1, layout.hiddenCount > 0 {
                            Text("+\(layout.hiddenCount)")
                                .font(.caption2)
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .background(.gray.opacity(0.12), in: Capsule())
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private struct AgentTagClusterLayout {
        let rows: [[SkillProvider]]
        let hiddenCount: Int

        init(agents: [SkillProvider], maxWidth: CGFloat, itemSpacing: CGFloat) {
            let badgeFont = NSFont.systemFont(ofSize: NSFont.smallSystemFontSize)
            let badgeWidths = agents.map { agent in
                let textWidth = agent.displayName.size(withAttributes: [.font: badgeFont]).width
                return ceil(textWidth + 37)
            }

            var firstRow: [SkillProvider] = []
            var secondRow: [SkillProvider] = []
            var firstWidth: CGFloat = 0
            var secondWidth: CGFloat = 0
            var hidden = 0

            func rowWidth(_ current: CGFloat, adding itemWidth: CGFloat, isEmpty: Bool) -> CGFloat {
                isEmpty ? itemWidth : current + itemSpacing + itemWidth
            }

            func overflowWidth(for hiddenCount: Int) -> CGFloat {
                let text = "+\(hiddenCount)"
                let width = text.size(withAttributes: [.font: badgeFont]).width
                return ceil(width + 16)
            }

            for (index, agent) in agents.enumerated() {
                let itemWidth = badgeWidths[index]
                let remainingAfter = agents.count - index - 1

                let firstCandidate = rowWidth(firstWidth, adding: itemWidth, isEmpty: firstRow.isEmpty)
                let firstReserve = remainingAfter > 0 ? itemSpacing + overflowWidth(for: remainingAfter) : 0
                if firstCandidate + firstReserve <= maxWidth {
                    firstRow.append(agent)
                    firstWidth = firstCandidate
                    continue
                }

                let secondCandidate = rowWidth(secondWidth, adding: itemWidth, isEmpty: secondRow.isEmpty)
                let secondReserve = remainingAfter > 0 ? itemSpacing + overflowWidth(for: remainingAfter) : 0
                if secondCandidate + secondReserve <= maxWidth {
                    secondRow.append(agent)
                    secondWidth = secondCandidate
                    continue
                }

                hidden = agents.count - index
                break
            }

            let computedRows = [firstRow, secondRow].filter { !$0.isEmpty }
            rows = computedRows.isEmpty ? [[]] : computedRows
            hiddenCount = hidden
        }
    }

    private func agentLinkChip(agent: SkillProvider, isLinked: Bool, isSource: Bool) -> some View {
        Button {
            if isLinked {
                viewModel.unlinkSkill(skillID: skill.id, agentID: agent.source)
            } else {
                viewModel.linkSkill(skillID: skill.id, agentID: agent.source)
            }
        } label: {
            HStack(spacing: 3) {
                ProviderIcon(source: agent.source, size: 12)
                Text(agent.displayName)
                    .font(.caption2)
            }
            .padding(.horizontal, 6).padding(.vertical, 2)
            .background(linkBackground(isLinked: isLinked, isSource: isSource))
            .foregroundStyle(linkForeground(isLinked: isLinked, isSource: isSource))
            .clipShape(Capsule())
        }
        .buttonStyle(.plain)
        .quickHelp(linkTooltip(isLinked: isLinked, isSource: isSource, agent: agent))
    }

    private func linkBackground(isLinked: Bool, isSource: Bool) -> Color {
        if isLinked { return Color.green.opacity(0.15) }
        if isSource { return Color.purple.opacity(0.1) }
        return Color.gray.opacity(0.1)
    }

    private func linkForeground(isLinked: Bool, isSource: Bool) -> Color {
        if isLinked { return .green }
        if isSource { return .purple }
        return .secondary
    }

    private func linkTooltip(isLinked: Bool, isSource: Bool, agent: SkillProvider) -> String {
        if isLinked { return l10n.skillUnlinkTip(agent.displayName) }
        if isSource { return l10n.skillSourceLinkTip(agent.displayName) }
        return l10n.skillLinkTip(agent.displayName)
    }
}
