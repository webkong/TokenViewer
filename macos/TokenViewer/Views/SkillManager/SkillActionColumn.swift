import SwiftUI

struct SkillActionColumn: View {
    let skill: SkillEntry
    @ObservedObject var viewModel: SkillManagerViewModel
    @State private var showingDeleteConfirmation = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                viewModel.organize(skill: skill)
            } label: {
                Text("Organize")
                    .font(.caption.weight(.semibold))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 5)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.small)

            Button {
                viewModel.restore(skill: skill)
            } label: {
                Text("Restore")
                    .font(.caption.weight(.semibold))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 5)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .tint(.orange)

            Button(role: .destructive) {
                showingDeleteConfirmation = true
            } label: {
                Text("Delete")
                    .font(.caption.weight(.semibold))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 5)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .confirmationDialog("Delete skill?", isPresented: $showingDeleteConfirmation) {
                Button("Delete", role: .destructive) {
                    viewModel.delete(skill: skill)
                }
                Button("Cancel", role: .cancel) {}
            }
        }
    }
}
