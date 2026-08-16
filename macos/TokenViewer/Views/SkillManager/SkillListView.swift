import AppKit
import SwiftUI

struct SkillListView: View {
    @ObservedObject var viewModel: SkillManagerViewModel
    @ObservedObject private var l10n = L10n.shared
    @State private var preview: SkillMarkdownPreview?
    @State private var selectedSkillID: String?

    var body: some View {
        SkillWorkspaceView(
            groups: skillGroups,
            selectedSkillID: $selectedSkillID,
            viewModel: viewModel,
            onPreview: { skill in
                preview = viewModel.skillMarkdownPreview(for: skill)
            }
        )
        .onAppear(perform: ensureSelection)
        .onChange(of: filteredSkills.map(\.id)) { _, _ in ensureSelection() }
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
        .alert(
            l10n.skillBuiltInOrganizeTitle,
            isPresented: Binding(
                get: { viewModel.builtInOrganizeAlert != nil },
                set: { if !$0 { viewModel.builtInOrganizeAlert = nil } }
            )
        ) {
            Button(l10n.skillBuiltInOrganizeConfirm) {
                if let alert = viewModel.builtInOrganizeAlert {
                    viewModel.organizeSkill(skillID: alert.skillID, agentID: alert.agentID)
                }
                viewModel.builtInOrganizeAlert = nil
            }
            Button(l10n.cancel, role: .cancel) {
                viewModel.builtInOrganizeAlert = nil
            }
        } message: {
            if let alert = viewModel.builtInOrganizeAlert {
                Text(l10n.skillBuiltInOrganizeWarning(alert.skillName, alert.agentName))
            }
        }
    }

    private func ensureSelection() {
        if let selectedSkillID,
           filteredSkills.contains(where: { $0.id == selectedSkillID }) {
            return
        }
        selectedSkillID = skillGroups.first(where: { !$0.isContainer })?.skills.first?.id
    }

    private var filteredSkills: [SkillEntry] {
        let skills = viewModel.filteredSkills
        if viewModel.selectedFilter == SkillManagerViewModel.allFilter { return skills }
        // Already filtered by viewModel.filteredSkills
        return skills
    }

    private var skillGroups: [SkillListGroup] {
        var groups: [String: SkillListGroup] = [:]

        for skill in filteredSkills {
            if skill.relativePath.count > 1, let containerName = skill.relativePath.first {
                var rootURL = URL(fileURLWithPath: skill.sourceDir, isDirectory: true)
                for _ in skill.relativePath {
                    rootURL.deleteLastPathComponent()
                }
                let key = "\(rootURL.standardized.path)::\(containerName)"
                if var group = groups[key] {
                    group.skills.append(skill)
                    groups[key] = group
                } else {
                    groups[key] = SkillListGroup(
                        id: key,
                        linkID: containerName,
                        title: containerName,
                        skills: [skill],
                        isContainer: true
                    )
                }
            } else {
                let key = "skill::\(skill.sourceDir)"
                groups[key] = SkillListGroup(
                    id: key,
                    linkID: skill.id,
                    title: skill.manifest.name,
                    skills: [skill],
                    isContainer: false
                )
            }
        }

        return groups.values
            .map { group in
                var sorted = group
                sorted.skills.sort {
                    $0.manifest.name.localizedCaseInsensitiveCompare($1.manifest.name) == .orderedAscending
                }
                return sorted
            }
            .sorted {
                $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
            }
    }
}

private struct SkillWorkspaceView: View {
    let groups: [SkillListGroup]
    @Binding var selectedSkillID: String?
    @ObservedObject var viewModel: SkillManagerViewModel
    let onPreview: (SkillEntry) -> Void
    @State private var expandedGroupIDs: Set<String> = []

    private var skills: [SkillEntry] {
        groups.flatMap(\.skills)
    }

    private var selectedSkill: SkillEntry? {
        guard let selectedSkillID else { return nil }
        return skills.first(where: { $0.id == selectedSkillID })
    }

