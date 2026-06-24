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

    func skillsWatchStart() -> Data? {
        callSkills { tt_skills_watch_start($0) }
    }

    func skillsWatchStop() {
        _ = callSkills { ptr in
            tt_skills_watch_stop(ptr)
            return nil
        }
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

    private func callSkills(_ body: @escaping (OpaquePointer) -> UnsafeMutablePointer<CChar>?) -> Data? {
        call(body)
    }

    private func callSkillsWithJSON(_ payload: Data, _ body: @escaping (OpaquePointer, UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?) -> Data? {
        call { handle in
            payload.withUnsafeBytes { bytes in
                guard let ptr = bytes.baseAddress?.assumingMemoryBound(to: CChar.self) else { return nil }
                return body(handle, ptr)
            }
        }
    }
}
