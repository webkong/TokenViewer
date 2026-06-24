import SwiftUI

struct SkillManagerView: View {
    @StateObject private var viewModel = SkillManagerViewModel.shared
    @State private var selectedPanel = "skills"

    var body: some View {
        VStack(spacing: 0) {
            panelPicker
            Divider()
            switch selectedPanel {
            case "agents":
                AgentsListView(viewModel: viewModel)
            case "sync":
                SkillGitSyncView(viewModel: viewModel)
            default:
                skillsContent
            }
        }
        .onAppear {
            viewModel.refresh()
            CoreBridge.shared.skillsWatchStart()
        }
        .onDisappear {
            CoreBridge.shared.skillsWatchStop()
        }
    }

    private var panelPicker: some View {
        HStack(spacing: 0) {
            Button { selectedPanel = "skills" } label: {
                Label(L10n.shared.skills, systemImage: "puzzlepiece.extension")
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 10)
            }
            .buttonStyle(.borderless)
            .background(selectedPanel == "skills" ? Color.accentColor.opacity(0.12) : Color.clear)
            .contentShape(Rectangle())

            Button { selectedPanel = "agents" } label: {
                Label("Agents", systemImage: "rectangle.stack")
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 10)
            }
            .buttonStyle(.borderless)
            .background(selectedPanel == "agents" ? Color.accentColor.opacity(0.12) : Color.clear)
            .contentShape(Rectangle())

            Button { selectedPanel = "sync" } label: {
                Label("Sync", systemImage: "arrow.triangle.merge")
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 10)
            }
            .buttonStyle(.borderless)
            .background(selectedPanel == "sync" ? Color.accentColor.opacity(0.12) : Color.clear)
            .contentShape(Rectangle())
        }
        .padding(.horizontal, 8)
        .padding(.top, 4)
    }

    @ViewBuilder
    private var skillsContent: some View {
        VStack(spacing: 0) {
            toolbar
            Divider()
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
    }

    private var toolbar: some View {
        HStack(spacing: 12) {
            TextField(L10n.shared.skillSearchPlaceholder, text: $viewModel.searchText)
                .textFieldStyle(.roundedBorder)

            Picker(L10n.shared.skillFilter, selection: $viewModel.selectedFilter) {
                Text(L10n.shared.skillAll).tag("all")
                ForEach(viewModel.agents) { agent in
                    Text(agent.name).tag(agent.id)
                }
            }
            .frame(width: 150)

            Button {
                viewModel.refresh()
            } label: {
                Label(L10n.shared.skillFetch, systemImage: "arrow.clockwise")
            }
        }
        .padding(12)
    }

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
