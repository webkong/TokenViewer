import Foundation

/// Payload for the compatibility confirmation alert shown in the skill list
/// when the user tries to link a skill to an agent outside its `compatible_agents`.
struct CompatibilityAlert: Identifiable {
    let id = UUID()
    let skillID: String
    let agentID: String
    let skillName: String
    let agentName: String
}

struct BuiltInOrganizeAlert: Identifiable {
    let id = UUID()
    let skillID: String
    let agentID: String
    let skillName: String
    let agentName: String
}

@MainActor
final class SkillManagerViewModel: ObservableObject {
    static let shared = SkillManagerViewModel()
    static let allFilter = "all"
    static let globalFilter = "global"

    @Published private(set) var skills: [SkillEntry] = []
    @Published private(set) var providers: [SkillProvider] = []
    @Published var selectedFilter: String = SkillManagerViewModel.allFilter
    /// Drives the cross-agent compatibility alert in the skill list.
    @Published var compatibilityAlert: CompatibilityAlert?
    /// Drives the confirmation shown before organizing an agent built-in skill.
    @Published var builtInOrganizeAlert: BuiltInOrganizeAlert?
    /// When false (default), skills that ship with an agent (no manifest.json)
    /// are hidden unless explicitly shown. Reduces clutter from built-ins.
    @Published var showBuiltInSkills: Bool = false

    /// Providers enabled in Settings.
    var visibleProviders: [SkillProvider] {
        let enabled = enabledProviderSet
        return providers
            .filter { enabled.contains($0.source) }
            .sorted { lhs, rhs in
                if lhs.isInstalled != rhs.isInstalled {
                    return lhs.isInstalled && !rhs.isInstalled
                }
                return lhs.displayName.localizedCaseInsensitiveCompare(rhs.displayName) == .orderedAscending
            }
    }

    private var enabledProviderSet: Set<String> {
        guard let raw = UserDefaults.standard.string(forKey: "skillsEnabledProviders"),
              let data = raw.data(using: .utf8),
              let arr = try? JSONDecoder().decode([String].self, from: data)
        else { return Set(ProviderRegistry.defaultSkillSources) }
        return Set(arr)
    }
    @Published var searchText: String = ""
    @Published var isLoading = false
    @Published var errorMessage: String?
    @Published var gitChanges: [SkillGitChange] = []
    @Published var gitStatusAhead: Int = 0
    @Published var gitStatusBehind: Int = 0
    @Published var gitStatusBranch: String? = nil
    @Published var gitStatusName: String? = nil
    @Published var gitStatusMessage: String? = nil
    @Published var gitConnectivity: SkillGitConnectivity? = nil
    @Published var installSourceType: SkillInstallSourceType = .folder
    @Published var installSelectedPath: String = ""
    @Published var installGitURL: String = ""
    @Published var installReplaceExisting: Bool = false
    @Published var installIsInstalling: Bool = false
    @Published var installErrorMessage: String?
    @Published var installSuccessMessage: String?
    @Published var installCandidates: [SkillInstallCandidate] = []
    @Published var installSelectedSkillIDs: Set<String> = []
    @Published var installSourceRootDisplay: String = "~/.agents/skills"

    private let decoder = JSONDecoder()

    private init() {
        decoder.keyDecodingStrategy = .convertFromSnakeCase
    }

    func refresh(showToast: Bool = false) {
        isLoading = true
        errorMessage = nil
        let enabled = enabledProviderSet
        ProviderRegistry.shared.reload()
        let providers = ProviderRegistry.shared.skillProviders

        Task.detached {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            let agentIDs = providers
                .map(\.source)
                .filter { enabled.contains($0) }
            let payload = try? JSONEncoder().encode(["agent_ids": agentIDs])
            let skillsData = payload.flatMap(CoreBridge.shared.skillsListForAgents)
                ?? CoreBridge.shared.skillsList()
            let skills = (try? decoder.decode([SkillEntry].self, from: skillsData ?? Data())) ?? []
            await SkillPreviewCache.shared.invalidate()

            await MainActor.run {
                self.isLoading = false
                self.skills = skills
                self.providers = providers
                if showToast {
                    ToastCenter.shared.success(L10n.shared.toastRefreshed)
                }
            }
        }
    }

