import AppKit
import SwiftUI

struct SkillListView: View {
    @ObservedObject var viewModel: SkillManagerViewModel
    @ObservedObject private var l10n = L10n.shared
    @State private var preview: SkillMarkdownPreview?

    private let horizontalPadding: CGFloat = 30

    var body: some View {
        let skills = filteredSkills
        VStack(spacing: 0) {
            SkillListHeader(viewModel: viewModel)
                .padding(.horizontal, horizontalPadding)

            Divider()
                .padding(.horizontal, horizontalPadding)

            ScrollView(.vertical, showsIndicators: false) {
                LazyVStack(spacing: 0) {
                    ForEach(Array(skills.enumerated()), id: \.element.id) { index, skill in
                        SkillRowView(skill: skill, viewModel: viewModel) {
                            preview = viewModel.skillMarkdownPreview(for: skill)
                        }
                            .padding(.vertical, 2)
                            .padding(.horizontal, horizontalPadding)
                            .transition(.skillListRow)

                        if index < skills.count - 1 {
                            Divider()
                                .padding(.horizontal, horizontalPadding)
                                .transition(.opacity)
                        }
                    }
                }
                .animation(.easeInOut(duration: 0.18), value: skills.map(\.id))
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

    private var filteredSkills: [SkillEntry] {
        let skills = viewModel.filteredSkills
        if viewModel.selectedFilter == SkillManagerViewModel.allFilter { return skills }
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
                let tint = ProviderRegistry.shared.brandColor(for: sourceAgent)
                Text(ProviderRegistry.shared.displayName(for: sourceAgent))
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
            if !viewModel.isInSourceRoot(skill), let sourceAgent = viewModel.sourceAgent(for: skill) {
                let displayName = ProviderRegistry.shared.displayName(for: sourceAgent)
                Button {
                    if viewModel.isBuiltInSkill(skill) {
                        viewModel.builtInOrganizeAlert = BuiltInOrganizeAlert(
                            skillID: skill.id,
                            agentID: sourceAgent,
                            skillName: skill.manifest.name,
                            agentName: displayName
                        )
                    } else {
                        viewModel.organize(skill: skill, agentID: sourceAgent)
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
            let tint = ProviderRegistry.shared.brandColor(for: agent.source)
            HStack(spacing: 3) {
                ProviderIcon(source: agent.source, size: 12)
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
    @State private var fileTree: SkillFileNode?
    @State private var selectedFilePath: String = ""
    @State private var selectedFileContent: String = ""
    @State private var isLoadingTree = true
    @State private var isLoadingContent = true
    @State private var contentError: String?
    @State private var isContentTruncated = false
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
