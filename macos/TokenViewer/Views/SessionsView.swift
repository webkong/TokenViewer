import SwiftUI

struct SessionsView: View {
    @ObservedObject private var viewModel = SessionsViewModel.shared
    @ObservedObject private var l10n = L10n.shared

    @State private var renameTarget: SessionEntry?
    @State private var renameText = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            header
            filters
            sessionList
        }
        .padding(20)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Color(nsColor: .windowBackgroundColor))
        .onAppear { viewModel.start() }
        .alert(l10n.sessionRenameTitle, isPresented: renameBinding) {
            TextField(l10n.sessionRenamePlaceholder, text: $renameText)
            Button(l10n.save) { commitRename() }
            Button(l10n.cancel, role: .cancel) {}
        } message: {
            Text(l10n.sessionRenameHint)
        }
    }

    private var header: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 3) {
                Text(l10n.sessionsTitle)
                    .font(.system(size: 26, weight: .bold))
                Text(l10n.sessionsSubtitle)
                    .font(.system(size: 13))
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button(action: { viewModel.refresh() }) {
                TVSymbol(name: "arrow.triangle.2.circlepath", size: 14)
                    .rotationEffect(.degrees(viewModel.isScanning ? 360 : 0))
                    .animation(
                        viewModel.isScanning
                            ? .linear(duration: 1).repeatForever(autoreverses: false)
                            : .default,
                        value: viewModel.isScanning
                    )
            }
            .tvIconButton()
            .help(l10n.sessionsRefresh)
            .disabled(viewModel.isScanning)
        }
    }

    // MARK: - Filters

    private var filters: some View {
        VStack(alignment: .leading, spacing: 10) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    filterChip(
                        label: l10n.sessionAll,
                        isSelected: viewModel.selectedAgent.isEmpty
                    ) { viewModel.selectAgent("") }

                    ForEach(viewModel.agentSources, id: \.self) { source in
                        filterChip(
                            label: AgentRegistry.shared.displayName(for: source),
                            isSelected: viewModel.selectedAgent == source
                        ) { viewModel.selectAgent(source) }
                    }
                }
            }

            HStack(spacing: 10) {
                HStack(spacing: 3) {
                    ForEach(SessionDateRange.allCases) { range in
                        rangeButton(range)
                    }
                }
                .padding(3)
                .background(
                    RoundedRectangle(cornerRadius: 9)
                        .fill(Color(nsColor: .controlBackgroundColor))
                        .overlay(RoundedRectangle(cornerRadius: 9).strokeBorder(.quaternary, lineWidth: 0.5))
                )

                Menu {
                    Button(l10n.sessionAllProjects) { viewModel.selectedProject = "" }
                    Divider()
                    ForEach(viewModel.projects, id: \.self) { project in
                        Button(project) { viewModel.selectedProject = project }
                    }
                } label: {
                    Label(
                        viewModel.selectedProject.isEmpty ? l10n.sessionAllProjects : viewModel.selectedProject,
                        systemImage: "folder"
                    )
                    .font(.system(size: 12, weight: .medium))
                    .lineLimit(1)
                    .frame(maxWidth: 180, alignment: .leading)
                }
                .menuStyle(.borderlessButton)
                .tvSelect(width: 190)

                Spacer()
            }

            HStack(spacing: 10) {
                HStack(spacing: 8) {
                    TVSymbol(name: "magnifyingglass")
                    TextField(l10n.sessionSearchPlaceholder, text: $viewModel.searchText)
                        .textFieldStyle(.plain)
                        .font(.system(size: 13))
                }
                .padding(.horizontal, 12)
                .frame(width: 360, height: 34)
                .background(
                    RoundedRectangle(cornerRadius: 9)
                        .fill(Color(nsColor: .controlBackgroundColor))
                        .overlay(RoundedRectangle(cornerRadius: 9).strokeBorder(.quaternary, lineWidth: 0.5))
                )

                Spacer()

                Text("\(viewModel.filteredSessions.count) / \(viewModel.totalCount)")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
        }
    }

    private func filterChip(label: String, isSelected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(label)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(isSelected ? Color.white : Color.primary)
                .padding(.horizontal, 13)
                .frame(height: 30)
                .background(
                    Capsule()
                        .fill(isSelected ? TVColor.brand : Color(nsColor: .controlBackgroundColor))
                        .overlay(Capsule().strokeBorder(.quaternary, lineWidth: 0.5))
                )
        }
        .buttonStyle(.plain)
    }

    private func rangeButton(_ range: SessionDateRange) -> some View {
        Button {
            viewModel.selectedRange = range
        } label: {
            Text(rangeLabel(range))
                .font(.system(size: 11, weight: viewModel.selectedRange == range ? .semibold : .regular))
                .foregroundStyle(viewModel.selectedRange == range ? Color.primary : Color.secondary)
                .padding(.horizontal, 11)
                .frame(height: 26)
                .background(
                    RoundedRectangle(cornerRadius: 7)
                        .fill(viewModel.selectedRange == range ? Color(nsColor: .windowBackgroundColor) : .clear)
                        .shadow(
                            color: viewModel.selectedRange == range ? .black.opacity(0.08) : .clear,
                            radius: 2,
                            y: 1
                        )
                )
        }
        .buttonStyle(.plain)
    }

    private func rangeLabel(_ range: SessionDateRange) -> String {
        switch range {
        case .all: l10n.allTime
        case .sevenDays: l10n.sevenDays
        case .thirtyDays: l10n.thirtyDays
        case .ninetyDays: l10n.ninetyDays
        }
    }

    // MARK: - Session list

    private var sessionList: some View {
        Group {
            if viewModel.filteredSessions.isEmpty && !viewModel.isLoading && !viewModel.isScanning {
                emptyState
            } else {
                ScrollView(showsIndicators: true) {
                    LazyVStack(spacing: 8) {
                        ForEach(viewModel.filteredSessions) { session in
                            sessionRow(session)
                        }
                        if viewModel.isLoading {
                            HStack { Spacer(); ProgressView().controlSize(.small); Spacer() }
                                .padding(.vertical, 10)
                        }
                    }
                    .padding(.bottom, 2)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private var emptyState: some View {
        VStack(spacing: 9) {
            Image(systemName: "bubble.left.and.bubble.right")
                .font(.system(size: 30))
                .foregroundStyle(.tertiary)
            Text(l10n.sessionsEmpty)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(.secondary)
            Text(l10n.sessionsEmptyHint)
                .font(.system(size: 11))
                .foregroundStyle(.tertiary)
        }
        .frame(maxWidth: .infinity, minHeight: 220)
    }

    // MARK: - Row

    private func sessionRow(_ session: SessionEntry) -> some View {
        HStack(spacing: 14) {
            AgentIcon(source: session.source, size: 28)

            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 6) {
                    Text(session.displayTitle)
                        .font(.system(size: 14, weight: .semibold))
                        .lineLimit(1)
                    Button { beginRename(session) } label: {
                        TVSymbol(name: "pencil", size: 10)
                    }
                    .buttonStyle(.plain)
                    .help(l10n.sessionRename)
                }

                HStack(spacing: 6) {
                    Text(AgentRegistry.shared.displayName(for: session.source))
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(AgentRegistry.shared.brandColor(for: session.source))
                    metadataSeparator
                    Text(session.project.isEmpty ? session.cwd : session.project)
                    if !session.model.isEmpty {
                        metadataSeparator
                        Text(session.model)
                    }
                    metadataSeparator
                    Text(relativeTime(session))
                    if session.duration_seconds > 0 {
                        metadataSeparator
                        Text(formatDuration(session.duration_seconds))
                    }
                }
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            metricColumn(value: formatTokens(session.total_tokens), label: l10n.sessionTokens)
            metricColumn(value: formatCost(session.total_cost_usd), label: l10n.cost)
            metricColumn(value: "\(session.turn_count)", label: l10n.sessionTurns)
            metricColumn(value: "\(session.edit_count)", label: l10n.sessionEdits)

            launchButton(session)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .frame(minHeight: 68)
        .background(
            RoundedRectangle(cornerRadius: 12)
                .fill(Color(nsColor: .controlBackgroundColor))
                .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(.quaternary, lineWidth: 0.5))
        )
        .contextMenu { renameMenu(session) }
    }

    private var metadataSeparator: some View {
        Text("·").foregroundStyle(.tertiary)
    }

    private func metricColumn(value: String, label: String) -> some View {
        VStack(alignment: .trailing, spacing: 2) {
            Text(value)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.primary)
                .monospacedDigit()
            Text(label)
                .font(.system(size: 9))
                .foregroundStyle(.tertiary)
        }
        .frame(width: 58, alignment: .trailing)
    }

    @ViewBuilder
    private func launchButton(_ session: SessionEntry) -> some View {
        let adapter = SessionCommandRegistry.shared.adapter(for: session.source)
        let registry = AgentRegistry.shared
        let installReady = !registry.hasDetectedInstalls || registry.isInstalled(for: session.source)
        let supported = adapter != nil && installReady
        let yolo = SessionYoloStore.shared.args(for: session.source) != nil

        VStack(alignment: .trailing, spacing: 4) {
            if yolo {
                Label("YOLO", systemImage: "bolt.fill")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(.orange)
            }
            Button { launch(session) } label: {
                Label(l10n.sessionLaunch, systemImage: "play.fill")
                    .frame(minWidth: 84)
            }
            .tvActionButton(.primary)
            .disabled(!supported)
            .help(launchHelp(session, adapter: adapter, installed: installReady))
        }
        .frame(width: 116, alignment: .trailing)
    }

    private func launchHelp(
        _ session: SessionEntry,
        adapter: SessionCommandAdapter?,
        installed: Bool
    ) -> String {
        let name = AgentRegistry.shared.displayName(for: session.source)
        if adapter == nil { return l10n.sessionUnsupportedAgent(name) }
        if !installed { return l10n.sessionAgentNotInstalled(name) }
        return l10n.sessionLaunchHelp
    }

    @ViewBuilder
    private func renameMenu(_ session: SessionEntry) -> some View {
        Button(l10n.sessionRename) { beginRename(session) }
    }

    private func launch(_ session: SessionEntry) {
        do {
            try SessionLaunchService.shared.launch(session)
        } catch {
            ToastCenter.shared.error(error.localizedDescription)
        }
    }

    // MARK: - Rename

    private var renameBinding: Binding<Bool> {
        Binding(get: { renameTarget != nil }, set: { if !$0 { renameTarget = nil } })
    }

    private func beginRename(_ session: SessionEntry) {
        renameText = session.displayTitle
        renameTarget = session
    }

    private func commitRename() {
        guard let target = renameTarget else { return }
        viewModel.rename(target, to: renameText)
        renameTarget = nil
    }

    // MARK: - Formatting

    private func relativeTime(_ session: SessionEntry) -> String {
        guard let date = session.lastActiveDate else { return "" }
        let interval = max(0, Date().timeIntervalSince(date))
        if interval < 60 { return l10n.sessionJustNow }
        if interval < 3_600 { return l10n.sessionMinutesAgo(Int(interval / 60)) }
        if interval < 86_400 { return l10n.sessionHoursAgo(Int(interval / 3_600)) }
        return l10n.sessionDaysAgo(Int(interval / 86_400))
    }

    private func formatTokens(_ value: UInt64) -> String {
        switch value {
        case 1_000_000...: String(format: "%.1fM", Double(value) / 1_000_000)
        case 1_000...: String(format: "%.1fK", Double(value) / 1_000)
        default: "\(value)"
        }
    }

    private func formatCost(_ value: Double) -> String {
        value >= 100 ? String(format: "$%.0f", value) : String(format: "$%.2f", value)
    }

    private func formatDuration(_ seconds: UInt64) -> String {
        if seconds < 60 { return l10n.sessionDurationMinutes(1) }
        let minutes = Int(seconds / 60)
        if minutes < 60 { return l10n.sessionDurationMinutes(minutes) }
        return l10n.sessionDurationHours(minutes / 60, minutes % 60)
    }
}
