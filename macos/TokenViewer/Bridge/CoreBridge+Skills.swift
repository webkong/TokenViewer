import Foundation

extension CoreBridge {
    func skillsList() -> Data? {
        callSkills { tt_skills_list($0) }
    }

    func skillsListAgents() -> Data? {
        callSkills { tt_skills_list_agents($0) }
    }

    func skillsGitStatus() -> Data? {
        callSkills { tt_skills_git_status($0) }
    }

    func skillsGitPull() -> Data? {
        callSkills { tt_skills_git_pull($0) }
    }

    func skillsGitPush() -> Data? {
        callSkills { tt_skills_git_push($0) }
    }

    func skillsGitConnectivity() -> Data? {
        callSkills { tt_skills_git_connectivity($0) }
    }

    func skillsGetConfig() -> Data? {
        callSkills { tt_skills_get_config($0) }
    }

    func skillsSetGitConfig(_ payload: Data) -> Data? {
        callSkillsWithJSON(payload) { tt_skills_set_git_config($0, $1) }
    }

    func skillsAddCustomAgent(_ payload: Data) -> Data? {
        callSkillsWithJSON(payload) { tt_skills_add_custom_agent($0, $1) }
    }

    func skillsRemoveCustomAgent(_ payload: Data) -> Data? {
        callSkillsWithJSON(payload) { tt_skills_remove_custom_agent($0, $1) }
    }

    func skillsOrganize(_ payload: Data) -> Data? {
        callSkillsWithJSON(payload) { tt_skills_organize($0, $1) }
    }

    func skillsDelete(_ payload: Data) -> Data? {
        callSkillsWithJSON(payload) { tt_skills_delete($0, $1) }
    }

    func skillsRestore(_ payload: Data) -> Data? {
        callSkillsWithJSON(payload) { tt_skills_restore($0, $1) }
    }

    func skillsLink(_ payload: Data) -> Data? {
        callSkillsWithJSON(payload) { tt_skills_link($0, $1) }
    }

    func skillsUnlink(_ payload: Data) -> Data? {
        callSkillsWithJSON(payload) { tt_skills_unlink($0, $1) }
    }

    private func callSkills(_ body: @escaping (OpaquePointer) -> UnsafeMutablePointer<CChar>?) -> Data? {
        call(body)
    }

    private func callSkillsWithJSON(_ payload: Data, _ body: @escaping (OpaquePointer, UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?) -> Data? {
        guard let jsonStr = String(data: payload, encoding: .utf8) else { return nil }
        return call { handle in
            jsonStr.withCString { ptr in body(handle, ptr) }
        }
    }
}