    var body: some View {
        HStack(spacing: 0) {
            VStack(spacing: 0) {
                HStack {
                    Text(L10n.shared.skillColumnSkill)
                    Spacer()
                    Text(L10n.shared.skillColumnAgents)
                }
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)
                .textCase(.uppercase)
                .padding(.horizontal, 16)
                .frame(height: 40)
                .background(Color(nsColor: .controlBackgroundColor).opacity(0.55))

                Divider()

                ScrollView(.vertical, showsIndicators: false) {
                    LazyVStack(spacing: 0) {
                        ForEach(Array(groups.enumerated()), id: \.element.id) { index, group in
                            if group.isContainer {
                                SkillCompactGroupRow(
                                    group: group,
                                    isExpanded: expandedGroupIDs.contains(group.id),
                                    viewModel: viewModel
                                ) {
                                    withAnimation(.easeInOut(duration: 0.18)) {
                                        if expandedGroupIDs.contains(group.id) {
                                            expandedGroupIDs.remove(group.id)
                                        } else {
                                            expandedGroupIDs.insert(group.id)
                                        }
                                    }
                                }

                                if expandedGroupIDs.contains(group.id) {
                                    ForEach(group.skills) { skill in
                                        SkillCompactRow(
                                            skill: skill,
                                            isSelected: selectedSkill?.id == skill.id,
                                            isChild: true,
                                            viewModel: viewModel
                                        ) {
                                            selectedSkillID = skill.id
                                        }
                                        .transition(.opacity.combined(with: .move(edge: .top)))
                                    }
                                }
                            } else if let skill = group.skills.first {
                                SkillCompactRow(
                                    skill: skill,
                                    isSelected: selectedSkill?.id == skill.id,
                                    viewModel: viewModel
                                ) {
                                    selectedSkillID = skill.id
                                }
                            }

                            if index < groups.count - 1 {
                                Divider().padding(.leading, 62)
                            }
                        }
                    }
                }
            }
            .frame(minWidth: 430, maxWidth: .infinity)

            Divider()

            Group {
                if let selectedSkill {
                    SkillDetailPanel(
                        skill: selectedSkill,
                        viewModel: viewModel,
                        onPreview: { onPreview(selectedSkill) }
                    )
                    .id(selectedSkill.id)
                } else {
                    VStack(spacing: 10) {
                        Image(systemName: "sidebar.right")
                            .font(.system(size: 24))
                            .foregroundStyle(.tertiary)
                        Text(L10n.shared.skillSelectForDetails)
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .frame(minWidth: 320, idealWidth: 360, maxWidth: 410, maxHeight: .infinity)
            .background(Color(nsColor: .controlBackgroundColor).opacity(0.28))
        }
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(Color(nsColor: .separatorColor).opacity(0.65), lineWidth: 1)
        }
    }
}

private struct SkillCompactRow: View {
    let skill: SkillEntry
    let isSelected: Bool
    var isChild = false
    @ObservedObject var viewModel: SkillManagerViewModel
    let onSelect: () -> Void

    private var activeAgents: [AgentConfig] {
        let ids = viewModel.skillAgentIDs(for: skill)
        return viewModel.visibleAgents.filter { ids.contains($0.source) }
    }

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "puzzlepiece.extension.fill")
                .font(.system(size: 15, weight: .medium))
                .foregroundStyle(TVColor.brand)
                .frame(width: 36, height: 36)
                .background(TVColor.brand.opacity(0.1), in: RoundedRectangle(cornerRadius: 9))

            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 6) {
                    Text(skill.manifest.name)
                        .font(.system(size: 14, weight: .semibold))
                        .lineLimit(1)

                    if viewModel.isInSourceRoot(skill) {
                        compactBadge(L10n.shared.skillGlobalBadge, color: TVColor.brand)
                    } else if let source = viewModel.sourceAgent(for: skill) {
                        compactBadge(AgentRegistry.shared.displayName(for: source), color: AgentRegistry.shared.brandColor(for: source))
                    }

                    if viewModel.isBuiltInSkill(skill) {
                        compactBadge(L10n.shared.skillBuiltIn, color: .orange)
                    }
                }

                Text(skill.manifest.description)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            HStack(spacing: -6) {
                ForEach(Array(activeAgents.prefix(3))) { agent in
                    AgentIcon(source: agent.source, size: 18)
                        .frame(width: 24, height: 24)
                        .background(Color(nsColor: .controlBackgroundColor), in: Circle())
                        .overlay(Circle().stroke(Color(nsColor: .windowBackgroundColor), lineWidth: 2))
                }
                if activeAgents.count > 3 {
                    Text("+\(activeAgents.count - 3)")
                        .font(.system(size: 9, weight: .semibold))
                        .frame(width: 24, height: 24)
                        .background(.quaternary, in: Circle())
                        .overlay(Circle().stroke(Color(nsColor: .windowBackgroundColor), lineWidth: 2))
                }
            }
            .frame(minWidth: 58, alignment: .trailing)

            Image(systemName: "chevron.right")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(.tertiary)
        }
        .padding(.leading, isChild ? 38 : 14)
        .padding(.trailing, 14)
        .padding(.vertical, 12)
        .background(isSelected ? TVColor.brand.opacity(0.09) : Color.clear)
        .overlay(alignment: .leading) {
            if isSelected {
                RoundedRectangle(cornerRadius: 2)
                    .fill(TVColor.brand)
                    .frame(width: 3)
                    .padding(.vertical, 8)
            }
        }
        .overlay(alignment: .leading) {
            if isChild {
                Rectangle()
                    .fill(TVColor.brand.opacity(0.2))
                    .frame(width: 2)
                    .padding(.leading, 22)
                    .padding(.vertical, 5)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture(perform: onSelect)
    }

    private func compactBadge(_ text: String, color: Color) -> some View {
        Text(text)
            .font(.system(size: 9, weight: .semibold))
            .foregroundStyle(color)
            .padding(.horizontal, 5)
            .padding(.vertical, 2)
            .background(color.opacity(0.1), in: Capsule())
    }
}

private struct SkillCompactGroupRow: View {
    let group: SkillListGroup
    let isExpanded: Bool
    @ObservedObject var viewModel: SkillManagerViewModel
    let onToggle: () -> Void

