import SwiftUI

struct AgentSettingsView: View {
    @ObservedObject var viewModel: SkillManagerViewModel
    @State private var name = ""
    @State private var path = ""
    @State private var linkStrategy = "Directory"
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        Form {
            TextField(l10n.agentName, text: $name)
            TextField(l10n.agentSkillsPath, text: $path)
            Picker(l10n.linkStrategy, selection: $linkStrategy) {
                Text("Directory").tag("Directory")
                Text("Single File").tag("SingleFile")
                Text("Overlay").tag("Overlay")
            }
            Button(l10n.addAgent) {
                let payload: [String: Any] = [
                    "name": name,
                    "skills_path": path,
                    "link_type": linkStrategy
                ]
                if let data = try? JSONSerialization.data(withJSONObject: payload) {
                    viewModel.addCustomAgent(data: data)
                }
            }
        }
        .padding()
    }
}
