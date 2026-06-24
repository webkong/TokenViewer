import SwiftUI

struct AgentsListView: View {
    @ObservedObject var viewModel: SkillManagerViewModel
    @State private var showAddSheet = false
    @State private var showDeleteConfirmation = false
    @State private var agentToDelete: SkillAgent?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("Agents")
                    .font(.headline)
                Spacer()
                Button {
                    showAddSheet = true
                } label: {
                    Label("Add Agent", systemImage: "plus")
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)

            Divider()

            if viewModel.agents.isEmpty {
                VStack(spacing: 12) {
                    Spacer()
                    Image(systemName: "rectangle.stack")
                        .font(.system(size: 40))
                        .foregroundStyle(.secondary)
                    Text("No agents")
                        .font(.headline)
                    Text("Add an agent to manage skill linking")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    Spacer()
                }
            } else {
                List(viewModel.agents) { agent in
                    HStack(spacing: 12) {
                        Image(systemName: "puzzlepiece")
                            .font(.title3)
                            .foregroundStyle(.secondary)
                            .frame(width: 32)

                        VStack(alignment: .leading, spacing: 2) {
                            HStack(spacing: 6) {
                                Text(agent.name)
                                    .fontWeight(.medium)
                                if !agent.isBuiltin {
                                    Text("custom")
                                        .font(.caption2.weight(.semibold))
                                        .foregroundStyle(.blue)
                                        .padding(.horizontal, 6)
                                        .padding(.vertical, 1)
                                        .background(Color.blue.opacity(0.1), in: Capsule())
                                }
                            }
                            Text(agent.skillsPath)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }

                        Spacer()

                        HStack(spacing: 2) {
                            Text("\(agent.linkedSkills.count)")
                                .font(.caption.weight(.semibold))
                            Text("skills")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        .padding(.horizontal, 8)
                        .padding(.vertical, 3)
                        .background(.quinary, in: Capsule())
                    }
                    .padding(.vertical, 4)
                    .contextMenu {
                        Button("Show in Finder") {
                            let url = URL(fileURLWithPath: agent.skillsPath)
                            NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: url.path)
                        }
                        if !agent.isBuiltin {
                            Divider()
                            Button("Delete", role: .destructive) {
                                agentToDelete = agent
                                showDeleteConfirmation = true
                            }
                        }
                    }
                }
                .listStyle(.inset)
            }

            Divider()
            Text("\(viewModel.agents.count) agents")
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(8)
        }
        .sheet(isPresented: $showAddSheet) {
            AddAgentSheetView(viewModel: viewModel, isPresented: $showAddSheet)
        }
        .confirmationDialog(
            "Delete agent?",
            isPresented: $showDeleteConfirmation,
            presenting: agentToDelete
        ) { agent in
            Button("Delete", role: .destructive) {
                viewModel.removeCustomAgent(agent.id)
            }
            Button("Cancel", role: .cancel) {}
        }
    }
}

struct AddAgentSheetView: View {
    @ObservedObject var viewModel: SkillManagerViewModel
    @Binding var isPresented: Bool
    @State private var name = ""
    @State private var path = ""
    @State private var linkStrategy = "Directory"

    var isValid: Bool {
        !name.trimmingCharacters(in: .whitespaces).isEmpty
            && !path.trimmingCharacters(in: .whitespaces).isEmpty
    }

    var body: some View {
        VStack(spacing: 0) {
            Text("Add Custom Agent")
                .font(.headline)
                .padding(.top, 20)

            Form {
                Section("Agent Info") {
                    TextField("Name (e.g. My Zed)", text: $name)
                }

                Section("Skills Path") {
                    HStack {
                        TextField("Path to agent's skills directory", text: $path)
                        Button("Browse") {
                            let panel = NSOpenPanel()
                            panel.canChooseDirectories = true
                            panel.canChooseFiles = false
                            if panel.runModal() == .OK, let url = panel.url {
                                path = url.path
                            }
                        }
                    }
                    if !path.isEmpty {
                        let expanded = (path as NSString).expandingTildeInPath
                        HStack(spacing: 4) {
                            Image(systemName: FileManager.default.fileExists(atPath: expanded) ? "checkmark.circle.fill" : "xmark.circle.fill")
                                .foregroundStyle(FileManager.default.fileExists(atPath: expanded) ? .green : .red)
                                .font(.caption)
                            Text(expanded)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }

                Section("Link Type") {
                    Picker("Strategy", selection: $linkStrategy) {
                        Text("Directory").tag("Directory")
                        Text("Single File").tag("SingleFile")
                        Text("Overlay").tag("Overlay")
                    }
                    .pickerStyle(.segmented)
                    Text("How skills are linked to this agent's directory")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .formStyle(.grouped)

            Divider()

            HStack {
                Button("Cancel") {
                    isPresented = false
                }
                Spacer()
                Button("Add Agent") {
                    let payload: [String: Any] = [
                        "name": name.trimmingCharacters(in: .whitespaces),
                        "skills_path": path.trimmingCharacters(in: .whitespaces),
                        "link_type": linkStrategy
                    ]
                    if let data = try? JSONSerialization.data(withJSONObject: payload) {
                        viewModel.addCustomAgent(data: data)
                        isPresented = false
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(!isValid)
            }
            .padding()
        }
        .frame(width: 450, height: 380)
    }
}
