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
        return providers.filter { enabled.contains($0.source) }
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

    private let decoder = JSONDecoder()

    private init() {
        decoder.keyDecodingStrategy = .convertFromSnakeCase
    }

    func refresh() {
        isLoading = true
        errorMessage = nil

        Task.detached {
            let skillsData = CoreBridge.shared.skillsList()
            let agentsData = CoreBridge.shared.skillsListAgents()

            await MainActor.run {
                self.isLoading = false
                self.skills = (try? self.decode([SkillEntry].self, from: skillsData)) ?? []
                self.providers = (try? self.decode([SkillProvider].self, from: agentsData)) ?? []
            }
        }
    }

    var filteredSkills: [SkillEntry] {
        skills.filter { skill in
            let matchesSearch = searchText.isEmpty
                || skill.manifest.name.localizedCaseInsensitiveContains(searchText)
                || skill.manifest.description.localizedCaseInsensitiveContains(searchText)

            guard selectedFilter != "all" else { return matchesSearch }
            // Match agent filter against compatible_agents (normalized canonical names)
            let compat = skill.manifest.compatibleAgents
            return matchesSearch && (compat.contains(selectedFilter) || compat.contains(canonicalAgentName(for: selectedFilter)))
        }
    }

    /// Map a canonical source name to possible skill-manifest agent names.
    private func canonicalAgentName(for source: String) -> String {
        source == "claude" ? "claude-code" : source
    }

    /// Check if a skill is linked (via symlink) to a given agent.
    func isSkillLinked(skillID: String, agentID: String) -> Bool {
        providers.first(where: { $0.source == agentID })?.linkedSkills.contains(skillID) == true
    }

    /// Get the source agent for a skill (the first agent that claims it as linked).
    func sourceAgent(for skillID: String) -> String? {
        providers.first(where: { $0.linkedSkills.contains(skillID) })?.source
    }

    private func decode<T: Decodable>(_ type: T.Type, from data: Data?) throws -> T {
        guard let data else {
            throw SkillManagerError.emptyResponse
        }
        return try decoder.decode(type, from: data)
    }

    func delete(skill: SkillEntry) {
        runSkillCommand(skillID: skill.id, call: CoreBridge.shared.skillsDelete)
    }

    func organize(skill: SkillEntry, agentID: String? = nil) {
        runSkillCommand(skillID: skill.id, agentID: agentID, call: CoreBridge.shared.skillsOrganize)
    }

    func restore(skill: SkillEntry, agentID: String? = nil) {
        runSkillCommand(skillID: skill.id, agentID: agentID, call: CoreBridge.shared.skillsRestore)
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
                } else {
                    self.errorMessage = "Failed to reset provider"
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
                } else {
                    self.errorMessage = "Failed to save provider config"
                }
            }
        }
    }

    func refreshGitStatus() {
        Task.detached {
            let data = CoreBridge.shared.skillsGitStatus()
            await MainActor.run {
                if let data,
                   let status = try? self.decoder.decode(SkillGitStatus.self, from: data) {
                    self.gitChanges = status.changes
                    self.gitStatusAhead = status.ahead
                    self.gitStatusBehind = status.behind
                    self.gitStatusBranch = status.branch
                }
            }
        }
    }

    func pullSkills() {
        Task.detached {
            _ = CoreBridge.shared.skillsGitPull()
            await MainActor.run {
                self.refreshGitStatus()
            }
        }
    }

    func linkSkill(skillID: String, agentID: String) {
        runSkillCommand(skillID: skillID, agentID: agentID, call: CoreBridge.shared.skillsLink)
    }

    func unlinkSkill(skillID: String, agentID: String) {
        runSkillCommand(skillID: skillID, agentID: agentID, call: CoreBridge.shared.skillsUnlink)
    }

    func pushSkills() {
        Task.detached {
            _ = CoreBridge.shared.skillsGitPush()
            await MainActor.run {
                self.refreshGitStatus()
            }
        }
    }

    private func runSkillCommand(skillID: String, agentID: String? = nil, call: @escaping (Data) -> Data?) {
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
                } else {
                    self.errorMessage = "Skill operation failed"
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
