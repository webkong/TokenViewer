import SwiftUI

struct SkillListView: View {
    @ObservedObject var viewModel: SkillManagerViewModel
    @ObservedObject private var l10n = L10n.shared

    private let horizontalPadding: CGFloat = 30

    var body: some View {
        VStack(spacing: 0) {
            SkillListHeader()
                .padding(.horizontal, horizontalPadding)

            Divider()
                .padding(.horizontal, horizontalPadding)

            ScrollView(.vertical, showsIndicators: false) {
                LazyVStack(spacing: 0) {
                    ForEach(Array(filteredSkills.enumerated()), id: \.element.id) { index, skill in
                        SkillRowView(skill: skill, viewModel: viewModel)
                            .padding(.vertical, 2)
                            .padding(.horizontal, horizontalPadding)

                        if index < filteredSkills.count - 1 {
                            Divider()
                                .padding(.horizontal, horizontalPadding)
                        }
                    }
                }
            }
        }
    }

    private var filteredSkills: [SkillEntry] {
        let skills = viewModel.filteredSkills
        if viewModel.selectedFilter == "all" { return skills }
        // Already filtered by viewModel.filteredSkills
        return skills
    }
}

private enum SkillListMetrics {
    static let columnSpacing: CGFloat = 12
    static let actionColumnWidth: CGFloat = 92
    static let agentsColumnWidth: CGFloat = 300
    static let columnInset: CGFloat = 14
}

private struct SkillListHeader: View {
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        HStack(alignment: .center, spacing: SkillListMetrics.columnSpacing) {
            Text(l10n.skillColumnSkill)
                .frame(maxWidth: .infinity, alignment: .leading)
                .layoutPriority(1)

            Text(l10n.skillColumnActions)
                .padding(.leading, SkillListMetrics.columnInset)
                .frame(width: SkillListMetrics.actionColumnWidth, alignment: .leading)
                .overlay(alignment: .leading) { columnDivider }
                .overlay(alignment: .trailing) { columnDivider }

            Text(l10n.skillColumnAgents)
                .padding(.leading, SkillListMetrics.columnInset)
                .frame(width: SkillListMetrics.agentsColumnWidth, alignment: .leading)
        }
        .font(.caption2.weight(.semibold))
        .foregroundStyle(.secondary)
        .textCase(.uppercase)
        .padding(.vertical, 7)
    }

    private var columnDivider: some View {
        Rectangle()
            .fill(Color(nsColor: .separatorColor).opacity(0.85))
            .frame(width: 1, height: 18)
    }
}

// MARK: - Skill Row

struct SkillRowView: View {
    let skill: SkillEntry
    @ObservedObject var viewModel: SkillManagerViewModel
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        HStack(alignment: .top, spacing: SkillListMetrics.columnSpacing) {
            skillInfo
                .frame(maxWidth: .infinity, alignment: .leading)
                .layoutPriority(1)

            actionButtons
                .padding(.leading, SkillListMetrics.columnInset)
                .frame(width: SkillListMetrics.actionColumnWidth, alignment: .leading)
                .overlay(alignment: .leading) { columnDivider }
                .overlay(alignment: .trailing) { columnDivider }

            agentLinkTags
                .padding(.leading, SkillListMetrics.columnInset)
                .frame(width: SkillListMetrics.agentsColumnWidth, alignment: .leading)
        }
        .padding(.vertical, 4)
        .alignmentGuide(.listRowSeparatorLeading) { _ in 0 }
        .alignmentGuide(.listRowSeparatorTrailing) { dimensions in dimensions[.trailing] }
    }

    private var columnDivider: some View {
        Rectangle()
            .fill(Color(nsColor: .separatorColor).opacity(0.75))
            .frame(width: 1)
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
                Text(ProviderRegistry.shared.displayName(for: sourceAgent))
                    .font(.caption2)
                    .padding(.horizontal, 5).padding(.vertical, 1)
                    .background(.gray.opacity(0.1), in: Capsule())
                    .foregroundStyle(.secondary)
            }
            if sourceAgent == "claude" {
                Text(ProviderRegistry.shared.displayName(for: "claude"))
                    .font(.caption2)
                    .padding(.horizontal, 5).padding(.vertical, 1)
                    .background(.orange.opacity(0.1), in: Capsule())
                    .foregroundStyle(.orange)
            }
        }
    }

    // MARK: - Action Buttons

    private var actionButtons: some View {
        HStack(spacing: 6) {
            if !viewModel.isInSourceRoot(skill), let sourceAgent = viewModel.sourceAgent(for: skill) {
                let displayName = ProviderRegistry.shared.displayName(for: sourceAgent)
                Button {
                    viewModel.organize(skill: skill, agentID: sourceAgent)
                } label: {
                    Image(systemName: "arrow.triangle.swap")
                        .font(.system(size: 13, weight: .semibold))
                        .frame(width: 26, height: 22)
                        .foregroundStyle(.blue)
                        .background(.blue.opacity(0.10), in: Capsule())
                        .overlay(Capsule().strokeBorder(.blue.opacity(0.18), lineWidth: 0.5))
                }
                .buttonStyle(.plain)
                .quickHelp(l10n.skillOrganizeTip(displayName))
            } else if viewModel.isInSourceRoot(skill), let sourceAgent = viewModel.sourceAgent(for: skill) {
                let displayName = ProviderRegistry.shared.displayName(for: sourceAgent)
                Button {
                    viewModel.restore(skill: skill, agentID: sourceAgent)
                } label: {
                    Image(systemName: "arrow.uturn.backward")
                        .font(.system(size: 13, weight: .semibold))
                        .frame(width: 26, height: 22)
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
                Image(systemName: "trash")
                    .font(.system(size: 13, weight: .semibold))
                    .frame(width: 26, height: 22)
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
        let activeAgentIDs = viewModel.skillAgentIDs(for: skill)
        let linked = agents.filter { viewModel.isSkillLinked(skillID: skill.id, agentID: $0.source) }
        let active = agents.filter { activeAgentIDs.contains($0.source) && !linked.contains($0) }
        let inactive = agents.filter { !activeAgentIDs.contains($0.source) }

        return Group {
            if agents.isEmpty {
                Text(l10n.skillNoAgentsEnabled).font(.caption2).foregroundStyle(.secondary)
            } else {
                FlowLayout(itemSpacing: 4, rowSpacing: 4) {
                    ForEach(linked + active + inactive) { agent in
                        agentLinkChip(
                            agent: agent,
                            isLinked: linked.contains(agent),
                            isSource: active.contains(agent)
                        )
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
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
