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
    @AppStorage("syncGitUserName") private var storedGitUserName = ""
    @AppStorage("syncGitUserEmail") private var storedGitUserEmail = ""
    @AppStorage("syncSkillFilterEnabled") private var filterEnabled = false
    @AppStorage("syncSkillFilterPrefixes") private var filterPrefixes = ""
    @AppStorage("syncSkillFilterSelectedIDs") private var filterSelectedIDsJSON = "[]"

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
                    filterSection
                    statusSection
                    changesSection
                    actionSection
                }
                .padding()
            }
        }
        .frame(width: 560, height: 720)
        .clearInitialFocus(trigger: providerRaw)
        .clearFocusOnOutsideClick()
        .sheet(isPresented: $showAuthSheet) {
            SkillAuthSheet(provider: $providerRaw) { userName, userEmail in
                storedGitUserName = userName.trimmingCharacters(in: .whitespacesAndNewlines)
                storedGitUserEmail = userEmail.trimmingCharacters(in: .whitespacesAndNewlines)
                applyConfig(showToast: true, userName: userName, userEmail: userEmail)
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
                Image(systemName: "arrow.triangle.2.circlepath")
                    .font(.system(size: 13, weight: .semibold))
                    .rotationEffect(.degrees(isCheckingConnectivity ? 360 : 0))
                    .animation(isCheckingConnectivity ? .linear(duration: 1).repeatForever(autoreverses: false) : .default, value: isCheckingConnectivity)
            }
            .buttonStyle(.borderless)
            .disabled(isCheckingConnectivity)
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
                    Text(displayedStatusMessage(message))
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
        let changes = displayedGitChanges
        if !changes.isEmpty {
            GroupBox(filterEnabled ? l10n.gitPendingFilteredChanges : l10n.gitPendingChanges) {
                VStack(alignment: .leading, spacing: 4) {
                    if filterEnabled {
                        Text(l10n.gitPendingFilteredChangesDesc)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .padding(.bottom, 2)
                    }

                    ForEach(changes, id: \.filePath) { change in
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

    private var displayedGitChanges: [SkillGitChange] {
        guard filterEnabled else { return viewModel.gitChanges }
        return viewModel.gitChanges.filter { change in
            guard let skillID = skillID(forChangePath: change.filePath) else { return false }
            return filterSelectedIDs.contains(skillID) || skillMatchesPrefixes(skillID)
        }
    }

    private func skillID(forChangePath path: String) -> String? {
        let trimmed = path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        guard !trimmed.isEmpty else { return nil }
        let skillID = trimmed.split(separator: "/", maxSplits: 1).first.map(String.init) ?? trimmed
        guard !skillID.isEmpty, !skillID.hasPrefix(".") else { return nil }
        return skillID
    }

    private var actionSection: some View {
        HStack(spacing: 12) {
            Button {
                AppFocus.clear()
                viewModel.pullSkills(
                    remoteURL: repoURL,
                    platform: provider.key,
                    token: currentToken,
                    userName: storedGitUserName,
                    userEmail: storedGitUserEmail
                )
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
                viewModel.pushSkills(
                    remoteURL: repoURL,
                    platform: provider.key,
                    token: currentToken,
                    userName: storedGitUserName,
                    userEmail: storedGitUserEmail,
                    filterPayload: syncFilterPayload()
                )
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
        if filterEnabled, viewModel.gitStatusName == "modified" {
            return displayedGitChanges.isEmpty ? l10n.gitUpToDateFiltered : l10n.gitFilteredChangesPending
        }

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

    private func displayedStatusMessage(_ message: String) -> String {
        guard filterEnabled, viewModel.gitStatusName == "modified" else { return message }
        let count = displayedGitChanges.count
        if count == 0 {
            return l10n.gitNoFilteredChanges
        }
        return l10n.gitFilteredChangesCount(count)
    }

    private func checkConnectivity() {
        isCheckingConnectivity = true
        viewModel.refreshGitConnectivity {
            isCheckingConnectivity = false
        }
    }

    private func applyConfig(showToast: Bool, userName: String? = nil, userEmail: String? = nil) {
        let token = currentToken
        guard viewModel.applyGitConfig(
            remoteURL: repoURL,
            platform: provider.key,
            token: token,
            userName: userName ?? storedGitUserName,
            userEmail: userEmail ?? storedGitUserEmail
        ) else { return }
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
            let gitUserName: String?
            let gitUserEmail: String?
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
        storedGitUserName = config.gitUserName ?? ""
        storedGitUserEmail = config.gitUserEmail ?? ""
    }

    private var filterSection: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 12) {
                Toggle(isOn: $filterEnabled) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(l10n.skillSyncFilter)
                            .font(.subheadline.weight(.semibold))
                        Text(l10n.skillSyncFilterDesc)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                .toggleStyle(.switch)

                if filterEnabled {
                    VStack(alignment: .leading, spacing: 8) {
                        Text(l10n.skillSyncFilterPrefixes)
                            .font(.caption.weight(.semibold))
                        TextField(l10n.skillSyncFilterPrefixesPlaceholder, text: $filterPrefixes, axis: .vertical)
                            .textFieldStyle(.roundedBorder)
                            .lineLimit(2...4)
                        Text(l10n.skillSyncFilterPrefixesHelp)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }

                    HStack {
                        Text(l10n.skillSyncFilterSelectedSkills)
                            .font(.caption.weight(.semibold))
                        Spacer()
                        Text(l10n.skillSyncFilterSelectedCount(filterSelectedIDs.count))
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }

                    HStack(spacing: 8) {
                        Button(l10n.skillSyncFilterSelectPrefixMatches) {
                            setFilterSelectedIDs(Set(filterableSkills.filter { skillMatchesPrefixes($0.id) }.map(\.id)))
                        }
                        .buttonStyle(.borderless)

                        Button(l10n.skillInstallSelectAll) {
                            setFilterSelectedIDs(Set(filterableSkills.map(\.id)))
                        }
                        .buttonStyle(.borderless)

                        Button(l10n.skillInstallSelectNone) {
                            setFilterSelectedIDs([])
                        }
                        .buttonStyle(.borderless)
                    }
                    .font(.caption)

                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 6) {
                            ForEach(filterableSkills) { skill in
                                Toggle(isOn: bindingForFilterSkill(skill.id)) {
                                    HStack(spacing: 8) {
                                        Text(skill.manifest.name)
                                            .font(.caption)
                                            .lineLimit(1)
                                        Text(skill.id)
                                            .font(.caption2.monospaced())
                                            .foregroundStyle(.secondary)
                                            .lineLimit(1)
                                        if skillMatchesPrefixes(skill.id) {
                                            Text(l10n.skillSyncFilterPrefixMatched)
                                                .font(.caption2)
                                                .foregroundStyle(.green)
                                        }
                                    }
                                }
                                .toggleStyle(.checkbox)
                            }
                        }
                        .padding(.vertical, 4)
                    }
                    .frame(maxHeight: 150)
                }
            }
            .padding(10)
        }
    }

    private var filterableSkills: [SkillEntry] {
        viewModel.skills
            .filter { !viewModel.isBuiltInSkill($0) }
            .sorted {
                $0.manifest.name.localizedCaseInsensitiveCompare($1.manifest.name) == .orderedAscending
            }
    }

    private var filterSelectedIDs: Set<String> {
        guard let data = filterSelectedIDsJSON.data(using: .utf8),
              let ids = try? JSONDecoder().decode([String].self, from: data) else {
            return []
        }
        return Set(ids)
    }

    private var parsedFilterPrefixes: [String] {
        filterPrefixes
            .components(separatedBy: CharacterSet(charactersIn: ",\n\r\t "))
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines).trimmingCharacters(in: CharacterSet(charactersIn: "*")) }
            .filter { !$0.isEmpty && !$0.contains("/") }
    }

    private func skillMatchesPrefixes(_ skillID: String) -> Bool {
        parsedFilterPrefixes.contains { skillID.hasPrefix($0) }
    }

    private func bindingForFilterSkill(_ skillID: String) -> Binding<Bool> {
        Binding(
            get: { filterSelectedIDs.contains(skillID) },
            set: { isSelected in
                var ids = filterSelectedIDs
                if isSelected {
                    ids.insert(skillID)
                } else {
                    ids.remove(skillID)
                }
                setFilterSelectedIDs(ids)
            }
        )
    }

    private func setFilterSelectedIDs(_ ids: Set<String>) {
        let sorted = ids.sorted()
        guard let data = try? JSONEncoder().encode(sorted),
              let raw = String(data: data, encoding: .utf8) else {
            return
        }
        filterSelectedIDsJSON = raw
    }

    private func syncFilterPayload() -> Data? {
        guard filterEnabled else { return nil }
        let payload = SkillSyncFilterPayload(
            includePrefixes: parsedFilterPrefixes,
            includeSkillIds: Array(filterSelectedIDs).sorted()
        )
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        return try? encoder.encode(payload)
    }
}

