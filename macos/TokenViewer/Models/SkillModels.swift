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
    let agentIds: [String]

    enum CodingKeys: String, CodingKey {
        case id
        case manifest
        case sourceDir
        case installedAt
        case agentIds
    }

    init(
        id: String,
        manifest: SkillManifest,
        sourceDir: String,
        installedAt: String,
        agentIds: [String] = []
    ) {
        self.id = id
        self.manifest = manifest
        self.sourceDir = sourceDir
        self.installedAt = installedAt
        self.agentIds = agentIds
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        manifest = try container.decode(SkillManifest.self, forKey: .manifest)
        sourceDir = try container.decode(String.self, forKey: .sourceDir)
        installedAt = try container.decode(String.self, forKey: .installedAt)
        agentIds = try container.decodeIfPresent([String].self, forKey: .agentIds) ?? []
    }
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
    let detectCmd: String?
    var isInstalled: Bool = false
    let brandColor: String
    let logoFile: String

    enum CodingKeys: String, CodingKey {
        case source
        case displayName
        case skillsPath
        case linkType
        case isLinked
        case linkedSkills
        case hasParser
        case hasLimits
        case detectCmd
        case isInstalled
        case brandColor
        case logoFile
    }

    init(
        source: String,
        displayName: String,
        skillsPath: String,
        linkType: String,
        isLinked: Bool,
        linkedSkills: [String],
        hasParser: Bool,
        hasLimits: Bool,
        detectCmd: String?,
        isInstalled: Bool = false,
        brandColor: String = "#059669",
        logoFile: String = ""
    ) {
        self.source = source
        self.displayName = displayName
        self.skillsPath = skillsPath
        self.linkType = linkType
        self.isLinked = isLinked
        self.linkedSkills = linkedSkills
        self.hasParser = hasParser
        self.hasLimits = hasLimits
        self.detectCmd = detectCmd
        self.isInstalled = isInstalled
        self.brandColor = brandColor
        self.logoFile = logoFile
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        source = try container.decode(String.self, forKey: .source)
        displayName = try container.decode(String.self, forKey: .displayName)
        skillsPath = try container.decode(String.self, forKey: .skillsPath)
        linkType = try container.decode(String.self, forKey: .linkType)
        isLinked = try container.decode(Bool.self, forKey: .isLinked)
        linkedSkills = try container.decode([String].self, forKey: .linkedSkills)
        hasParser = try container.decode(Bool.self, forKey: .hasParser)
        hasLimits = try container.decode(Bool.self, forKey: .hasLimits)
        detectCmd = try container.decodeIfPresent(String.self, forKey: .detectCmd)
        isInstalled = try container.decodeIfPresent(Bool.self, forKey: .isInstalled) ?? false
        brandColor = try container.decodeIfPresent(String.self, forKey: .brandColor) ?? "#059669"
        logoFile = try container.decodeIfPresent(String.self, forKey: .logoFile) ?? ""
    }
}

struct SkillOperationResult: Codable {
    let ok: Bool
    let error: String?
}

struct SkillGitStatus: Codable {
    let status: String?
    let message: String?
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
