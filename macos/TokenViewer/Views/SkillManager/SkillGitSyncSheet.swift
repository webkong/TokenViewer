import SwiftUI

enum SkillGitProvider: String, CaseIterable {
    case github = "GitHub"
    case gitlab = "GitLab"
    case other = "Other"

    var key: String {
        switch self {
        case .github: return "github"
        case .gitlab: return "gitlab"
        case .other: return "other"
        }
    }

    var host: String {
        switch self {
        case .github: return "github.com"
        case .gitlab: return "gitlab.com"
        case .other: return ""
        }
    }

    var tokenPlaceholder: String {
        switch self {
        case .github: return "ghp_xxxxxxxxxxxx"
        case .gitlab: return "glpat-xxxxxxxxxxxxxx"
        case .other: return "Personal access token"
        }
    }

    var icon: String {
        switch self {
        case .github: return "chevron.left.forwardslash.chevron.right"
        case .gitlab: return "server.rack"
        case .other: return "network"
        }
    }
}

struct SkillGitSyncSheet: View {
    @ObservedObject var viewModel: SkillManagerViewModel
    @Environment(\.dismiss) private var dismiss
    @ObservedObject private var l10n = L10n.shared

    @AppStorage("syncRepoURL") private var repoURL = ""
    @AppStorage("syncProvider") private var providerRaw = SkillGitProvider.github.rawValue
    @AppStorage("syncTokenSaved_github") private var tokenSavedGithub = false
    @AppStorage("syncTokenSaved_gitlab") private var tokenSavedGitlab = false
    @AppStorage("syncTokenSaved_other") private var tokenSavedOther = false

    @State private var showAuthSheet = false
    @State private var isCheckingConnectivity = false

    private var provider: SkillGitProvider {
        SkillGitProvider(rawValue: providerRaw) ?? .github
    }

    private var tokenSaved: Bool {
        switch provider {
        case .github: return tokenSavedGithub
        case .gitlab: return tokenSavedGitlab
        case .other: return tokenSavedOther
        }
    }

    private var repoPlaceholder: String {
        if provider == .other {
            return "https://git.example.com/user/skills-repo.git"
        }
        return "https://\(provider.host)/user/skills-repo.git"
    }

    private var isBusy: Bool {
        viewModel.gitStatusName == "pushing" || viewModel.gitStatusName == "pulling"
    }

