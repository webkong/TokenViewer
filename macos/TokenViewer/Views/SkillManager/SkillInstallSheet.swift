import AppKit
import Foundation
import SwiftUI
import UniformTypeIdentifiers

struct SkillInstallSheet: View {
    @ObservedObject var viewModel: SkillManagerViewModel
    @ObservedObject private var l10n = L10n.shared
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            header
            sourcePicker
            sourceInput
            destinationInfo
            candidateSelection

            if let errorMessage = viewModel.installErrorMessage {
                messageRow(icon: "exclamationmark.triangle.fill", text: errorMessage, color: .red)
            } else if let successMessage = viewModel.installSuccessMessage {
                messageRow(icon: "checkmark.circle.fill", text: successMessage, color: .green)
            }

            Divider()
            footer
        }
        .padding(22)
        .frame(width: 560)
        .onChange(of: viewModel.installSourceType) { _, _ in viewModel.resetInstallSelection() }
        .onChange(of: viewModel.installSelectedPath) { _, _ in viewModel.resetInstallSelection() }
        .onChange(of: viewModel.installGitURL) { _, _ in viewModel.resetInstallSelection() }
        .onAppear {
            viewModel.loadInstallSourceRootDisplay()
        }
        .onDisappear {
            viewModel.resetInstallForm()
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(l10n.skillInstallTitle)
                .font(.system(size: 20, weight: .semibold))
            Text(l10n.skillInstallDesc)
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
        }
    }

    private var sourcePicker: some View {
        Picker("", selection: $viewModel.installSourceType) {
            ForEach(SkillInstallSourceType.allCases) { type in
                Label(type.title(l10n), systemImage: type.icon)
                    .tag(type)
            }
        }
        .pickerStyle(.segmented)
        .labelsHidden()
    }

    @ViewBuilder
    private var sourceInput: some View {
        switch viewModel.installSourceType {
        case .folder:
            pathInput(
                title: l10n.skillInstallFolder,
                placeholder: l10n.skillInstallFolderPlaceholder,
                buttonTitle: l10n.skillInstallChooseFolder,
                action: chooseFolder
            )
        case .zip:
            pathInput(
                title: l10n.skillInstallZip,
                placeholder: l10n.skillInstallZipPlaceholder,
                buttonTitle: l10n.skillInstallChooseZip,
                action: chooseZip
            )
        case .git:
            VStack(alignment: .leading, spacing: 8) {
                Text(l10n.skillInstallGit)
                    .font(.system(size: 12, weight: .semibold))
                TextField(l10n.skillInstallGitPlaceholder, text: $viewModel.installGitURL)
                    .textFieldStyle(.roundedBorder)
            }
        }
    }

    private func pathInput(title: String, placeholder: String, buttonTitle: String, action: @escaping () -> Void) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.system(size: 12, weight: .semibold))
            HStack(spacing: 8) {
                TextField(placeholder, text: $viewModel.installSelectedPath)
                    .textFieldStyle(.roundedBorder)
                Button(buttonTitle, action: action)
                    .controlSize(.small)
            }
        }
    }

    private var destinationInfo: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Image(systemName: "folder")
                    .foregroundStyle(.secondary)
                Text(l10n.skillsSourceRoot)
                    .font(.system(size: 12, weight: .semibold))
                Text(viewModel.installSourceRootDisplay)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Toggle(l10n.skillInstallReplaceExisting, isOn: $viewModel.installReplaceExisting)
                .toggleStyle(.checkbox)
                .font(.system(size: 12))
        }
        .padding(10)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
    }

    private var footer: some View {
        HStack {
            Spacer()
            Button(l10n.cancel) {
                dismiss()
            }
            .disabled(viewModel.installIsInstalling)

            Button {
                viewModel.runSkillInstall()
            } label: {
                if viewModel.installIsInstalling {
                    ProgressView()
                        .controlSize(.small)
                } else {
                    Text(viewModel.installCandidates.isEmpty ? l10n.skillInstall : l10n.skillInstallSelected)
                }
            }
            .buttonStyle(.borderedProminent)
            .disabled(viewModel.installIsInstalling || !canInstall)
        }
    }

    private var canInstall: Bool {
        if !viewModel.installCandidates.isEmpty && viewModel.installSelectedSkillIDs.isEmpty {
            return false
        }
        switch viewModel.installSourceType {
        case .folder, .zip:
            return !viewModel.installSelectedPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        case .git:
            return !viewModel.installGitURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }
    }

    @ViewBuilder
    private var candidateSelection: some View {
        if !viewModel.installCandidates.isEmpty {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Text(l10n.skillInstallSelectSkills)
                        .font(.system(size: 12, weight: .semibold))
                    Spacer()
                    Button(l10n.skillInstallSelectAll) {
                        viewModel.installSelectedSkillIDs = Set(viewModel.installCandidates.map(\.id))
                    }
                    .buttonStyle(.borderless)
                    .font(.system(size: 11))
                    Button(l10n.skillInstallSelectNone) {
                        viewModel.installSelectedSkillIDs.removeAll()
                    }
                    .buttonStyle(.borderless)
                    .font(.system(size: 11))
                }

                ScrollView {
                    VStack(alignment: .leading, spacing: 6) {
                        ForEach(viewModel.installCandidates) { candidate in
                            Toggle(isOn: Binding(
                                get: { viewModel.installSelectedSkillIDs.contains(candidate.id) },
                                set: { isOn in
                                    if isOn {
                                        viewModel.installSelectedSkillIDs.insert(candidate.id)
                                    } else {
                                        viewModel.installSelectedSkillIDs.remove(candidate.id)
                                    }
                                }
                            )) {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(candidate.id)
                                        .font(.system(size: 12, weight: .medium))
                                    Text(candidate.sourceDir)
                                        .font(.system(size: 10))
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                        .truncationMode(.middle)
                                }
                            }
                            .toggleStyle(.checkbox)
                        }
                    }
                    .padding(.vertical, 2)
                }
                .frame(maxHeight: 160)
            }
            .padding(10)
            .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
        }
    }

    private func messageRow(icon: String, text: String, color: Color) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: icon)
                .foregroundStyle(color)
            Text(text)
                .font(.system(size: 12))
                .foregroundStyle(color)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            viewModel.installSelectedPath = url.path
            viewModel.resetInstallSelection()
        }
    }

    private func chooseZip() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = false
        panel.canChooseFiles = true
        panel.allowsMultipleSelection = false
        panel.allowedContentTypes = [.zip]
        if panel.runModal() == .OK, let url = panel.url {
            viewModel.installSelectedPath = url.path
            viewModel.resetInstallSelection()
        }
    }
}

