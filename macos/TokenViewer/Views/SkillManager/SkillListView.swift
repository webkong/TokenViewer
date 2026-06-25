import SwiftUI

struct SkillListView: View {
    @ObservedObject var viewModel: SkillManagerViewModel

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

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .top, spacing: 12) {
                // Left: Info
                VStack(alignment: .leading, spacing: 3) {
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
                        .lineLimit(2)
                }
                .frame(maxWidth: .infinity, alignment: .leading)

                // Right: Actions
                actionButtons
            }

            // Tags row
            if !skill.manifest.tags.isEmpty {
                HStack(spacing: 4) {
                    ForEach(skill.manifest.tags, id: \.self) { tag in
                        Text(tag)
                            .font(.caption2)
                            .padding(.horizontal, 5).padding(.vertical, 1)
                            .background(.blue.opacity(0.08), in: Capsule())
                            .foregroundStyle(.blue)
                    }
                }
            }

            // Agent link tags row
            agentLinkTags
        }
        .padding(.vertical, 2)
    }

    // MARK: - Source Badge

    @ViewBuilder
    private var sourceBadge: some View {
        if let sourceAgent = viewModel.sourceAgent(for: skill.id) {
            Text("Global")
                .font(.caption2)
                .padding(.horizontal, 5).padding(.vertical, 1)
                .background(.blue.opacity(0.1), in: Capsule())
                .foregroundStyle(.blue)
            if sourceAgent == "claude" {
                Text("Claude Code")
                    .font(.caption2)
                    .padding(.horizontal, 5).padding(.vertical, 1)
                    .background(.orange.opacity(0.1), in: Capsule())
                    .foregroundStyle(.orange)
            }
        }
    }

    // MARK: - Action Buttons

    private var actionButtons: some View {
        VStack(spacing: 4) {
            if viewModel.sourceAgent(for: skill.id) == nil {
                // Not organized yet — show Organize
                Button {
                    viewModel.organize(skill: skill)
                } label: {
                    Label("Organize", systemImage: "arrow.triangle.swap")
                        .font(.caption.weight(.medium))
                        .padding(.horizontal, 10).padding(.vertical, 5)
                        .foregroundStyle(.blue)
                        .background(.blue.opacity(0.10), in: Capsule())
                        .overlay(Capsule().strokeBorder(.blue.opacity(0.18), lineWidth: 0.5))
                }
                .buttonStyle(.plain)
            } else {
                // Organized — show Restore
                Button {
                    viewModel.restore(skill: skill)
                } label: {
                    Label("Restore", systemImage: "arrow.uturn.backward")
                        .font(.caption.weight(.medium))
                        .padding(.horizontal, 10).padding(.vertical, 5)
                        .foregroundStyle(.orange)
                        .background(.orange.opacity(0.10), in: Capsule())
                        .overlay(Capsule().strokeBorder(.orange.opacity(0.18), lineWidth: 0.5))
                }
                .buttonStyle(.plain)
            }
            Button(role: .destructive) {
                viewModel.delete(skill: skill)
            } label: {
                Label("Delete", systemImage: "trash")
                    .font(.caption.weight(.medium))
                    .padding(.horizontal, 10).padding(.vertical, 5)
                    .foregroundStyle(.red)
                    .background(.red.opacity(0.10), in: Capsule())
                    .overlay(Capsule().strokeBorder(.red.opacity(0.18), lineWidth: 0.5))
            }
            .buttonStyle(.plain)
        }
    }

    // MARK: - Agent Link Tags

    private var agentLinkTags: some View {
        let agents = viewModel.visibleProviders
        let linked = agents.filter { viewModel.isSkillLinked(skillID: skill.id, agentID: $0.source) }
        let unlinked = agents.filter { !linked.contains($0) }
        let sourceAgent = viewModel.sourceAgent(for: skill.id)

        return Group {
            if agents.isEmpty {
                Text("No agents enabled").font(.caption2).foregroundStyle(.secondary)
            } else {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 4) {
                        ForEach(linked) { agent in
                            agentLinkChip(agent: agent, isLinked: true, isSource: agent.source == sourceAgent)
                        }
                        ForEach(unlinked) { agent in
                            agentLinkChip(agent: agent, isLinked: false, isSource: false)
                        }
                    }
                }
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
        .help(linkTooltip(isLinked: isLinked, isSource: isSource, agent: agent))
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
        if isLinked { return "Click to remove symlink for \(agent.displayName)" }
        if isSource { return "Source skill in \(agent.displayName) — click to create symlink" }
        return "Click to create symlink for \(agent.displayName)"
    }
}