    private var currentToken: String {
        KeychainManager.shared.getToken(for: provider.key) ?? ""
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    repositorySection
                    statusSection
                    changesSection
                    actionSection
                }
                .padding()
            }
        }
        .frame(width: 520, height: 580)
        .clearInitialFocus(trigger: providerRaw)
        .sheet(isPresented: $showAuthSheet) {
            SkillAuthSheet(provider: $providerRaw) {
                applyConfig(showToast: true)
            }
        }
        .onAppear {
            loadConfig()
            viewModel.refreshGitStatus()
            checkConnectivity()
        }
    }

    private var header: some View {
        HStack {
            Text(l10n.gitSync)
                .font(.headline)
            Spacer()
            connectivityDot
            Button {
                showAuthSheet = true
            } label: {
                HStack(spacing: 4) {
                    Image(systemName: "gearshape")
                    Text(tokenSaved ? provider.rawValue : l10n.gitAuthorize)
                }
                .font(.caption)
                .padding(.horizontal, 8)
                .padding(.vertical, 4)
                .background(tokenSaved ? Color.green.opacity(0.1) : Color.accentColor.opacity(0.1), in: Capsule())
                .foregroundStyle(tokenSaved ? .green : .accentColor)
            }
            .buttonStyle(.plain)
            .quickHelp(l10n.gitAuthorizeTip)

            Button {
                viewModel.refreshGitStatus(showToast: true)
                checkConnectivity()
            } label: {
                Image(systemName: "arrow.clockwise")
            }
            .buttonStyle(.borderless)
            .quickHelp(l10n.gitRefreshStatusTip)

            Button(l10n.gitDone) { dismiss() }
                .keyboardShortcut(.escape)
                .quickHelp(l10n.gitDoneTip)
        }
        .padding(.horizontal)
        .padding(.vertical, 10)
    }

    private var repositorySection: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 12) {
                Text(l10n.gitRepository)
                    .font(.subheadline.weight(.semibold))

                TextField(repoPlaceholder, text: $repoURL)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit {
                        AppFocus.clear()
                        applyConfig(showToast: true)
                    }

                Text(l10n.gitRepositoryDesc(provider.rawValue))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .padding(10)
        }
    }

    private var statusSection: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    statusIcon
                    Text(statusTitle)
                        .font(.headline)
                    Spacer()
                    Text(viewModel.gitStatusBranch ?? l10n.gitNoBranch)
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                }

                if let message = viewModel.gitStatusMessage {
                    Text(message)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                HStack(spacing: 16) {
                    Text("\(l10n.gitAhead): \(viewModel.gitStatusAhead)")
                    Text("\(l10n.gitBehind): \(viewModel.gitStatusBehind)")
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(8)
        }
    }

    @ViewBuilder
    private var changesSection: some View {
        if !viewModel.gitChanges.isEmpty {
            GroupBox(l10n.gitPendingChanges) {
                VStack(alignment: .leading, spacing: 4) {
                    ForEach(viewModel.gitChanges, id: \.filePath) { change in
                        HStack {
                            Image(systemName: change.changeType == "added" ? "plus.circle.fill" :
                                  change.changeType == "deleted" ? "minus.circle.fill" : "pencil.circle.fill")
                                .foregroundStyle(change.changeType == "added" ? .green :
                                                 change.changeType == "deleted" ? .red : .orange)
                            Text(change.filePath)
                                .font(.caption)
                                .lineLimit(1)
                            Spacer()
                            Text(change.changeType.capitalized)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                        .padding(.vertical, 2)
                    }
                }
                .padding(8)
            }
        }
    }

    private var actionSection: some View {
        HStack(spacing: 12) {
            Button {
                AppFocus.clear()
                viewModel.pullSkills(remoteURL: repoURL, platform: provider.key, token: currentToken)
                checkConnectivity()
            } label: {
                Label(l10n.pull, systemImage: "arrow.down.circle.fill")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
            .controlSize(.large)
            .disabled(repoURL.isEmpty || !tokenSaved || isBusy)
            .quickHelp(l10n.gitPullTip)

            Button {
                AppFocus.clear()
                viewModel.pushSkills(remoteURL: repoURL, platform: provider.key, token: currentToken)
                checkConnectivity()
            } label: {
                Label(l10n.push, systemImage: "arrow.up.circle.fill")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .disabled(repoURL.isEmpty || !tokenSaved || isBusy)
            .quickHelp(l10n.gitPushTip)
        }
    }

    @ViewBuilder
    private var connectivityDot: some View {
        if isCheckingConnectivity {
            HStack(spacing: 4) {
                ProgressView().scaleEffect(0.7)
                Text(l10n.gitChecking)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        } else if viewModel.gitConnectivity?.status == "connected" {
            HStack(spacing: 4) {
                Image(systemName: "circle.fill")
                    .font(.system(size: 7))
                    .foregroundStyle(.green)
                Text(l10n.gitConnected)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        } else if viewModel.gitConnectivity?.status == "disconnected" {
            HStack(spacing: 4) {
                Image(systemName: "circle.fill")
                    .font(.system(size: 7))
                    .foregroundStyle(.red)
                Text(l10n.gitDisconnected)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var statusIcon: some View {
        Group {
            switch viewModel.gitStatusName {
            case "synced":
                Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
            case "modified":
                Image(systemName: "exclamationmark.circle.fill").foregroundStyle(.orange)
            case "conflicted":
                Image(systemName: "xmark.circle.fill").foregroundStyle(.red)
            case "pushing", "pulling":
                ProgressView().scaleEffect(0.7)
            case "error":
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.red)
            default:
                Image(systemName: "circle.dashed").foregroundStyle(.secondary)
            }
        }
        .font(.title2)
    }

    private var statusTitle: String {
        switch viewModel.gitStatusName {
        case "synced": return l10n.gitUpToDate
        case "modified": return l10n.gitChangesPending
        case "conflicted": return l10n.gitConflicts
        case "pushing": return l10n.gitPushing
        case "pulling": return l10n.gitPulling
        case "error": return l10n.gitError
        default: return l10n.gitNotConfigured
        }
    }

    private func checkConnectivity() {
        isCheckingConnectivity = true
        viewModel.refreshGitConnectivity()
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
            isCheckingConnectivity = false
        }
    }

    private func applyConfig(showToast: Bool) {
        let token = currentToken
        guard viewModel.applyGitConfig(remoteURL: repoURL, platform: provider.key, token: token) else { return }
        viewModel.refreshGitStatus()
        if showToast {
            ToastCenter.shared.success(l10n.toastSaved)
        }
    }

    private func loadConfig() {
        guard let data = CoreBridge.shared.skillsGetConfig() else { return }
        struct Config: Codable {
            let gitRemoteUrl: String
            let gitPlatform: String
        }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let config = try? decoder.decode(Config.self, from: data) else { return }
        repoURL = config.gitRemoteUrl
        switch config.gitPlatform {
        case "github": providerRaw = SkillGitProvider.github.rawValue
        case "gitlab": providerRaw = SkillGitProvider.gitlab.rawValue
        default: providerRaw = SkillGitProvider.other.rawValue
        }
    }
}

struct SkillAuthSheet: View {
    @Binding var provider: String
    var onSave: (() -> Void)?

    @AppStorage("syncTokenSaved_github") private var tokenSavedGithub = false
    @AppStorage("syncTokenSaved_gitlab") private var tokenSavedGitlab = false
    @AppStorage("syncTokenSaved_other") private var tokenSavedOther = false
    @Environment(\.dismiss) private var dismiss
    @ObservedObject private var l10n = L10n.shared

    @State private var showTokenHelp = false
    @State private var tokenGithub = ""
    @State private var tokenGitlab = ""
    @State private var tokenOther = ""

    private var currentProvider: SkillGitProvider {
        SkillGitProvider(rawValue: provider) ?? .github
    }

    private var tokenSaved: Bool {
        switch currentProvider {
        case .github: return tokenSavedGithub
        case .gitlab: return tokenSavedGitlab
        case .other: return tokenSavedOther
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Text(l10n.gitAuthorization)
                .font(.title2.bold())

            VStack(alignment: .leading, spacing: 8) {
                Text(l10n.gitProvider)
                    .font(.subheadline.weight(.medium))
                Picker(l10n.gitProvider, selection: $provider) {
                    ForEach(SkillGitProvider.allCases, id: \.rawValue) { provider in
                        Text(provider.rawValue).tag(provider.rawValue)
                    }
                }
                .pickerStyle(.segmented)
            }

            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 6) {
                    Text("\(currentProvider.rawValue) \(l10n.gitToken)")
                        .font(.subheadline.weight(.medium))
                    Button {
                        showTokenHelp = true
                    } label: {
                        Image(systemName: "questionmark.circle.fill")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .buttonStyle(.plain)
                    .popover(isPresented: $showTokenHelp) {
                        tokenHelpContent
                    }
                }

                tokenField

                VStack(alignment: .leading, spacing: 4) {
                    Label(l10n.gitTokenStoredLocally, systemImage: "lock.shield")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                    Text(l10n.gitTokenScopes)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
            }

            if tokenSaved {
                Label(l10n.gitTokenSaved, systemImage: "checkmark.circle.fill")
                    .font(.caption)
                    .foregroundStyle(.green)
            }

            Spacer()

            HStack(spacing: 12) {
                Button(l10n.cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Spacer()
                if tokenSaved {
                    Button(l10n.gitRemoveToken, role: .destructive) {
                        removeCurrentToken()
                    }
                }
                Button(tokenSaved ? l10n.gitUpdateToken : l10n.save) {
                    AppFocus.clear()
                    saveCurrentToken()
                    onSave?()
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .buttonStyle(.borderedProminent)
                .disabled(currentTokenBinding.wrappedValue.isEmpty)
            }
        }
        .padding(24)
        .frame(width: 420, height: 330)
        .clearInitialFocus(trigger: provider)
        .onAppear(perform: loadTokensFromKeychain)
    }

    @ViewBuilder
    private var tokenField: some View {
        SecureField(currentProvider.tokenPlaceholder, text: currentTokenBinding)
            .textFieldStyle(.roundedBorder)
    }

    private var currentTokenBinding: Binding<String> {
        switch currentProvider {
        case .github: return $tokenGithub
        case .gitlab: return $tokenGitlab
        case .other: return $tokenOther
        }
    }

    private var tokenHelpContent: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(l10n.gitTokenHelpTitle(currentProvider.rawValue))
                .font(.headline)
            SkillHelpStep("1", l10n.gitTokenHelpStep1(currentProvider.rawValue))
            SkillHelpStep("2", l10n.gitTokenHelpStep2)
            SkillHelpStep("3", l10n.gitTokenHelpStep3)
            SkillHelpStep("4", l10n.gitTokenHelpStep4)
        }
        .padding(16)
        .frame(width: 280)
    }

    private func loadTokensFromKeychain() {
        tokenGithub = KeychainManager.shared.getToken(for: "github") ?? ""
        tokenGitlab = KeychainManager.shared.getToken(for: "gitlab") ?? ""
        tokenOther = KeychainManager.shared.getToken(for: "other") ?? ""
    }

    private func saveCurrentToken() {
        let token = currentTokenBinding.wrappedValue
        guard !token.isEmpty else { return }
        do {
            try KeychainManager.shared.saveToken(token, for: currentProvider.key)
            setTokenSaved(true)
        } catch {
            ToastCenter.shared.error(l10n.toastSaveFailed)
        }
    }

    private func removeCurrentToken() {
        do {
            try KeychainManager.shared.deleteToken(for: currentProvider.key)
        } catch {
            ToastCenter.shared.error(l10n.toastSaveFailed)
        }
        currentTokenBinding.wrappedValue = ""
        setTokenSaved(false)
    }

    private func setTokenSaved(_ saved: Bool) {
        switch currentProvider {
        case .github: tokenSavedGithub = saved
        case .gitlab: tokenSavedGitlab = saved
        case .other: tokenSavedOther = saved
        }
    }
}

struct SkillHelpStep: View {
    let number: String
    let text: String

    init(_ number: String, _ text: String) {
        self.number = number
        self.text = text
    }

    var body: some View {
        HStack(spacing: 8) {
            Text(number)
                .font(.caption2.bold())
                .foregroundStyle(.white)
                .frame(width: 18, height: 18)
                .background(Circle().fill(Color.accentColor))
            Text(text)
                .font(.caption)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}