    private var activeAgents: [AgentConfig] {
        let ids = group.skills.reduce(into: Set<String>()) { result, skill in
            result.formUnion(viewModel.skillAgentIDs(for: skill))
        }
        return viewModel.visibleAgents.filter { ids.contains($0.source) }
    }

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "folder.fill")
                .font(.system(size: 15, weight: .medium))
                .foregroundStyle(TVColor.brand)
                .frame(width: 36, height: 36)
                .background(TVColor.brand.opacity(0.1), in: RoundedRectangle(cornerRadius: 9))

            VStack(alignment: .leading, spacing: 4) {
                Text(group.title)
                    .font(.system(size: 14, weight: .semibold))
                    .lineLimit(1)
                Text(L10n.shared.skillChildCount(group.skills.count))
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            HStack(spacing: -6) {
                ForEach(Array(activeAgents.prefix(3))) { agent in
                    AgentIcon(source: agent.source, size: 18)
                        .frame(width: 24, height: 24)
                        .background(Color(nsColor: .controlBackgroundColor), in: Circle())
                        .overlay(Circle().stroke(Color(nsColor: .windowBackgroundColor), lineWidth: 2))
                }
                if activeAgents.count > 3 {
                    Text("+\(activeAgents.count - 3)")
                        .font(.system(size: 9, weight: .semibold))
                        .frame(width: 24, height: 24)
                        .background(.quaternary, in: Circle())
                        .overlay(Circle().stroke(Color(nsColor: .windowBackgroundColor), lineWidth: 2))
                }
            }
            .frame(minWidth: 58, alignment: .trailing)

            Image(systemName: "chevron.right")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(.secondary)
                .rotationEffect(.degrees(isExpanded ? 90 : 0))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .contentShape(Rectangle())
        .onTapGesture(perform: onToggle)
    }
}

private struct SkillDetailPanel: View {
    let skill: SkillEntry
    @ObservedObject var viewModel: SkillManagerViewModel
    let onPreview: () -> Void
    @ObservedObject private var l10n = L10n.shared
    @State private var showDeleteConfirm = false

    private var activeAgentIDs: Set<String> {
        viewModel.skillAgentIDs(for: skill)
    }

    private var sourceAgent: String? {
        viewModel.sourceAgent(for: skill)
    }

    var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(alignment: .leading, spacing: 0) {
                Text(l10n.skillDetails)
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(TVColor.brand)
                    .textCase(.uppercase)

                Text(skill.manifest.name)
                    .font(.system(size: 21, weight: .semibold))
                    .lineLimit(2)
                    .padding(.top, 7)

                Text(skill.manifest.description)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .lineLimit(4)
                    .padding(.top, 5)

                VStack(spacing: 10) {
                    detailLine(l10n.skillLocation, value: abbreviatedPath(skill.sourceDir))
                    detailLine(l10n.skillStatus, value: l10n.skillReady, valueColor: TVColor.brand)
                    if let sourceAgent {
                        detailLine(l10n.skillSourceAgent, value: AgentRegistry.shared.displayName(for: sourceAgent))
                    }
                }
                .padding(.vertical, 16)

                Divider()

                HStack {
                    Text(l10n.skillAgentAssignments)
                        .font(.system(size: 13, weight: .semibold))
                    Spacer()
                    Text(l10n.skillAgentAssignmentsHint)
                        .font(.system(size: 10))
                        .foregroundStyle(.tertiary)
                }
                .padding(.top, 17)
                .padding(.bottom, 9)

                if viewModel.visibleAgents.isEmpty {
                    Text(l10n.skillNoAgentsEnabled)
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                } else {
                    VStack(spacing: 7) {
                        ForEach(viewModel.visibleAgents) { agent in
                            agentAssignment(agent)
                        }
                    }
                }

                Divider().padding(.vertical, 18)

                Button(action: onPreview) {
                    Label(l10n.skillPreview, systemImage: "doc.text.magnifyingglass")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.bordered)
                .controlSize(.regular)

                HStack(spacing: 8) {
                    if !viewModel.isInSourceRoot(skill), let sourceAgent {
                        Button {
                            organize(from: sourceAgent)
                        } label: {
                            Label(l10n.skillOrganize, systemImage: "arrow.triangle.swap")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.borderedProminent)
                    } else if viewModel.isInSourceRoot(skill), let sourceAgent {
                        Button {
                            viewModel.restore(skill: skill, agentID: sourceAgent)
                        } label: {
                            Label(l10n.skillRestore, systemImage: "arrow.uturn.backward")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.borderedProminent)
                    }

                    Button(role: .destructive) {
                        showDeleteConfirm = true
                    } label: {
                        Image(systemName: "trash")
                            .frame(width: 24)
                    }
                    .buttonStyle(.bordered)
                }
                .controlSize(.regular)
                .padding(.top, 9)
            }
            .padding(20)
        }
        .alert(l10n.skillDelete, isPresented: $showDeleteConfirm) {
            Button(l10n.cancel, role: .cancel) {}
            Button(l10n.skillDelete, role: .destructive) {
                viewModel.delete(skill: skill)
            }
        } message: {
            Text(l10n.skillDeleteConfirm)
        }
    }

    private func detailLine(_ label: String, value: String, valueColor: Color = .primary) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text(label)
                .foregroundStyle(.secondary)
                .frame(width: 48, alignment: .leading)
            Text(value)
                .foregroundStyle(valueColor)
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)
            Spacer(minLength: 0)
        }
        .font(.system(size: 11))
    }

    private func agentAssignment(_ agent: AgentConfig) -> some View {
        let isActive = activeAgentIDs.contains(agent.source)
        let isLinked = viewModel.isSkillLinked(skillID: skill.id, agentID: agent.source)
        let isPhysicalSource = sourceAgent == agent.source && !isLinked

        return Button {
            if isLinked {
                viewModel.unlinkSkill(skillID: skill.id, agentID: agent.source)
            } else if viewModel.requiresCompatibilityConfirmation(skillID: skill.id, agentID: agent.source) {
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
            HStack(spacing: 9) {
                AgentIcon(source: agent.source, size: 17)
                Text(agent.displayName)
                    .font(.system(size: 12, weight: .medium))
                Spacer()
                if isPhysicalSource {
                    Text(l10n.skillSourceAgent)
                        .font(.system(size: 10))
                        .foregroundStyle(.secondary)
                }
                Image(systemName: isActive ? "checkmark.circle.fill" : "circle")
                    .foregroundStyle(isActive ? TVColor.brand : Color.secondary)
            }
            .padding(.horizontal, 10)
            .frame(minHeight: 36)
            .background(
                isActive ? TVColor.brand.opacity(0.08) : Color(nsColor: .controlBackgroundColor),
                in: RoundedRectangle(cornerRadius: 8)
            )
        }
        .buttonStyle(.plain)
        .quickHelp(isLinked ? l10n.skillUnlinkTip(agent.displayName) : l10n.skillLinkTip(agent.displayName))
    }

    private func organize(from sourceAgent: String) {
        if viewModel.isBuiltInSkill(skill) {
            viewModel.builtInOrganizeAlert = BuiltInOrganizeAlert(
                skillID: skill.id,
                agentID: sourceAgent,
                skillName: skill.manifest.name,
                agentName: AgentRegistry.shared.displayName(for: sourceAgent)
            )
        } else {
            viewModel.organize(skill: skill, agentID: sourceAgent)
        }
    }

    private func abbreviatedPath(_ path: String) -> String {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        guard path.hasPrefix(home) else { return path }
        return "~" + path.dropFirst(home.count)
    }
}

