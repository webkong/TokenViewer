import Foundation

@MainActor
final class SkillManagerViewModel: ObservableObject {
    static let shared = SkillManagerViewModel()

    @Published private(set) var skills: [SkillEntry] = []
    @Published private(set) var agents: [SkillAgent] = []
    @Published var selectedFilter: String = "all"
    @Published var searchText: String = ""
    @Published var isLoading = false
    @Published var errorMessage: String?
    @Published var gitChanges: [SkillGitChange] = []

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
                self.agents = (try? self.decode([SkillAgent].self, from: agentsData)) ?? []
            }
        }
    }

    var filteredSkills: [SkillEntry] {
        skills.filter { skill in
            let matchesSearch = searchText.isEmpty
                || skill.manifest.name.localizedCaseInsensitiveContains(searchText)
                || skill.manifest.description.localizedCaseInsensitiveContains(searchText)

            guard selectedFilter != "all" else { return matchesSearch }
            return matchesSearch && skill.manifest.compatibleAgents.contains(selectedFilter)
        }
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

    func organize(skill: SkillEntry) {
        runSkillCommand(skillID: skill.id, call: CoreBridge.shared.skillsOrganize)
    }

    func restore(skill: SkillEntry) {
        runSkillCommand(skillID: skill.id, call: CoreBridge.shared.skillsRestore)
    }

    func removeCustomAgent(_ agentID: String) {
        Task.detached {
            let payload = try? JSONEncoder().encode(["agent_id": agentID])
            let resultData = payload.flatMap(CoreBridge.shared.skillsRemoveCustomAgent)

            await MainActor.run {
                if let resultData,
                   let result = try? JSONDecoder().decode(SkillOperationResult.self, from: resultData),
                   result.ok {
                    self.refresh()
                } else {
                    self.errorMessage = "Failed to remove agent"
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
                    self.errorMessage = "Failed to add agent"
                }
            }
        }
    }

    func refreshGitStatus() {
        Task.detached {
            let data = CoreBridge.shared.skillsGitStatus()
            await MainActor.run {
                if let data,
                   let status = try? JSONDecoder().decode(SkillGitStatus.self, from: data) {
                    self.gitChanges = status.changes
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

    func pushSkills() {
        Task.detached {
            _ = CoreBridge.shared.skillsGitPush()
            await MainActor.run {
                self.refreshGitStatus()
            }
        }
    }

    private func runSkillCommand(skillID: String, call: @escaping (Data) -> Data?) {
        Task.detached {
            let payload = try? JSONEncoder().encode(["skill_id": skillID])
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
