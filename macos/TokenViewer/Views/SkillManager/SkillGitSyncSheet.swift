import SwiftUI

struct SkillGitSyncSheet: View {
    @ObservedObject var viewModel: SkillManagerViewModel
    @Environment(\.dismiss) private var dismiss
    @State private var gitPlatform: String = "custom"
    @State private var gitRemoteURL: String = ""
    @State private var gitToken: String = ""
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Git Sync").font(.headline)
                Spacer()
                Button("Done") { dismiss() }
                    .keyboardShortcut(.escape)
            }
            .padding(.horizontal)
            .padding(.top, 16)
            .padding(.bottom, 8)

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    statusSection
                    Divider()
                    platformSection
                    Divider()
                    configSection
                    Divider()
                    changesSection
                }
                .padding()
            }
        }
        .frame(width: 500, height: 580)
        .onAppear {
            viewModel.refreshGitStatus()
            loadConfig()
        }
    }

    // MARK: - Status

    private var statusSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Status").font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary)

            HStack(spacing: 20) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Branch").font(.caption).foregroundStyle(.secondary)
                    Text(viewModel.gitStatusBranch ?? "(no branch)")
                        .font(.system(size: 13, design: .monospaced))
                }
                VStack(alignment: .leading, spacing: 4) {
                    Text("Ahead").font(.caption).foregroundStyle(.secondary)
                    Text("\(viewModel.gitStatusAhead)").font(.system(size: 13, design: .monospaced))
                }
                VStack(alignment: .leading, spacing: 4) {
                    Text("Behind").font(.caption).foregroundStyle(.secondary)
                    Text("\(viewModel.gitStatusBehind)").font(.system(size: 13, design: .monospaced))
                }
                Spacer()
                HStack(spacing: 8) {
                    Button { viewModel.pullSkills() } label: {
                        Label(l10n.pull, systemImage: "arrow.down.circle")
                    }
                    .controlSize(.small)
                    Button { viewModel.pushSkills() } label: {
                        Label(l10n.push, systemImage: "arrow.up.circle")
                    }
                    .controlSize(.small)
                    Button { viewModel.refreshGitStatus() } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                    .controlSize(.small)
                }
            }
        }
    }

    // MARK: - Platform

    private var platformSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Platform").font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary)

            VStack(spacing: 6) {
                Picker("", selection: $gitPlatform) {
                    Text("GitHub").tag("github")
                    Text("GitLab").tag("gitlab")
                    Text("Custom Git").tag("custom")
                }
                .pickerStyle(.segmented)
                .labelsHidden()

                if gitPlatform == "github" {
                    VStack(alignment: .leading, spacing: 4) {
                        HStack(spacing: 6) {
                            TextField("username/repo", text: $githubRepo)
                                .textFieldStyle(.roundedBorder)
                            Text(".git")
                                .font(.system(size: 12, design: .monospaced))
                                .foregroundStyle(.secondary)
                        }
                        Text("Remote: https://github.com/\(githubRepo).git")
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                            .padding(.leading, 2)
                    }
                } else if gitPlatform == "gitlab" {
                    VStack(alignment: .leading, spacing: 4) {
                        HStack(spacing: 6) {
                            TextField("username/repo", text: $gitlabRepo)
                                .textFieldStyle(.roundedBorder)
                            Text(".git")
                                .font(.system(size: 12, design: .monospaced))
                                .foregroundStyle(.secondary)
                        }
                        Text("Remote: https://gitlab.com/\(gitlabRepo).git")
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                            .padding(.leading, 2)
                    }
                } else {
                    TextField("https://example.com/repo.git", text: $gitRemoteURL)
                        .textFieldStyle(.roundedBorder)
                }
            }
        }
    }

    // MARK: - Config

    private var configSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Authentication").font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary)

            HStack {
                Text("Token").font(.caption).frame(width: 60, alignment: .leading)
                SecureField(
                    gitPlatform == "github" ? "ghp_xxxxxxxxxxxxxxxxxxxx" :
                    gitPlatform == "gitlab" ? "glpat-xxxxxxxxxxxx" :
                    "Personal access token",
                    text: $gitToken
                )
                .textFieldStyle(.roundedBorder)
            }
            HStack {
                Spacer()
                Button("Save Config") {
                    saveConfig()
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
        }
    }

    // MARK: - Changes

    private var changesSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Pending Changes").font(.system(size: 11, weight: .semibold)).foregroundStyle(.secondary)

            if viewModel.gitChanges.isEmpty {
                Text("No pending changes")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .padding(.vertical, 8)
            } else {
                List(viewModel.gitChanges, id: \.filePath) { change in
                    HStack {
                        Image(systemName: change.changeType == "added" ? "plus.circle.fill" :
                              change.changeType == "deleted" ? "minus.circle.fill" :
                              "pencil.circle.fill")
                            .foregroundStyle(change.changeType == "added" ? .green :
                                             change.changeType == "deleted" ? .red : .orange)
                            .font(.caption)
                        Text(change.filePath)
                            .font(.caption)
                            .lineLimit(1)
                        Spacer()
                        Text(change.changeType)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
                .listStyle(.plain)
                .frame(minHeight: 100)
            }
        }
    }

    // MARK: - State for GitHub/GitLab fields
    @State private var githubRepo: String = ""
    @State private var gitlabRepo: String = ""

    // MARK: - Load / Save config

    private func loadConfig() {
        guard let data = CoreBridge.shared.skillsGetConfig() else { return }
        struct Config: Codable {
            struct GitConfig: Codable {}
            let gitRemoteUrl: String
            let gitPlatform: String
        }
        guard let config = try? JSONDecoder().decode(Config.self, from: data) else { return }
        gitPlatform = config.gitPlatform
        let url = config.gitRemoteUrl

        if gitPlatform == "github", let range = url.range(of: "github.com/") {
            let rest = url[range.upperBound...].replacingOccurrences(of: ".git", with: "")
            githubRepo = String(rest)
        } else if gitPlatform == "gitlab", let range = url.range(of: "gitlab.com/") {
            let rest = url[range.upperBound...].replacingOccurrences(of: ".git", with: "")
            gitlabRepo = String(rest)
        } else {
            gitRemoteURL = url
        }
    }

    private func saveConfig() {
        let remoteURL: String
        switch gitPlatform {
        case "github":
            let repo = githubRepo.trimmingCharacters(in: .whitespaces)
            remoteURL = repo.isEmpty ? "" : "https://github.com/\(repo).git"
        case "gitlab":
            let repo = gitlabRepo.trimmingCharacters(in: .whitespaces)
            remoteURL = repo.isEmpty ? "" : "https://gitlab.com/\(repo).git"
        default:
            remoteURL = gitRemoteURL.trimmingCharacters(in: .whitespaces)
        }

        var payload: [String: String] = [
            "remote_url": remoteURL,
            "platform": gitPlatform,
        ]
        let token = gitToken.trimmingCharacters(in: .whitespaces)
        if !token.isEmpty {
            payload["token"] = token
        }
        guard let data = try? JSONSerialization.data(withJSONObject: payload) else { return }
        _ = CoreBridge.shared.skillsSetGitConfig(data)
    }
}