private struct SkillListGroup: Identifiable {
    let id: String
    let linkID: String
    let title: String
    var skills: [SkillEntry]
    let isContainer: Bool
}

private struct SkillDirectoryGroupView: View {
    let group: SkillListGroup
    @ObservedObject var viewModel: SkillManagerViewModel
    let horizontalPadding: CGFloat
    let onPreview: (SkillEntry) -> Void
    @State private var isExpanded = false

    var body: some View {
        VStack(spacing: 0) {
            if let representative = group.skills.first {
                SkillRowView(
                    skill: representative,
                    viewModel: viewModel,
                    group: group,
                    isGroupExpanded: isExpanded
                ) {
                    withAnimation(.easeInOut(duration: 0.18)) {
                        isExpanded.toggle()
                    }
                }
                .padding(.vertical, 2)
                .padding(.horizontal, horizontalPadding)
            }

            if isExpanded {
                VStack(spacing: 0) {
                    ForEach(Array(group.skills.enumerated()), id: \.element.id) { index, skill in
                        SkillRowView(skill: skill, viewModel: viewModel, showsOperations: false) {
                            onPreview(skill)
                        }
                        .padding(.vertical, 2)
                        .padding(.leading, horizontalPadding + 22)
                        .padding(.trailing, horizontalPadding)
                        .overlay(alignment: .leading) {
                            Rectangle()
                                .fill(Color.accentColor.opacity(0.18))
                                .frame(width: 2)
                                .padding(.leading, horizontalPadding + 8)
                        }
                        .transition(.skillListRow)

                        if index < group.skills.count - 1 {
                            Divider()
                                .padding(.leading, horizontalPadding + 22)
                                .padding(.trailing, horizontalPadding)
                        }
                    }
                }
            }
        }
    }
}

private enum SkillListMetrics {
    static let columnSpacing: CGFloat = 12
    static let actionColumnWidth: CGFloat = 104
    static let agentsColumnWidth: CGFloat = 300
    static let columnInset: CGFloat = 14
}

private extension AnyTransition {
    static var skillListRow: AnyTransition {
        .asymmetric(
            insertion: .opacity.combined(with: .move(edge: .top)),
            removal: .opacity.combined(with: .scale(scale: 0.98, anchor: .top))
        )
    }
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
            HStack(spacing: 4) {
                Text(l10n.skillShowBuiltIn)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                Toggle("", isOn: $viewModel.showBuiltInSkills)
                    .toggleStyle(.switch)
                    .controlSize(.mini)
                    .labelsHidden()
                    .quickHelp(l10n.skillShowBuiltIn)
            }
        }
    }

    private var columnDivider: some View {
        Rectangle()
            .fill(Color(nsColor: .separatorColor).opacity(0.85))
            .frame(width: 1, height: 18)
    }
}

// MARK: - Skill Row