    func loadInstallSourceRootDisplay() {
        Task {
            let display = await Task.detached {
                SkillInstallCore.sourceRootDisplay()
            }.value
            await MainActor.run {
                if let display {
                    self.installSourceRootDisplay = display
                }
            }
        }
    }

    func resetInstallSelection() {
        installCandidates = []
        installSelectedSkillIDs = []
        installSuccessMessage = nil
        installErrorMessage = nil
    }

    func resetInstallForm() {
        installSourceType = .folder
        installSelectedPath = ""
        installGitURL = ""
        installReplaceExisting = false
        resetInstallSelection()
    }

    func runSkillInstall() {
        installErrorMessage = nil
        installSuccessMessage = nil
        installIsInstalling = true

        let request = SkillInstallPayload(
            sourceType: installSourceType,
            path: installSelectedPath.trimmingCharacters(in: .whitespacesAndNewlines),
            gitURL: installGitURL.trimmingCharacters(in: .whitespacesAndNewlines),
            replaceExisting: installReplaceExisting,
            selectedSkillIDs: Array(installSelectedSkillIDs).sorted()
        )

        Task.detached {
            do {
                let response = try SkillInstallCore.install(request)
                await MainActor.run {
                    self.installIsInstalling = false
                    if response.status == "selection_required" {
                        self.installCandidates = response.candidates
                        self.installSelectedSkillIDs = Set(response.candidates.map(\.id))
                        self.installSuccessMessage = nil
                        return
                    }

                    let installed = response.installedSkillIds
                    self.installCandidates = []
                    self.installSelectedSkillIDs = []
                    self.installSuccessMessage = L10n.shared.skillInstallSuccessList(installed)
                    self.refresh(showToast: true)
                    ToastCenter.shared.success(L10n.shared.skillInstallSuccessList(installed))
                }
            } catch {
                await MainActor.run {
                    self.installIsInstalling = false
                    let message = L10n.shared.skillInstallFailed(error.localizedDescription)
                    self.installErrorMessage = message
                    ToastCenter.shared.error(message)
                }
            }
        }
    }

    var filteredSkills: [SkillEntry] {
        skills.filter { skill in
            let matchesSearch = searchText.isEmpty
                || skill.manifest.name.localizedCaseInsensitiveContains(searchText)
                || skill.manifest.description.localizedCaseInsensitiveContains(searchText)

            // Hide agent-built-in skills unless the user opts in.
            if !showBuiltInSkills && isBuiltInSkill(skill) {
                return false
            }

            if selectedFilter == Self.allFilter {
                return matchesSearch
            }
            if selectedFilter == Self.globalFilter {
                return matchesSearch && isInSourceRoot(skill)
            }
            return matchesSearch && skillMatchesAgent(skill, agentID: selectedFilter)
        }
    }

    func ensureValidFilter() {
        guard selectedFilter != Self.allFilter,
              selectedFilter != Self.globalFilter else { return }
        if !visibleProviders.contains(where: { $0.source == selectedFilter }) {
            selectedFilter = Self.allFilter
        }
    }

    private func skillMatchesAgent(_ skill: SkillEntry, agentID: String) -> Bool {
        skillAgentIDs(for: skill).contains(agentID)
    }

    /// Check if a skill is linked (via symlink) to a given agent.
    func isSkillLinked(skillID: String, agentID: String) -> Bool {
        providers.first(where: { $0.source == agentID })?.linkedSkills.contains(skillID) == true
    }

