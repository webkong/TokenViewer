import Foundation

struct SkillManifest: Codable, Hashable {
    let name: String
    let description: String
    let tags: [String]
    let compatibleAgents: [String]
    let version: String
    /// false (default) when the manifest was synthesized because no
    /// manifest.json existed; true when loaded from a user-authored file.
    let hasManifest: Bool

    enum CodingKeys: String, CodingKey {
        case name
        case description
        case tags
        case compatibleAgents
        case version
        case hasManifest
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        description = try c.decode(String.self, forKey: .description)
        tags = try c.decodeIfPresent([String].self, forKey: .tags) ?? []
        compatibleAgents = try c.decodeIfPresent([String].self, forKey: .compatibleAgents) ?? []
        version = try c.decodeIfPresent(String.self, forKey: .version) ?? "unknown"
        hasManifest = try c.decodeIfPresent(Bool.self, forKey: .hasManifest) ?? false
    }
}

struct SkillEntry: Codable, Identifiable, Hashable {
    let id: String
    let manifest: SkillManifest
    let sourceDir: String
    let installedAt: String
    let agentIds: [String]
    let isBuiltIn: Bool

    enum CodingKeys: String, CodingKey {
        case id
        case manifest
        case sourceDir
        case installedAt
        case agentIds
        case isBuiltIn
    }

    init(
        id: String,
        manifest: SkillManifest,
        sourceDir: String,
        installedAt: String,
        agentIds: [String] = [],
        isBuiltIn: Bool = false
    ) {
        self.id = id
        self.manifest = manifest
        self.sourceDir = sourceDir
        self.installedAt = installedAt
        self.agentIds = agentIds
        self.isBuiltIn = isBuiltIn
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        manifest = try container.decode(SkillManifest.self, forKey: .manifest)
        sourceDir = try container.decode(String.self, forKey: .sourceDir)
        installedAt = try container.decode(String.self, forKey: .installedAt)
        agentIds = try container.decodeIfPresent([String].self, forKey: .agentIds) ?? []
        isBuiltIn = try container.decodeIfPresent(Bool.self, forKey: .isBuiltIn) ?? false
    }
}

struct SkillMarkdownPreview: Identifiable, Hashable {
    let id = UUID()
    let skill: SkillEntry
    let filePath: String
    let content: String
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