private struct SkillRowView: View {
    let skill: SkillEntry
    @ObservedObject var viewModel: SkillManagerViewModel
    var showsOperations = true
    var group: SkillListGroup? = nil
    var isGroupExpanded = true
    let onPreview: () -> Void
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        HStack(alignment: .top, spacing: SkillListMetrics.columnSpacing) {
            skillInfo
                .frame(maxWidth: .infinity, alignment: .leading)
                .layoutPriority(1)

            if showsOperations {
                actionButtons
                    .padding(.leading, SkillListMetrics.columnInset)
                    .frame(width: SkillListMetrics.actionColumnWidth, alignment: .leading)
                    .overlay(alignment: .leading) { columnDivider }
                    .overlay(alignment: .trailing) { columnDivider }

                agentLinkTags
                    .padding(.leading, SkillListMetrics.columnInset)
                    .frame(width: SkillListMetrics.agentsColumnWidth, alignment: .leading)
            }
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
        Group {
            if let group {
                groupInfo(group)
            } else {
                individualSkillInfo
            }
        }
    }

    private func groupInfo(_ group: SkillListGroup) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "folder.fill")
                .font(.system(size: 13))
                .foregroundStyle(Color.accentColor)
            Text(group.title)
                .fontWeight(.semibold)
                .lineLimit(1)
            Text(l10n.skillChildCount(group.skills.count))
                .font(.caption2)
                .foregroundStyle(.secondary)
            Image(systemName: "chevron.right")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(.secondary)
                .rotationEffect(.degrees(isGroupExpanded ? 90 : 0))
            Spacer()
        }
        .frame(maxHeight: .infinity, alignment: .center)
        .contentShape(Rectangle())
        .onTapGesture(perform: onPreview)
    }

    private var individualSkillInfo: some View {
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
                let tint = AgentRegistry.shared.brandColor(for: sourceAgent)
                Text(AgentRegistry.shared.displayName(for: sourceAgent))
                    .font(.caption2)
                    .padding(.horizontal, 5).padding(.vertical, 1)
                    .background(tint.opacity(0.12), in: Capsule())
                    .foregroundStyle(tint)
            }
        }
        // Skills from an agent-owned system container get a "Built-in" marker.
        if viewModel.isBuiltInSkill(skill) {
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
            if !operationIsInSourceRoot, let sourceAgent = operationSourceAgent {
                let displayName = AgentRegistry.shared.displayName(for: sourceAgent)
                Button {
                    if operationIsBuiltIn {
                        viewModel.builtInOrganizeAlert = BuiltInOrganizeAlert(
                            skillID: operationSkillID,
                            agentID: sourceAgent,
                            skillName: operationSkillName,
                            agentName: displayName
                        )
                    } else {
                        viewModel.organizeSkill(skillID: operationSkillID, agentID: sourceAgent)
                    }
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
            } else if operationIsInSourceRoot, let sourceAgent = operationSourceAgent {
                let displayName = AgentRegistry.shared.displayName(for: sourceAgent)
                Button {
                    viewModel.restoreSkill(skillID: operationSkillID, agentID: sourceAgent)
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
                viewModel.deleteSkill(skillID: operationSkillID)
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
        let agents = viewModel.visibleAgents
        let activeAgentIDs = operationAgentIDs
        let linked = agents.filter { viewModel.isSkillLinked(skillID: operationSkillID, agentID: $0.source) }
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

    private func agentLinkChip(agent: AgentConfig, isLinked: Bool, isSource: Bool) -> some View {
        Button {
            if isLinked {
                viewModel.unlinkSkill(skillID: operationSkillID, agentID: agent.source)
            } else if group == nil && viewModel.requiresCompatibilityConfirmation(skillID: operationSkillID, agentID: agent.source) {
                // Cross-agent link: the skill declares specific compatible agents
                // and this one isn't among them. Surface a confirmation alert.
                viewModel.compatibilityAlert = CompatibilityAlert(
                    skillID: operationSkillID,
                    agentID: agent.source,
                    skillName: operationSkillName,
                    agentName: agent.displayName
                )
            } else {
                viewModel.linkSkill(skillID: operationSkillID, agentID: agent.source)
            }
        } label: {
            let tint = AgentRegistry.shared.brandColor(for: agent.source)
            HStack(spacing: 3) {
                AgentIcon(source: agent.source, size: 12)
                Text(agent.displayName)
                    .font(.caption2)
            }
            .padding(.horizontal, 6).padding(.vertical, 2)
            .background(linkBackground(tint: tint, isLinked: isLinked, isSource: isSource))
            .foregroundStyle(linkForeground(tint: tint, isLinked: isLinked, isSource: isSource))
            .clipShape(Capsule())
            .overlay(
                Capsule().strokeBorder(
                    (isLinked || isSource ? tint : Color.gray).opacity(isLinked || isSource ? 0.22 : 0.08),
                    lineWidth: 0.5
                )
            )
        }
        .buttonStyle(.plain)
        .quickHelp(linkTooltip(isLinked: isLinked, isSource: isSource, agent: agent))
    }

    private func linkBackground(tint: Color, isLinked: Bool, isSource: Bool) -> Color {
        if isLinked { return tint.opacity(0.18) }
        if isSource { return tint.opacity(0.14) }
        return Color.gray.opacity(0.1)
    }

    private func linkForeground(tint: Color, isLinked: Bool, isSource: Bool) -> Color {
        if isLinked || isSource { return tint }
        return .secondary
    }

    private func linkTooltip(isLinked: Bool, isSource: Bool, agent: AgentConfig) -> String {
        if isLinked { return l10n.skillUnlinkTip(agent.displayName) }
        if isSource { return l10n.skillSourceLinkTip(agent.displayName) }
        return l10n.skillLinkTip(agent.displayName)
    }

    private var operationSkillID: String { group?.linkID ?? skill.id }

    private var operationSkillName: String { group?.title ?? skill.manifest.name }

    private var operationIsInSourceRoot: Bool {
        group?.skills.allSatisfy(viewModel.isInSourceRoot) ?? viewModel.isInSourceRoot(skill)
    }

    private var operationIsBuiltIn: Bool {
        group?.skills.contains(where: viewModel.isBuiltInSkill) ?? viewModel.isBuiltInSkill(skill)
    }

    private var operationSourceAgent: String? {
        if let group {
            return group.skills.lazy.compactMap(viewModel.sourceAgent).first
        }
        return viewModel.sourceAgent(for: skill)
    }

    private var operationAgentIDs: Set<String> {
        if let group {
            return group.skills.reduce(into: Set<String>()) { result, child in
                result.formUnion(viewModel.skillAgentIDs(for: child))
            }
        }
        return viewModel.skillAgentIDs(for: skill)
    }
}

private struct SkillMarkdownPreviewSheet: View {
    let preview: SkillMarkdownPreview
    @ObservedObject private var l10n = L10n.shared
    @Environment(\.dismiss) private var dismiss
    @State private var fileTree: SkillFileNode?
    @State private var selectedFilePath: String = ""
    @State private var selectedFileContent: String = ""
    @State private var isLoadingTree = true
    @State private var isLoadingContent = true
    @State private var contentError: String?
    @State private var isContentTruncated = false
    @State private var showEnvironmentSheet = false
    @State private var initialLoadTask: Task<Void, Never>?
    @State private var fileLoadTask: Task<Void, Never>?

    var body: some View {
        VStack(spacing: 0) {
            header
                .padding(.horizontal, 20)
                .padding(.vertical, 14)

            Divider()

            HStack(spacing: 0) {
                fileSidebar

                Divider()

                fileContent
            }
        }
        .frame(minWidth: 820, idealWidth: 900, minHeight: 520, idealHeight: 620)
        .sheet(isPresented: $showEnvironmentSheet) {
            SkillEnvironmentConfigurationSheet(skill: preview.skill)
        }
        .onAppear { startInitialLoad() }
        .onDisappear {
            initialLoadTask?.cancel()
            fileLoadTask?.cancel()
        }
    }

    private var fileSidebar: some View {
        ScrollView {
            if isLoadingTree {
                ProgressView()
                    .controlSize(.small)
                    .frame(maxWidth: .infinity)
                    .padding(.top, 20)
            } else if let fileTree {
                SkillFileTreeView(
                    node: fileTree,
                    isRoot: true,
                    selectedPath: $selectedFilePath,
                    onSelect: selectFile(path:)
                )
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(10)
            } else {
                Text(l10n.skillFilesEmpty)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(12)
            }
        }
        .frame(width: 240)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.55))
    }

    private var fileContent: some View {
        VStack(spacing: 0) {
            if isContentTruncated {
                HStack(spacing: 6) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                    Text(l10n.skillPreviewTruncated(SkillPreviewCache.maximumTextBytes))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Spacer()
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 7)
                .background(Color.orange.opacity(0.08))
                Divider()
            }

            if isLoadingContent {
                VStack(spacing: 8) {
                    ProgressView()
                    Text(l10n.loading)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let contentError {
                Text(contentError)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                    .padding(18)
            } else {
                SkillPlainTextView(content: selectedFileContent)
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
    }

    private func startInitialLoad() {
        initialLoadTask?.cancel()
        isLoadingTree = true
        isLoadingContent = true
        initialLoadTask = Task {
            let prepared = await SkillPreviewCache.shared.preparedPreview(for: preview)
            guard !Task.isCancelled else { return }
            fileTree = prepared.fileTree
            isLoadingTree = false
            selectedFilePath = prepared.primaryFilePath
            apply(prepared.primaryContent)
        }
    }

    private func selectFile(path: String) {
        let normalizedPath = standardizedPath(path)
        selectedFilePath = normalizedPath
        fileLoadTask?.cancel()
        isLoadingContent = true
        contentError = nil
        isContentTruncated = false
        fileLoadTask = Task {
            let result = await SkillPreviewCache.shared.loadFile(at: normalizedPath)
            guard !Task.isCancelled, selectedFilePath == normalizedPath else { return }
            apply(result)
        }
    }

    private func apply(_ result: SkillFileLoadResult) {
        isLoadingContent = false
        switch result {
        case .loaded(let content):
            selectedFileContent = content.text
            contentError = nil
            isContentTruncated = content.isTruncated
        case .missing:
            selectedFileContent = ""
            contentError = l10n.skillPreviewMissingFile
            isContentTruncated = false
        case .notText:
            selectedFileContent = ""
            contentError = l10n.skillPreviewNotText
            isContentTruncated = false
        case .unreadable(let message):
            selectedFileContent = ""
            contentError = l10n.skillPreviewReadFailed(message)
            isContentTruncated = false
        }
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
                Text(selectedFilePath.isEmpty ? preview.filePath : selectedFilePath)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
            }

            Spacer()

            if !preview.skill.manifest.environmentVariables.isEmpty {
                Button {
                    showEnvironmentSheet = true
                } label: {
                    Label(l10n.skillEnvironmentTitle, systemImage: "gearshape.fill")
                }
                .quickHelp(l10n.skillEnvironmentManageTip)
            }

            Button(l10n.openInFinder) {
                openInFinder()
            }
            .quickHelp(l10n.openInFinder)

            Button(l10n.gitDone) {
                dismiss()
            }
            .keyboardShortcut(.cancelAction)
            .quickHelp(l10n.gitDoneTip)
        }
    }

    private func openInFinder() {
        let filePath = standardizedPath(selectedFilePath.isEmpty ? preview.filePath : selectedFilePath)
        let fileURL = URL(fileURLWithPath: filePath)
        if FileManager.default.fileExists(atPath: filePath) {
            NSWorkspace.shared.activateFileViewerSelecting([fileURL])
            return
        }

        let skillDir = standardizedPath(preview.skill.sourceDir)
        NSWorkspace.shared.open(URL(fileURLWithPath: skillDir))
    }

    private func standardizedPath(_ path: String) -> String {
        (NSString(string: path).expandingTildeInPath as NSString).standardizingPath
    }
}

private struct SkillEnvironmentConfigurationSheet: View {
    let skill: SkillEntry
    @ObservedObject private var l10n = L10n.shared
    @Environment(\.dismiss) private var dismiss
    @State private var saveTrigger = 0

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Image(systemName: "gearshape.fill")
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(Color.accentColor)
                VStack(alignment: .leading, spacing: 2) {
                    Text(skill.manifest.name)
                        .font(.headline)
                    Text(l10n.skillEnvironmentTitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button(l10n.save) {
                    saveTrigger += 1
                }
                .buttonStyle(.borderedProminent)
                Button(l10n.gitDone) {
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)
            }
            .padding(18)

            Divider()

            ScrollView {
                SkillEnvironmentEditor(
                    variables: skill.manifest.environmentVariables,
                    showsHeader: false,
                    saveTrigger: saveTrigger
                )
                .padding(18)
            }
        }
        .frame(width: 760, height: 440)
    }
}

struct SkillEnvironmentEditor: View {
    let variables: [SkillEnvironmentVariable]
    var relatedSkills: [String: [String]] = [:]
    var title: String? = nil
    var subtitle: String? = nil
    var showsHeader = true
    var saveTrigger = 0
    @ObservedObject private var l10n = L10n.shared
    @State private var values: [String: String] = [:]
    @State private var revealed: Set<String> = []
    @State private var isSaving = false
    @State private var statusMessage: String?
    @State private var statusIsError = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if showsHeader {
                HStack(spacing: 7) {
                    Image(systemName: "gearshape.fill")
                        .foregroundStyle(Color.accentColor)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(title ?? l10n.skillEnvironmentTitle)
                            .font(.system(size: 13, weight: .semibold))
                        if let subtitle, !subtitle.isEmpty {
                            Text(subtitle)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        } else {
                            Text(l10n.skillEnvironmentSecureNote)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                    Spacer()
                    if let statusMessage {
                        Text(statusMessage)
                            .font(.caption2)
                            .foregroundStyle(statusIsError ? .red : .green)
                    }
                    Button(l10n.save) {
                        save()
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    .disabled(isSaving)
                }
            } else if let statusMessage {
                Text(statusMessage)
                    .font(.caption2)
                    .foregroundStyle(statusIsError ? .red : .green)
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }

            ForEach(variables) { variable in
                HStack(alignment: .center, spacing: 10) {
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: 5) {
                            Text(variable.name)
                                .font(.system(size: 11, weight: .medium, design: .monospaced))
                            if variable.required {
                                Text(l10n.skillEnvironmentRequired)
                                    .font(.caption2)
                                    .foregroundStyle(.red)
                            }
                            if variable.inferred {
                                Text(l10n.skillEnvironmentInferred)
                                    .font(.caption2)
                                    .foregroundStyle(.orange)
                            }
                        }
                        if !variable.note.isEmpty {
                            Text(variable.note)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(2)
                        } else if variable.inferred {
                            Text(l10n.skillEnvironmentInferredNote)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                        if let skills = relatedSkills[variable.name], !skills.isEmpty {
                            Text(l10n.skillEnvironmentUsedBy(skills.joined(separator: ", ")))
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                                .lineLimit(2)
                        }
                    }
                    .frame(width: 270, alignment: .leading)

                    environmentField(for: variable)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11))

                    if variable.secret {
                        Button {
                            if revealed.contains(variable.name) {
                                revealed.remove(variable.name)
                            } else {
                                revealed.insert(variable.name)
                            }
                        } label: {
                            Image(systemName: revealed.contains(variable.name) ? "eye.slash" : "eye")
                        }
                        .buttonStyle(.borderless)
                        .quickHelp(l10n.skillEnvironmentReveal)
                    } else {
                        Color.clear.frame(width: 18)
                    }
                }
            }

            Text(l10n.skillEnvironmentActivationNote)
                .font(.caption2)
                .foregroundStyle(.tertiary)
        }
        .onAppear(perform: loadValues)
        .onChange(of: saveTrigger) { _, _ in
            save()
        }
    }

    @ViewBuilder
    private func environmentField(for variable: SkillEnvironmentVariable) -> some View {
        let binding = Binding(
            get: { values[variable.name, default: ""] },
            set: {
                values[variable.name] = $0
                statusMessage = nil
            }
        )
        if variable.secret && !revealed.contains(variable.name) {
            SecureField(l10n.skillEnvironmentValuePlaceholder, text: binding)
        } else {
            TextField(l10n.skillEnvironmentValuePlaceholder, text: binding)
        }
    }

    private func loadValues() {
        values = Dictionary(
            uniqueKeysWithValues: variables.map {
                ($0.name, SkillEnvironmentManager.shared.value(for: $0.name) ?? $0.defaultValue)
            }
        )
    }

    private func save() {
        if variables.contains(where: { $0.required && values[$0.name, default: ""].isEmpty }) {
            statusIsError = true
            statusMessage = l10n.skillEnvironmentRequiredMissing
            return
        }

        let snapshot = values
        let variables = variables
        isSaving = true
        statusMessage = nil
        Task.detached {
            var succeeded = true
            for variable in variables {
                let value = snapshot[variable.name, default: ""]
                do {
                    if value.isEmpty {
                        try SkillEnvironmentManager.shared.remove(variable.name)
                    } else {
                        try SkillEnvironmentManager.shared.save(value, for: variable.name)
                    }
                } catch {
                    succeeded = false
                }
            }
            await MainActor.run {
                isSaving = false
                statusIsError = !succeeded
                statusMessage = succeeded
                    ? l10n.skillEnvironmentSaved
                    : l10n.skillEnvironmentSaveFailed
            }
        }
    }
}

