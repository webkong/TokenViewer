import SwiftUI

struct SkillGitSyncView: View {
    @ObservedObject var viewModel: SkillManagerViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Button {
                    viewModel.refreshGitStatus()
                } label: {
                    Label(L10n.shared.refresh, systemImage: "arrow.clockwise")
                }

                Button {
                    viewModel.pullSkills()
                } label: {
                    Label(L10n.shared.pull, systemImage: "arrow.down.circle")
                }

                Button {
                    viewModel.pushSkills()
                } label: {
                    Label(L10n.shared.push, systemImage: "arrow.up.circle")
                }
            }

            if viewModel.gitChanges.isEmpty {
                Text("No pending changes")
                    .foregroundStyle(.secondary)
                    .padding(.vertical, 8)
            } else {
                List(Array(viewModel.gitChanges.enumerated()), id: \.offset) { _, change in
                    HStack {
                        Text(change.changeType)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(change.changeType == "added" ? .green : .orange)
                        Text(change.filePath)
                            .font(.caption)
                    }
                }
            }
        }
        .padding()
    }
}
