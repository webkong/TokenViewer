import SwiftUI

struct SkillManagerView: View {
    @StateObject private var viewModel = SkillManagerViewModel.shared
    @State private var showSyncSheet = false
    @State private var showInstallSheet = false
    @State private var showOrganizeAllConfirm = false
    @State private var showRestoreAllConfirm = false
    @AppStorage("skillsEnabledProviders") private var enabledProvidersJSON: String = ProviderRegistry.defaultSkillSourcesJSON

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            header
            agentFilterBar

            Group {
                if viewModel.isLoading {
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
        .onAppear { viewModel.refresh() }
        .onDisappear { viewModel.resetInstallForm() }
        .onChange(of: enabledProvidersJSON) { _, _ in
            viewModel.ensureValidFilter()
            viewModel.refresh()
        }
        .sheet(isPresented: $showSyncSheet) {
            SkillGitSyncSheet(viewModel: viewModel)
        }
        .sheet(isPresented: $showInstallSheet) {
            SkillInstallSheet(viewModel: viewModel)
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
                showInstallSheet = true
            } label: {
                Label(L10n.shared.skillInstall, systemImage: "plus")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.small)
            .quickHelp(L10n.shared.skillInstallTip)

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

            ForEach(viewModel.visibleProviders) { p in
                FilterChip(
                    icon: nil,
                    providerIcon: p.source,
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

// MARK: - Filter Chip

struct FilterChip: View {
    var icon: String? = nil
    var providerIcon: String? = nil
    let label: String
    let isSelected: Bool
    let tooltip: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                if let name = providerIcon {
                    ProviderIcon(source: name, size: 14)
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
