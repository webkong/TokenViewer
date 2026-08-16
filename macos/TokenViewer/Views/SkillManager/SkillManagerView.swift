import SwiftUI

struct SkillManagerView: View {
    @StateObject private var viewModel = SkillManagerViewModel.shared
    @State private var showSyncSheet = false
    @State private var showInstallSheet = false
    @State private var showEnvironmentSheet = false
    @State private var showOrganizeAllConfirm = false
    @State private var showRestoreAllConfirm = false
    @State private var showOnboarding = false
    @AppStorage("skillsEnabledProviders") private var enabledAgentsJSON: String = AgentRegistry.defaultAgentSourcesJSON
    @AppStorage("skillsOnboardingSeen") private var onboardingSeen = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            header
            agentFilterBar

            Group {
                if viewModel.isLoading && viewModel.skills.isEmpty {
                    Spacer()
                    ProgressView()
                        .frame(maxWidth: .infinity)
                    Spacer()
                } else if viewModel.filteredSkills.isEmpty {
                    emptyState
                } else {
                    SkillListView(viewModel: viewModel)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .padding(20)
        .background(Color(nsColor: .windowBackgroundColor))
        .onAppear {
            // Let the tab selection render first, then refresh directory-backed
            // data asynchronously. Cached skills stay visible during refreshes.
            Task { @MainActor in
                await Task.yield()
                viewModel.refreshIfNeeded()
            }
            if !onboardingSeen {
                onboardingSeen = true
                showOnboarding = true
            }
        }
        .onDisappear { viewModel.resetInstallForm() }
        .onChange(of: enabledAgentsJSON) { _, _ in
            viewModel.ensureValidFilter()
            viewModel.refresh()
        }
        .sheet(isPresented: $showOnboarding) {
            SkillOnboardingSheet()
        }
        .sheet(isPresented: $showSyncSheet) {
            SkillGitSyncSheet(viewModel: viewModel)
        }
        .sheet(isPresented: $showInstallSheet) {
            SkillInstallSheet(viewModel: viewModel)
        }
        .sheet(isPresented: $showEnvironmentSheet) {
            SkillEnvironmentManagerSheet(viewModel: viewModel)
        }
        .alert(L10n.shared.skillOrganizeAllConfirmTitle, isPresented: $showOrganizeAllConfirm) {
            Button(L10n.shared.cancel, role: .cancel) {}
            Button(L10n.shared.skillOrganize) {
                AppFocus.clear()
                viewModel.organizeFilteredSkills()
            }
        } message: {
            Text(L10n.shared.skillOrganizeAllConfirmMessage)
        }
        .alert(L10n.shared.skillRestoreAllConfirmTitle, isPresented: $showRestoreAllConfirm) {
            Button(L10n.shared.cancel, role: .cancel) {}
            Button(L10n.shared.skillRestore) {
                AppFocus.clear()
                viewModel.restoreFilteredSkills()
            }
        } message: {
            Text(L10n.shared.skillRestoreAllConfirmMessage)
        }
    }

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(L10n.shared.skills)
                    .font(.system(size: 24, weight: .bold))
                Text(L10n.shared.skillsSubtitle)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Button {
                showOnboarding = true
            } label: {
                Image(systemName: "questionmark.circle")
                    .font(.system(size: 14))
            }
            .buttonStyle(.borderless)
            .quickHelp(L10n.shared.skillOnboardingShowHelpTip)

            Button {
                showInstallSheet = true
            } label: {
                Label(L10n.shared.skillInstall, systemImage: "plus")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.small)
            .quickHelp(L10n.shared.skillInstallTip)

            Button {
                showEnvironmentSheet = true
            } label: {
                Image(systemName: "gearshape.fill")
                    .font(.system(size: 13, weight: .semibold))
            }
            .buttonStyle(.borderless)
            .quickHelp(L10n.shared.skillEnvironmentManageTip)

            Button { viewModel.refresh(showToast: true) } label: {
                Image(systemName: "arrow.triangle.2.circlepath")
                    .font(.system(size: 13, weight: .semibold))
                    .rotationEffect(.degrees(viewModel.isLoading ? 360 : 0))
                    .animation(viewModel.isLoading ? .linear(duration: 1).repeatForever(autoreverses: false) : .default, value: viewModel.isLoading)
            }
            .buttonStyle(.borderless)
            .disabled(viewModel.isLoading)
            .quickHelp(L10n.shared.skillRefreshTip)
        }
    }

    // MARK: - Agent Filter Bar

    private var agentFilterBar: some View {
        HStack(alignment: .top, spacing: 12) {
            filterChips
                .layoutPriority(1)

            Spacer(minLength: 0)

            filterActions
                .fixedSize()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(.quaternary, lineWidth: 0.5))
    }

    private var filterChips: some View {
        FlowLayout(itemSpacing: 6, rowSpacing: 6) {
            FilterChip(
                icon: "square.grid.2x2",
                label: L10n.shared.skillAll,
                isSelected: viewModel.selectedFilter == SkillManagerViewModel.allFilter,
                tooltip: L10n.shared.skillAllFilterTip,
                action: { viewModel.selectedFilter = SkillManagerViewModel.allFilter }
            )

            FilterChip(
                icon: "globe",
                label: L10n.shared.skillGlobal,
                isSelected: viewModel.selectedFilter == SkillManagerViewModel.globalFilter,
                tooltip: L10n.shared.skillGlobalFilterTip,
                action: { viewModel.selectedFilter = SkillManagerViewModel.globalFilter }
            )

            ForEach(viewModel.visibleAgents) { p in
                FilterChip(
                    icon: nil,
                    agentIcon: p.source,
                    label: p.displayName,
                    isSelected: viewModel.selectedFilter == p.source,
                    tooltip: L10n.shared.skillAgentFilterTip(p.displayName),
                    action: { viewModel.selectedFilter = p.source }
                )
            }
        }
    }

    private var filterActions: some View {
        HStack(spacing: 8) {
            // Search field
            HStack(spacing: 4) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)
                TextField(L10n.shared.skillSearchPlaceholder, text: $viewModel.searchText)
                    .help(L10n.shared.skillSearchPlaceholder)
                    .textFieldStyle(.plain)
                    .font(.system(size: 12))
                    .frame(width: 120)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(.quinary, in: RoundedRectangle(cornerRadius: 6))

            Button {
                AppFocus.clear()
                showOrganizeAllConfirm = true
            } label: {
                Image(systemName: "arrow.triangle.swap")
                    .font(.system(size: 12, weight: .medium))
            }
            .buttonStyle(.borderless)
            .disabled(viewModel.isLoading)
            .quickHelp(L10n.shared.skillOrganizeAllTip)

            Button {
                AppFocus.clear()
                showRestoreAllConfirm = true
            } label: {
                Image(systemName: "arrow.uturn.backward")
                    .font(.system(size: 12, weight: .medium))
            }
            .buttonStyle(.borderless)
            .disabled(viewModel.isLoading)
            .quickHelp(L10n.shared.skillRestoreAllTip)

            // Sync button
            Button {
                viewModel.refreshGitStatus()
                showSyncSheet = true
            } label: {
                Image(systemName: "arrow.triangle.merge")
                    .font(.system(size: 12, weight: .medium))
            }
            .buttonStyle(.borderless)
            .quickHelp(L10n.shared.skillGitSyncTip)
        }
    }

    // MARK: - Empty State

    private var emptyState: some View {
        VStack(spacing: 12) {
            Spacer()
            Image(systemName: "puzzlepiece.extension")
                .font(.system(size: 40))
                .foregroundStyle(.secondary)
            Text(L10n.shared.skillNoSkills)
                .font(.headline)
            Text(L10n.shared.skillNoSkillsDesc)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            Spacer()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct SkillEnvironmentManagerSheet: View {
    @ObservedObject var viewModel: SkillManagerViewModel
    @ObservedObject private var l10n = L10n.shared
    @Environment(\.dismiss) private var dismiss
    @State private var searchText = ""

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Image(systemName: "gearshape.fill")
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(Color.accentColor)
                VStack(alignment: .leading, spacing: 2) {
                    Text(l10n.skillEnvironmentManageTitle)
                        .font(.headline)
                    Text(l10n.skillEnvironmentManageSubtitle)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                HStack(spacing: 5) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                    TextField(l10n.skillEnvironmentSearchPlaceholder, text: $searchText)
                        .textFieldStyle(.plain)
                        .frame(width: 190)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 5)
                .background(.quinary, in: RoundedRectangle(cornerRadius: 6))
                Button(l10n.gitDone) {
                    dismiss()
                }
                .keyboardShortcut(.cancelAction)
            }
            .padding(18)

            Divider()

            if filteredSkillGroups.isEmpty {
                VStack(spacing: 10) {
                    Spacer()
                    Image(systemName: "gearshape")
                        .font(.system(size: 30))
                        .foregroundStyle(.secondary)
                    Text(searchText.isEmpty
                        ? l10n.skillEnvironmentNone
                        : l10n.skillEnvironmentSearchEmpty)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    Spacer()
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVStack(spacing: 12) {
                        ForEach(filteredSkillGroups) { group in
                            SkillEnvironmentEditor(
                                variables: group.variables,
                                title: group.skill.manifest.name,
                                subtitle: group.skill.manifest.description
                            )
                            .padding(14)
                            .background(
                                Color(nsColor: .controlBackgroundColor),
                                in: RoundedRectangle(cornerRadius: 10)
                            )
                            .overlay {
                                RoundedRectangle(cornerRadius: 10)
                                    .stroke(.separator.opacity(0.45), lineWidth: 1)
                            }
                        }
                    }
                    .padding(18)
                }
            }
        }
        .frame(width: 820, height: 560)
    }

    private var filteredSkillGroups: [SkillEnvironmentGroup] {
        viewModel.skills.compactMap { skill in
            let variables = skill.manifest.environmentVariables
            guard !variables.isEmpty else { return nil }
            guard !searchText.isEmpty else {
                return SkillEnvironmentGroup(skill: skill, variables: variables)
            }

            let skillMatches = skill.manifest.name.localizedCaseInsensitiveContains(searchText)
                || skill.manifest.description.localizedCaseInsensitiveContains(searchText)
            let matchingVariables = skillMatches ? variables : variables.filter {
                $0.name.localizedCaseInsensitiveContains(searchText)
                    || $0.note.localizedCaseInsensitiveContains(searchText)
                    || $0.defaultValue.localizedCaseInsensitiveContains(searchText)
            }
            guard !matchingVariables.isEmpty else { return nil }
            return SkillEnvironmentGroup(skill: skill, variables: matchingVariables)
        }
        .sorted {
            $0.skill.manifest.name.localizedCaseInsensitiveCompare(
                $1.skill.manifest.name
            ) == .orderedAscending
        }
    }
}

private struct SkillEnvironmentGroup: Identifiable {
    let skill: SkillEntry
    let variables: [SkillEnvironmentVariable]

    var id: String { skill.id }
}

// MARK: - Filter Chip

struct FilterChip: View {
    var icon: String? = nil
    var agentIcon: String? = nil
    let label: String
    let isSelected: Bool
    let tooltip: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                if let name = agentIcon {
                    AgentIcon(source: name, size: 14)
                } else if let icon {
                    Image(systemName: icon)
                        .font(.system(size: 11, weight: .medium))
                }
                Text(label)
                    .font(.system(size: 11, weight: .medium))
                    .lineLimit(1)
            }
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
            .background(
                Capsule()
                    .fill(isSelected ? Color.accentColor : Color(nsColor: .controlBackgroundColor))
                    .overlay(
                        Capsule()
                            .strokeBorder(isSelected ? Color.accentColor.opacity(0.3) : Color.secondary.opacity(0.15), lineWidth: 0.75)
                    )
            )
            .foregroundStyle(isSelected ? .white : .primary)
        }
        .buttonStyle(.plain)
        .quickHelp(tooltip)
    }
}