private struct SkillSyncFilterPayload: Encodable {
    let includePrefixes: [String]
    let includeSkillIds: [String]
}

struct SkillAuthSheet: View {
    @Binding var provider: String
    var onSave: ((String, String) -> Void)?

    @AppStorage("syncTokenSaved_github") private var tokenSavedGithub = false
    @AppStorage("syncTokenSaved_gitlab") private var tokenSavedGitlab = false
    @AppStorage("syncTokenSaved_other") private var tokenSavedOther = false
    @AppStorage("syncGitUserName") private var storedGitUserName = ""
    @AppStorage("syncGitUserEmail") private var storedGitUserEmail = ""
    @Environment(\.dismiss) private var dismiss
    @ObservedObject private var l10n = L10n.shared

    @State private var showTokenHelp = false
    @State private var tokenGithub = ""
    @State private var tokenGitlab = ""
    @State private var tokenOther = ""
    @State private var gitUserName = ""
    @State private var gitUserEmail = ""
    @State private var defaultGitUserName = ""
    @State private var defaultGitUserEmail = ""

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
                Text(l10n.gitCommitIdentity)
                    .font(.subheadline.weight(.medium))

                TextField(l10n.gitUserName, text: $gitUserName)
                    .textFieldStyle(.roundedBorder)