private struct SkillPlainTextView: NSViewRepresentable {
    let content: String

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> NSScrollView {
        let scrollView = NSScrollView()
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = false
        scrollView.autohidesScrollers = true
        scrollView.borderType = .noBorder
        scrollView.drawsBackground = false

        let textView = NSTextView(frame: scrollView.contentView.bounds)
        textView.isEditable = false
        textView.isSelectable = true
        textView.isRichText = false
        textView.importsGraphics = false
        textView.usesFindBar = true
        textView.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        textView.textColor = .textColor
        textView.backgroundColor = .textBackgroundColor
        textView.textContainerInset = NSSize(width: 18, height: 18)
        textView.minSize = NSSize(width: 0, height: 0)
        textView.maxSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude
        )
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = true
        textView.textContainer?.containerSize = NSSize(
            width: 0,
            height: CGFloat.greatestFiniteMagnitude
        )
        scrollView.documentView = textView
        context.coordinator.textView = textView
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard context.coordinator.content != content,
              let textView = context.coordinator.textView else { return }
        context.coordinator.content = content
        textView.string = content
        textView.scrollToBeginningOfDocument(nil)
    }

    final class Coordinator {
        weak var textView: NSTextView?
        var content = ""
    }
}

private struct SkillFileTreeView: View {
    let node: SkillFileNode
    let isRoot: Bool
    let level: Int
    @Binding var selectedPath: String
    let onSelect: (String) -> Void
    @State private var isExpanded: Bool

