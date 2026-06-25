import SwiftUI

struct SkillManagerView: View {
    @StateObject private var viewModel = SkillManagerViewModel.shared
    @State private var showSyncSheet = false

    var body: some View {
        VStack(spacing: 0) {
            // Agent filter chips
            agentFilterBar
            Divider()

            // Skill list
            if viewModel.isLoading {
                Spacer()
                ProgressView()
                Spacer()
            } else if viewModel.filteredSkills.isEmpty {
                emptyState
            } else {
                SkillListView(viewModel: viewModel)
            }
        }
        .onAppear { viewModel.refresh() }
        .sheet(isPresented: $showSyncSheet) {
            SkillGitSyncSheet(viewModel: viewModel)
        }
    }

    // MARK: - Agent Filter Bar

    private var agentFilterBar: some View {
        HStack(spacing: 8) {
            // All agents chip
            FilterChip(
                icon: "square.grid.2x2",
                label: L10n.shared.skillAll,
                isSelected: viewModel.selectedFilter == "all",
                action: { viewModel.selectedFilter = "all" }
            )

            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach(viewModel.visibleProviders) { p in
                        FilterChip(
                            icon: nil,
                            providerIcon: p.source,
                            label: p.displayName,
                            isSelected: viewModel.selectedFilter == p.source,
                            action: { viewModel.selectedFilter = p.source }
                        )
                    }
                }
                .padding(.horizontal, 4)
            }

            Spacer(minLength: 6)

            // Search field
            HStack(spacing: 4) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)
                TextField("Search", text: $viewModel.searchText)
                    .textFieldStyle(.plain)
                    .font(.system(size: 12))
                    .frame(width: 120)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(.quinary, in: RoundedRectangle(cornerRadius: 6))

            // Sync button
            Button {
                viewModel.refreshGitStatus()
                showSyncSheet = true
            } label: {
                Image(systemName: "arrow.triangle.merge")
                    .font(.system(size: 12, weight: .medium))
            }
            .buttonStyle(.borderless)
            .help("Git Sync")

            // Refresh button
            Button { viewModel.refresh() } label: {
                Image(systemName: "arrow.clockwise")
                    .font(.system(size: 12, weight: .medium))
            }
            .buttonStyle(.borderless)
            .help(L10n.shared.skillFetch)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
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
    }
}

// MARK: - Filter Chip

struct FilterChip: View {
    var icon: String? = nil
    var providerIcon: String? = nil
    let label: String
    let isSelected: Bool
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
    }
}