    /// True when linking `agentID` for `skillID` should trigger a compatibility
    /// confirmation — i.e. the skill declares specific (non-wildcard) compatible
    /// agents and `agentID` isn't among them.
    func requiresCompatibilityConfirmation(skillID: String, agentID: String) -> Bool {
        guard let skill = skills.first(where: { $0.id == skillID }) else { return false }
        let compat = skill.manifest.compatibleAgents
        if compat.contains("*") { return false }
        return !compat.contains(agentID)
    }

    /// Get all agents that currently have this skill, either from scan results or persisted links.
    func skillAgentIDs(for skill: SkillEntry) -> Set<String> {
        var agentIDs = Set(skill.agentIds)

        for provider in providers where provider.linkedSkills.contains(skill.id) {
            agentIDs.insert(provider.source)
        }

        let sourceDir = standardizedPath(skill.sourceDir)
        for provider in providers where sourceDir.hasPrefix(standardizedPath(provider.skillsPath) + "/") {
            agentIDs.insert(provider.source)
        }

        return agentIDs
    }

    /// Get the primary source agent for actions that need a single target.
    func sourceAgent(for skill: SkillEntry) -> String? {
        if let scanned = skill.agentIds.first {
            return scanned
        }
        let sourceDir = standardizedPath(skill.sourceDir)
        if let physicalSource = providers.first(where: { provider in
            sourceDir.hasPrefix(standardizedPath(provider.skillsPath) + "/")
        })?.source {
            return physicalSource
        }
        return providers.first(where: { $0.linkedSkills.contains(skill.id) })?.source
    }

    func sourceAgent(for skillID: String) -> String? {
        guard let skill = skills.first(where: { $0.id == skillID }) else {
            return providers.first(where: { $0.linkedSkills.contains(skillID) })?.source
        }
        return sourceAgent(for: skill)
    }

    func isInSourceRoot(_ skill: SkillEntry) -> Bool {
        let sourceDir = standardizedPath(skill.sourceDir)
        return !providers.contains { provider in
            sourceDir.hasPrefix(standardizedPath(provider.skillsPath) + "/")
        }
    }

    func isBuiltInSkill(_ skill: SkillEntry) -> Bool {
        skill.isBuiltIn || isCodexSystemSkill(skill)
    }

    private func isCodexSystemSkill(_ skill: SkillEntry) -> Bool {
        guard skillAgentIDs(for: skill).contains("codex"),
              let codex = providers.first(where: { $0.source == "codex" }) else {
            return false
        }

        let systemDir = URL(fileURLWithPath: standardizedPath(codex.skillsPath))
            .appendingPathComponent(".system")
        let marker = systemDir.appendingPathComponent(".codex-system-skills.marker")
        guard FileManager.default.fileExists(atPath: marker.path) else {
            return false
        }

        let systemEntry = systemDir.appendingPathComponent(skill.id).path
        if FileManager.default.fileExists(atPath: systemEntry) {
            return true
        }
        return (try? FileManager.default.destinationOfSymbolicLink(atPath: systemEntry)) != nil
    }

    private func standardizedPath(_ path: String) -> String {
        (NSString(string: path).expandingTildeInPath as NSString).standardizingPath
    }

    func skillMarkdownPreview(for skill: SkillEntry) -> SkillMarkdownPreview {
        SkillPreviewCache.descriptor(for: skill)
    }

    private func decode<T: Decodable>(_ type: T.Type, from data: Data?) throws -> T {
        guard let data else {
            throw SkillManagerError.emptyResponse
        }
        return try decoder.decode(type, from: data)
    }

    func delete(skill: SkillEntry) {
        runSkillCommand(skillID: skill.id, successMessage: L10n.shared.toastDeleted, call: CoreBridge.shared.skillsDelete)
    }

    func organize(skill: SkillEntry, agentID: String? = nil) {
        organizeSkill(skillID: skill.id, agentID: agentID)
    }

    func organizeSkill(skillID: String, agentID: String? = nil) {
        runSkillCommand(skillID: skillID, agentID: agentID, successMessage: L10n.shared.toastOrganized, call: CoreBridge.shared.skillsOrganize)
    }