                TextField(l10n.gitUserEmail, text: $gitUserEmail)
                    .textFieldStyle(.roundedBorder)

                VStack(alignment: .leading, spacing: 4) {
                    Text(defaultIdentityText)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                    Text(l10n.gitCommitIdentityDesc)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
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
                    guard saveCurrentToken() else { return }
                    onSave?(gitUserName, gitUserEmail)
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
                .buttonStyle(.borderedProminent)
                .disabled(currentTokenBinding.wrappedValue.isEmpty)
            }
        }
        .padding(24)
        .frame(width: 420, height: 455)
        .clearInitialFocus(trigger: provider)
        .clearFocusOnOutsideClick()
        .onAppear {
            loadTokensFromKeychain()
            loadGitIdentityConfig()
        }
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

    private var defaultIdentityText: String {
        let name = defaultGitUserName.trimmingCharacters(in: .whitespacesAndNewlines)
        let email = defaultGitUserEmail.trimmingCharacters(in: .whitespacesAndNewlines)

        if !name.isEmpty, !email.isEmpty {
            return l10n.gitDefaultIdentity("\(name) <\(email)>")
        }
        if !name.isEmpty {
            return l10n.gitDefaultIdentity(name)
        }
        if !email.isEmpty {
            return l10n.gitDefaultIdentity(email)
        }
        return l10n.gitDefaultIdentityMissing
    }

    private func loadTokensFromKeychain() {
        tokenGithub = KeychainManager.shared.getToken(for: "github") ?? ""
        tokenGitlab = KeychainManager.shared.getToken(for: "gitlab") ?? ""
        tokenOther = KeychainManager.shared.getToken(for: "other") ?? ""
    }

    private func loadGitIdentityConfig() {
        guard let data = CoreBridge.shared.skillsGetConfig() else { return }
        struct Config: Codable {
            let gitUserName: String?
            let gitUserEmail: String?
            let defaultGitUserName: String?
            let defaultGitUserEmail: String?
        }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let config = try? decoder.decode(Config.self, from: data) else { return }
        gitUserName = config.gitUserName ?? storedGitUserName
        gitUserEmail = config.gitUserEmail ?? storedGitUserEmail
        defaultGitUserName = config.defaultGitUserName ?? ""
        defaultGitUserEmail = config.defaultGitUserEmail ?? ""
    }

    private func saveCurrentToken() -> Bool {
        let token = currentTokenBinding.wrappedValue
        guard !token.isEmpty else { return false }
        do {
            try KeychainManager.shared.saveToken(token, for: currentProvider.key)
            setTokenSaved(true)
            storedGitUserName = gitUserName.trimmingCharacters(in: .whitespacesAndNewlines)
            storedGitUserEmail = gitUserEmail.trimmingCharacters(in: .whitespacesAndNewlines)
            return true
        } catch {
            ToastCenter.shared.error(l10n.toastSaveFailed)
            return false
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
