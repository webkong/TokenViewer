import SwiftUI

struct SkillListView: View {
    @ObservedObject var viewModel: SkillManagerViewModel
    @ObservedObject private var l10n = L10n.shared
    @State private var preview: SkillMarkdownPreview?

    private let horizontalPadding: CGFloat = 30

    var body: some View {
        VStack(spacing: 0) {
            SkillListHeader(viewModel: viewModel)
                .padding(.horizontal, horizontalPadding)

            Divider()
                .padding(.horizontal, horizontalPadding)

            ScrollView(.vertical, showsIndicators: false) {
                LazyVStack(spacing: 0) {
                    ForEach(Array(filteredSkills.enumerated()), id: \.element.id) { index, skill in
                        SkillRowView(skill: skill, viewModel: viewModel) {
                            preview = viewModel.skillMarkdownPreview(for: skill)
                        }
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
        .sheet(item: $preview) { preview in
            SkillMarkdownPreviewSheet(preview: preview)
        }
        .alert(
            l10n.skillCompatTitle,
            isPresented: Binding(
                get: { viewModel.compatibilityAlert != nil },
                set: { if !$0 { viewModel.compatibilityAlert = nil } }
            )
        ) {
            Button(l10n.skillCompatConfirm) {
                if let alert = viewModel.compatibilityAlert {
                    viewModel.linkSkill(skillID: alert.skillID, agentID: alert.agentID)
                }
                viewModel.compatibilityAlert = nil
            }
            Button(l10n.gitCancel, role: .cancel) {
                viewModel.compatibilityAlert = nil
            }
        } message: {
            if let alert = viewModel.compatibilityAlert {
                Text(l10n.skillCompatWarning(alert.skillName, alert.agentName))
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
    static let actionColumnWidth: CGFloat = 104
    static let agentsColumnWidth: CGFloat = 300
    static let columnInset: CGFloat = 14
}

private struct SkillListHeader: View {
    @ObservedObject var viewModel: SkillManagerViewModel
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
        .overlay(alignment: .trailing) {
            Toggle(l10n.skillShowBuiltIn, isOn: $viewModel.showBuiltInSkills)
                .toggleStyle(.switch)
                .controlSize(.mini)
                .labelsHidden()
                .quickHelp(l10n.skillShowBuiltIn)
        }
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
    let onPreview: () -> Void
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
        .contentShape(Rectangle())
        .onTapGesture(perform: onPreview)
        .quickHelp(l10n.skillPreviewTip)
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
        // Skills shipped by an agent (no user-authored manifest.json) get a
        // "Built-in" marker so users can spot agent-scoped skills at a glance.
        if !skill.manifest.hasManifest {
            Text(l10n.skillBuiltIn)
                .font(.caption2)
                .padding(.horizontal, 5).padding(.vertical, 1)
                .background(.orange.opacity(0.12), in: Capsule())
                .foregroundStyle(.orange)
                .quickHelp(l10n.skillBuiltInTip)
        }
    }

    // MARK: - Action Buttons

    private var actionButtons: some View {
        HStack(spacing: 4) {
            if !viewModel.isInSourceRoot(skill), let sourceAgent = viewModel.sourceAgent(for: skill) {
                let displayName = ProviderRegistry.shared.displayName(for: sourceAgent)
                Button {
                    viewModel.organize(skill: skill, agentID: sourceAgent)
                } label: {
                    Image(systemName: "arrow.triangle.swap")
                        .font(.system(size: 11, weight: .semibold))
                        .frame(width: 36, height: 22)
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
                        .font(.system(size: 11, weight: .semibold))
                        .frame(width: 36, height: 22)
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
                    .font(.system(size: 11, weight: .semibold))
                    .frame(width: 36, height: 22)
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
            } else if viewModel.requiresCompatibilityConfirmation(skillID: skill.id, agentID: agent.source) {
                // Cross-agent link: the skill declares specific compatible agents
                // and this one isn't among them. Surface a confirmation alert.
                viewModel.compatibilityAlert = CompatibilityAlert(
                    skillID: skill.id,
                    agentID: agent.source,
                    skillName: skill.manifest.name,
                    agentName: agent.displayName
                )
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

private struct SkillMarkdownPreviewSheet: View {
    let preview: SkillMarkdownPreview
    @ObservedObject private var l10n = L10n.shared
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            header
                .padding(.horizontal, 20)
                .padding(.vertical, 14)

            Divider()

            ScrollView {
                Text(preview.content)
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(.primary)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(18)
            }
            .background(Color(nsColor: .textBackgroundColor))
        }
        .frame(minWidth: 680, idealWidth: 760, minHeight: 520, idealHeight: 620)
    }

    private var header: some View {
        HStack(alignment: .center, spacing: 12) {
            Image(systemName: "doc.text")
                .font(.system(size: 18, weight: .semibold))
                .foregroundStyle(.blue)
                .frame(width: 30, height: 30)
                .background(.blue.opacity(0.10), in: RoundedRectangle(cornerRadius: 6, style: .continuous))

            VStack(alignment: .leading, spacing: 3) {
                Text(preview.skill.manifest.name)
                    .font(.headline)
                    .lineLimit(1)
                Text(preview.filePath)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
            }

            Spacer()

            Button(l10n.gitDone) {
                dismiss()
            }
            .keyboardShortcut(.cancelAction)
            .quickHelp(l10n.gitDoneTip)
        }
    }
}