    func restore(skill: SkillEntry, agentID: String? = nil) {
        runSkillCommand(skillID: skill.id, agentID: agentID, successMessage: L10n.shared.toastRestored, call: CoreBridge.shared.skillsRestore)
    }

    func resetProvider(_ source: String) {
        Task.detached {
            let payload = try? JSONEncoder().encode(["source": source])
            let resultData = payload.flatMap(CoreBridge.shared.skillsRemoveCustomAgent)

            await MainActor.run {
                if let resultData,
                   let result = try? JSONDecoder().decode(SkillOperationResult.self, from: resultData),
                   result.ok {
                    ProviderRegistry.shared.reload()
                    self.refresh()
                    ToastCenter.shared.success(L10n.shared.toastReset)
                } else {
                    self.errorMessage = "Failed to reset provider"
                    ToastCenter.shared.error(L10n.shared.toastSaveFailed)
                }
            }
        }
    }

    func addCustomAgent(data: Data) {
        Task.detached {
            let resultData = CoreBridge.shared.skillsAddCustomAgent(data)

            await MainActor.run {
                if let resultData,
                   let result = try? JSONDecoder().decode(SkillOperationResult.self, from: resultData),
                   result.ok {
                    ProviderRegistry.shared.reload()
                    self.refresh()
                    ToastCenter.shared.success(L10n.shared.toastSaved)
                } else {
                    self.errorMessage = "Failed to save provider config"
                    ToastCenter.shared.error(L10n.shared.toastSaveFailed)
                }
            }
        }
    }

    func refreshGitStatus(showToast: Bool = false) {
        Task.detached {
            let data = CoreBridge.shared.skillsGitStatus()
            await MainActor.run {
                if let data,
                   let status = try? self.decoder.decode(SkillGitStatus.self, from: data) {
                    self.gitChanges = status.changes
                    self.gitStatusAhead = status.ahead
                    self.gitStatusBehind = status.behind
                    self.gitStatusBranch = status.branch
                    self.gitStatusName = status.status
                    self.gitStatusMessage = status.message
                    if showToast {
                        if status.status == "error" {
                            ToastCenter.shared.error(status.message ?? L10n.shared.skillOperationFailed)
                        } else {
                            ToastCenter.shared.success(L10n.shared.toastRefreshed)
                        }
                    }
                } else if showToast {
                    ToastCenter.shared.error(L10n.shared.skillOperationFailed)
                }
            }
        }
    }

    func refreshGitConnectivity(completion: (() -> Void)? = nil) {
        guard applyStoredGitConfig(showErrorToast: false) else {
            gitConnectivity = nil
            completion?()
            return
        }

        Task.detached {
            let data = CoreBridge.shared.skillsGitConnectivity()
            await MainActor.run {
                self.gitConnectivity = try? self.decoder.decode(SkillGitConnectivity.self, from: data ?? Data())
                completion?()
            }
        }
    }

    func applyStoredGitConfig(showErrorToast: Bool = true) -> Bool {
        let defaults = UserDefaults.standard
        let providerRaw = defaults.string(forKey: "syncProvider") ?? "GitHub"
        let platform: String
        let tokenSaved: Bool

        switch providerRaw {
        case "GitLab":
            platform = "gitlab"
            tokenSaved = defaults.bool(forKey: "syncTokenSaved_gitlab")
        case "Other":
            platform = "other"
            tokenSaved = defaults.bool(forKey: "syncTokenSaved_other")
        default:
            platform = "github"
            tokenSaved = defaults.bool(forKey: "syncTokenSaved_github")
        }

        let remoteURL = defaults.string(forKey: "syncRepoURL") ?? ""
        let token = tokenSaved ? (KeychainManager.shared.getToken(for: platform) ?? "") : ""
        let userName = defaults.string(forKey: "syncGitUserName") ?? ""
        let userEmail = defaults.string(forKey: "syncGitUserEmail") ?? ""
        let gitBranch = defaults.string(forKey: "syncGitBranch") ?? "main"
        return applyGitConfig(
            remoteURL: remoteURL,
            platform: platform,
            token: token,
            gitBranch: gitBranch,
            userName: userName,
            userEmail: userEmail,
            showErrorToast: showErrorToast
        )
    }

