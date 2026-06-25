import Foundation

@MainActor
final class SkillManagerViewModel: ObservableObject {
    static let shared = SkillManagerViewModel()

    @Published private(set) var skills: [SkillEntry] = []
    @Published private(set) var providers: [SkillProvider] = []
    @Published var selectedFilter: String = "all"

    /// Providers enabled in Settings (defaults to claude, codex, opencode).
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
        else { return ["claude", "codex", "opencode"] }
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

    private let decoder = JSONDecoder()

    private init() {
        decoder.keyDecodingStrategy = .convertFromSnakeCase
    }

    func refresh(showToast: Bool = false) {
        isLoading = true
        errorMessage = nil
        let enabled = enabledProviderSet

        Task.detached {
            let agentsData = CoreBridge.shared.skillsListAgents()
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            let providers = (try? decoder.decode([SkillProvider].self, from: agentsData ?? Data())) ?? []
            let agentIDs = providers
                .map(\.source)
                .filter { enabled.contains($0) }
            let payload = try? JSONEncoder().encode(["agent_ids": agentIDs])
            let skillsData = payload.flatMap(CoreBridge.shared.skillsListForAgents)
                ?? CoreBridge.shared.skillsList()
            let skills = (try? decoder.decode([SkillEntry].self, from: skillsData ?? Data())) ?? []

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

    var filteredSkills: [SkillEntry] {
        skills.filter { skill in
            let matchesSearch = searchText.isEmpty
                || skill.manifest.name.localizedCaseInsensitiveContains(searchText)
                || skill.manifest.description.localizedCaseInsensitiveContains(searchText)

            guard selectedFilter != "all" else { return matchesSearch }
            return matchesSearch && skillMatchesAgent(skill, agentID: selectedFilter)
        }
    }

    func ensureValidFilter() {
        guard selectedFilter != "all" else { return }
        if !visibleProviders.contains(where: { $0.source == selectedFilter }) {
            selectedFilter = "all"
        }
    }

    private func skillMatchesAgent(_ skill: SkillEntry, agentID: String) -> Bool {
        if sourceAgent(for: skill) == agentID {
            return true
        }
        return isSkillLinked(skillID: skill.id, agentID: agentID)
    }

    /// Check if a skill is linked (via symlink) to a given agent.
    func isSkillLinked(skillID: String, agentID: String) -> Bool {
        providers.first(where: { $0.source == agentID })?.linkedSkills.contains(skillID) == true
    }

    /// Get the source agent for a skill (the first agent that claims it as linked).
    func sourceAgent(for skill: SkillEntry) -> String? {
        if let linked = providers.first(where: { $0.linkedSkills.contains(skill.id) })?.source {
            return linked
        }
        let sourceDir = standardizedPath(skill.sourceDir)
        return providers.first { provider in
            sourceDir.hasPrefix(standardizedPath(provider.skillsPath) + "/")
        }?.source
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

    private func standardizedPath(_ path: String) -> String {
        (NSString(string: path).expandingTildeInPath as NSString).standardizingPath
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
        runSkillCommand(skillID: skill.id, agentID: agentID, successMessage: L10n.shared.toastOrganized, call: CoreBridge.shared.skillsOrganize)
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

    func refreshGitConnectivity() {
        Task.detached {
            let data = CoreBridge.shared.skillsGitConnectivity()
            await MainActor.run {
                self.gitConnectivity = try? self.decoder.decode(SkillGitConnectivity.self, from: data ?? Data())
            }
        }
    }

    func applyGitConfig(remoteURL: String, platform: String, token: String) -> Bool {
        let trimmedURL = remoteURL.trimmingCharacters(in: .whitespacesAndNewlines)
        let trimmedToken = token.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedURL.isEmpty, !trimmedToken.isEmpty else {
            ToastCenter.shared.error(L10n.shared.gitConfigRequired)
            return false
        }
        let payload = try? JSONEncoder().encode([
            "remote_url": trimmedURL,
            "platform": platform,
            "token": trimmedToken,
        ])
        guard let payload,
              let resultData = CoreBridge.shared.skillsSetGitConfig(payload),
              let result = try? decoder.decode(SkillOperationResult.self, from: resultData),
              result.ok
        else {
            ToastCenter.shared.error(L10n.shared.toastSaveFailed)
            return false
        }
        return true
    }

    func pullSkills(remoteURL: String? = nil, platform: String? = nil, token: String? = nil) {
        Task.detached {
            if let remoteURL, let platform, let token {
                let configured = await MainActor.run {
                    self.applyGitConfig(remoteURL: remoteURL, platform: platform, token: token)
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

    func pushSkills(remoteURL: String? = nil, platform: String? = nil, token: String? = nil) {
        Task.detached {
            if let remoteURL, let platform, let token {
                let configured = await MainActor.run {
                    self.applyGitConfig(remoteURL: remoteURL, platform: platform, token: token)
                }
                guard configured else { return }
            }
            await MainActor.run {
                self.gitStatusName = "pushing"
                self.gitStatusMessage = nil
            }
            let data = CoreBridge.shared.skillsGitPush()
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

            if status.status == "error" {
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
                guard let payload,
                      let resultData = call(payload),
                      let result = try? JSONDecoder().decode(SkillOperationResult.self, from: resultData),
                      result.ok
                else {
                    firstError = firstError ?? L10n.shared.skillOperationFailed
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
                    self.errorMessage = "Skill operation failed"
                    ToastCenter.shared.error(L10n.shared.skillOperationFailed)
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
