import Foundation

struct SkillManifest: Codable, Hashable {
    let name: String
    let description: String
    let tags: [String]
    let compatibleAgents: [String]
    let version: String
}

struct SkillEntry: Codable, Identifiable, Hashable {
    let id: String
    let manifest: SkillManifest
    let sourceDir: String
    let installedAt: String
}

struct SkillProvider: Codable, Identifiable, Hashable {
    let source: String
    var id: String { source }
    let displayName: String
    let skillsPath: String
    let linkType: String
    let isLinked: Bool
    let linkedSkills: [String]
    let hasParser: Bool
    let hasLimits: Bool
}

struct SkillOperationResult: Codable {
    let ok: Bool
    let error: String?
}

struct SkillGitStatus: Codable {
    let branch: String?
    let ahead: Int
    let behind: Int
    let hasChanges: Bool
    let changes: [SkillGitChange]
}

struct SkillGitChange: Codable {
    let filePath: String
    let changeType: String
}

struct SkillGitConnectivity: Codable {
    let status: String
    let message: String?
}