    func applyGitConfig(
        remoteURL: String,
        platform: String,
        token: String,
        gitBranch: String = "main",
        userName: String = "",
        userEmail: String = "",
        showErrorToast: Bool = true
    ) -> Bool {
        let trimmedURL = remoteURL.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedToken = token.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedBranch = gitBranch.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedUserName = userName.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedUserEmail = userEmail.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedURL.isEmpty, !trimmedToken.isEmpty else {
            if showErrorToast {
                ToastCenter.shared.error(L10n.shared.gitConfigRequired)
            }
            return false
        }
        let payload = try? JSONEncoder().encode([
            "remote_url": trimmedURL,
            "platform": platform,
            "token": trimmedToken,
            "git_branch": trimmedBranch.isEmpty ? "main" : trimmedBranch,
            "user_name": trimmedUserName,
            "user_email": trimmedUserEmail,
        ])
        guard let payload,
              let resultData = CoreBridge.shared.skillsSetGitConfig(payload),
              let result = try? decoder.decode(SkillOperationResult.self, from: resultData),
              result.ok
        else {
            if showErrorToast {
                ToastCenter.shared.error(L10n.shared.toastSaveFailed)
            }
            return false
        }
        return true
    }

    func pullSkills(
        remoteURL: String? = nil,
        platform: String? = nil,
        token: String? = nil,
        gitBranch: String = "main",
        userName: String? = nil,
        userEmail: String? = nil
    ) {
        Task.detached {
            if let remoteURL, let platform, let token {
                let configured = await MainActor.run {
                    self.applyGitConfig(
                        remoteURL: remoteURL,
                        platform: platform,
                        token: token,
                        gitBranch: gitBranch,
                        userName: userName ?? "",
                        userEmail: userEmail ?? ""
                    )
                }
                guard configured else { return }
            }
            await MainActor.run {
                self.gitStatusName = "pulling"
                self.gitStatusMessage = nil
            }
            let data = CoreBridge.shared.skillsGitPull()
            await MainActor.run {
                self.handleGitSyncResult(data, successMessage: L10n.shared.toastPulled)
            }
        }
    }

    func linkSkill(skillID: String, agentID: String) {
        runSkillCommand(skillID: skillID, agentID: agentID, successMessage: L10n.shared.toastLinked, call: CoreBridge.shared.skillsLink)
    }

    func unlinkSkill(skillID: String, agentID: String) {
        runSkillCommand(skillID: skillID, agentID: agentID, successMessage: L10n.shared.toastUnlinked, call: CoreBridge.shared.skillsUnlink)
    }

    func pushSkills(
        remoteURL: String? = nil,
        platform: String? = nil,
        token: String? = nil,
        gitBranch: String = "main",
        userName: String? = nil,
        userEmail: String? = nil,
        filterPayload: Data? = nil
    ) {
        Task.detached {
            if let remoteURL, let platform, let token {
                let configured = await MainActor.run {
                    self.applyGitConfig(
                        remoteURL: remoteURL,
                        platform: platform,
                        token: token,
                        gitBranch: gitBranch,
                        userName: userName ?? "",
                        userEmail: userEmail ?? ""
                    )
                }
                guard configured else { return }
            }
            await MainActor.run {
                self.gitStatusName = "pushing"
                self.gitStatusMessage = nil
            }
            let data = filterPayload.flatMap(CoreBridge.shared.skillsGitPushFiltered)
                ?? CoreBridge.shared.skillsGitPush()
            await MainActor.run {
                self.handleGitSyncResult(data, successMessage: L10n.shared.toastPushed)
            }
        }
    }

