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

struct SkillAgent: Codable, Identifiable, Hashable {
    let id: String
    let name: String
    let skillsPath: String
    let linkType: String
    let isBuiltin: Bool
    let isLinked: Bool
    let linkedSkills: [String]
    let icon: String?
    let exists: Bool
}

enum SkillLinkType: String {
    case directory = "Directory"
    case singleFile = "SingleFile"
    case overlay = "Overlay"
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