    init(
        node: SkillFileNode,
        isRoot: Bool = false,
        level: Int = 0,
        selectedPath: Binding<String>,
        onSelect: @escaping (String) -> Void
    ) {
        self.node = node
        self.isRoot = isRoot
        self.level = level
        self._selectedPath = selectedPath
        self.onSelect = onSelect
        _isExpanded = State(initialValue: isRoot || level == 0)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            if !isRoot {
                if node.isDirectory {
                    Button {
                        isExpanded.toggle()
                    } label: {
                        row(icon: "folder", tint: .blue, showsChevron: true, isSelected: false)
                    }
                    .buttonStyle(.plain)
                } else {
                    Button {
                        onSelect(node.path)
                    } label: {
                        row(icon: "doc.text", tint: .secondary, showsChevron: false, isSelected: selectedPath == node.path)
                    }
                    .buttonStyle(.plain)
                }
            }

            if node.isDirectory && isExpanded {
                ForEach(node.children) { child in
                    SkillFileTreeView(
                        node: child,
                        level: isRoot ? 0 : level + 1,
                        selectedPath: $selectedPath,
                        onSelect: onSelect
                    )
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func row(icon: String, tint: Color, showsChevron: Bool, isSelected: Bool) -> some View {
        HStack(spacing: 6) {
            if showsChevron {
                Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(.tertiary)
                    .frame(width: 12)
            } else {
                Color.clear.frame(width: 12)
            }

            Image(systemName: icon)
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(tint)
                .frame(width: 16)

            Text(node.name)
                .font(.system(size: 12))
                .lineLimit(1)
                .truncationMode(.middle)

            if let size = node.sizeBytes, !node.isDirectory {
                Text(ByteCountFormatter.string(fromByteCount: size, countStyle: .file))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }

            Spacer(minLength: 0)
        }
        .padding(.leading, CGFloat(level) * 16 + 4)
        .padding(.trailing, 6)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, 4)
        .background(
            RoundedRectangle(cornerRadius: 5, style: .continuous)
                .fill(isSelected ? TVColor.brand.opacity(0.12) : Color.clear)
        )
        .foregroundStyle(isSelected ? TVColor.brand : .primary)
        .textSelection(.enabled)
    }
}