    func organizeFilteredSkills() {
        let targets = filteredSkills.compactMap { skill -> (String, String)? in
            guard !isInSourceRoot(skill), let agentID = sourceAgent(for: skill) else { return nil }
            return (skill.id, agentID)
        }
        runBatchSkillCommand(targets: targets, successMessage: L10n.shared.toastOrganized, call: CoreBridge.shared.skillsOrganize)
    }

    func restoreFilteredSkills() {
        let targets = filteredSkills.compactMap { skill -> (String, String)? in
            guard isInSourceRoot(skill), let agentID = sourceAgent(for: skill) else { return nil }
            return (skill.id, agentID)
        }
        runBatchSkillCommand(targets: targets, successMessage: L10n.shared.toastRestored, call: CoreBridge.shared.skillsRestore)
    }

    private func handleGitSyncResult(_ data: Data?, successMessage: String) {
        if let data,
           let status = try? decoder.decode(SkillGitStatus.self, from: data) {
            gitChanges = status.changes
            gitStatusAhead = status.ahead
            gitStatusBehind = status.behind
            gitStatusBranch = status.branch
            gitStatusName = status.status
            gitStatusMessage = status.message

            if status.status == "error" || status.status == "conflicted" {
                ToastCenter.shared.error(status.message ?? L10n.shared.skillOperationFailed)
            } else {
                refresh()
                refreshGitStatus()
                ToastCenter.shared.success(successMessage)
            }
        } else {
            ToastCenter.shared.error(L10n.shared.skillOperationFailed)
        }
    }

    private func runBatchSkillCommand(targets: [(skillID: String, agentID: String)], successMessage: String, call: @escaping (Data) -> Data?) {
        guard !targets.isEmpty else {
            ToastCenter.shared.error(L10n.shared.skillNoBatchTargets)
            return
        }

        Task.detached {
            var successCount = 0
            var firstError: String?

            for target in targets {
                let payload = try? JSONEncoder().encode([
                    "skill_id": target.skillID,
                    "agent_id": target.agentID,
                ])
                guard let payload, let resultData = call(payload) else {
                    firstError = firstError ?? L10n.shared.skillOperationFailed
                    continue
                }
                guard let result = try? JSONDecoder().decode(SkillOperationResult.self, from: resultData) else {
                    firstError = firstError ?? L10n.shared.skillOperationFailed
                    continue
                }
                guard result.ok else {
                    firstError = firstError ?? result.error ?? L10n.shared.skillOperationFailed
                    continue
                }
                successCount += 1
            }

            let finalSuccessCount = successCount
            let finalFirstError = firstError
            await MainActor.run {
                if finalSuccessCount > 0 {
                    self.refresh()
                    ToastCenter.shared.success(successMessage)
                } else {
                    self.errorMessage = finalFirstError ?? L10n.shared.skillOperationFailed
                    ToastCenter.shared.error(self.errorMessage ?? L10n.shared.skillOperationFailed)
                }
            }
        }
    }

    private func runSkillCommand(skillID: String, agentID: String? = nil, successMessage: String, call: @escaping (Data) -> Data?) {
        Task.detached {
            var dict: [String: String] = ["skill_id": skillID]
            if let agentID { dict["agent_id"] = agentID }
            let payload = try? JSONEncoder().encode(dict)
            let resultData = payload.flatMap(call)

            await MainActor.run {
                if let resultData,
                   let result = try? JSONDecoder().decode(SkillOperationResult.self, from: resultData),
                   result.ok {
                    self.refresh()
                    ToastCenter.shared.success(successMessage)
                } else {
                    let result = resultData.flatMap { try? JSONDecoder().decode(SkillOperationResult.self, from: $0) }
                    self.errorMessage = result?.error ?? L10n.shared.skillOperationFailed
                    ToastCenter.shared.error(self.errorMessage ?? L10n.shared.skillOperationFailed)
                }
            }
        }
    }
}

enum SkillManagerError: LocalizedError {
    case emptyResponse

    var errorDescription: String? {
        switch self {
        case .emptyResponse:
            return "Empty response from Skill Manager core"
        }
    }
}