enum SkillInstallSourceType: String, CaseIterable, Identifiable, Codable {
    case folder
    case zip
    case git

    var id: String { rawValue }

    var icon: String {
        switch self {
        case .folder: return "folder"
        case .zip: return "doc.zipper"
        case .git: return "arrow.triangle.branch"
        }
    }

    func title(_ l10n: L10n) -> String {
        switch self {
        case .folder: return l10n.skillInstallFolder
        case .zip: return l10n.skillInstallZip
        case .git: return l10n.skillInstallGit
        }
    }
}

struct SkillInstallPayload: Codable {
    let sourceType: SkillInstallSourceType
    let path: String
    let gitURL: String
    let replaceExisting: Bool
    let selectedSkillIDs: [String]

    enum CodingKeys: String, CodingKey {
        case sourceType = "source_type"
        case path
        case gitURL = "git_url"
        case replaceExisting = "replace_existing"
        case selectedSkillIDs = "selected_skill_ids"
    }
}

struct SkillInstallCandidate: Codable, Identifiable, Hashable {
    let id: String
    let sourceDir: String

    enum CodingKeys: String, CodingKey {
        case id
        case sourceDir = "source_dir"
    }
}

struct SkillInstallResponse: Codable {
    let ok: Bool
    let status: String
    let installedSkillIds: [String]
    let candidates: [SkillInstallCandidate]
    let error: String?

    enum CodingKeys: String, CodingKey {
        case ok
        case status
        case installedSkillIds = "installed_skill_ids"
        case candidates
        case error
    }
}

enum SkillInstallCore {
    static func sourceRootDisplay() -> String? {
        guard let data = CoreBridge.shared.skillsGetConfig() else { return nil }
        struct Config: Codable { let sourceRoot: String }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return (try? decoder.decode(Config.self, from: data))?.sourceRoot
    }

    static func install(_ request: SkillInstallPayload) throws -> SkillInstallResponse {
        let encoder = JSONEncoder()
        let payload = try encoder.encode(request)
        guard let data = CoreBridge.shared.skillsInstall(payload) else {
            throw SkillInstallCoreError.emptyResponse
        }
        let decoder = JSONDecoder()
        let response = try decoder.decode(SkillInstallResponse.self, from: data)
        if !response.ok {
            throw SkillInstallCoreError.operationFailed(response.error ?? L10n.shared.skillOperationFailed)
        }
        return response
    }
}

enum SkillInstallCoreError: LocalizedError {
    case emptyResponse
    case operationFailed(String)

    var errorDescription: String? {
        switch self {
        case .emptyResponse:
            return "Empty response from Skill Manager core"
        case .operationFailed(let message):
            return message
        }
    }
}
