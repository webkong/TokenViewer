import Foundation

enum DeviceSyncComponent: String, Codable, CaseIterable, Hashable, Sendable {
    case skills
    case agentLinks = "agent_links"
    case skillEnv = "skill_env"
    case preferences
}

enum DeviceSyncScopeMode: String, Codable, Hashable, Sendable {
    case all
    case selected
}

struct DeviceSyncSkillScope: Codable, Equatable, Sendable {
    var mode: DeviceSyncScopeMode = .selected
    var skillIds: [String] = []
}

struct DeviceSyncEnvironmentScope: Codable, Equatable, Sendable {
    var mode: DeviceSyncScopeMode = .selected
    var names: [String] = []
}

struct DeviceSyncProviderConfig: Codable, Equatable, Sendable {
    var kind: String
    var endpoint: String?
    var remotePrefix: String
    var localRoot: String?
    var username: String?
    var bucket: String?
    var region: String?
    var pathStyle: Bool?
    var insecure: Bool

    init(
        kind: String,
        endpoint: String? = nil,
        remotePrefix: String = "",
        localRoot: String? = nil,
        username: String? = nil,
        bucket: String? = nil,
        region: String? = nil,
        pathStyle: Bool? = nil,
        insecure: Bool = false
    ) {
        self.kind = kind
        self.endpoint = endpoint
        self.remotePrefix = remotePrefix
        self.localRoot = localRoot
        self.username = username
        self.bucket = bucket
        self.region = region
        self.pathStyle = pathStyle
        self.insecure = insecure
    }
}

struct DeviceSyncConfig: Codable, Equatable, Sendable {
    var schemaVersion: UInt32 = 1
    var enabled: Bool = false
    var profileId: String
    var vaultId: String?
    var provider: DeviceSyncProviderConfig?
    var contentSource: String = "cloud"
    var components: [DeviceSyncComponent] = [.agentLinks, .preferences]
    var skillScope: DeviceSyncSkillScope = .init()
    var environmentScope: DeviceSyncEnvironmentScope = .init()
    var autoSync: Bool = false

    init(
        profileId: String,
        enabled: Bool = false,
        vaultId: String? = nil,
        provider: DeviceSyncProviderConfig? = nil,
        contentSource: String = "cloud",
        components: [DeviceSyncComponent] = [.agentLinks, .preferences],
        skillScope: DeviceSyncSkillScope = .init(),
        environmentScope: DeviceSyncEnvironmentScope = .init(),
        autoSync: Bool = false
    ) {
        self.profileId = profileId
        self.enabled = enabled
        self.vaultId = vaultId
        self.provider = provider
        self.contentSource = contentSource
        self.components = components
        self.skillScope = skillScope
        self.environmentScope = environmentScope
        self.autoSync = autoSync
    }
}

struct DeviceSyncIdentity: Codable, Equatable, Sendable {
    let schemaVersion: UInt32
    let deviceId: String
    let displayName: String
    let createdAt: String
}

struct DeviceSyncSyncResultSummary: Codable, Equatable, Sendable {
    let direction: String
    let snapshotId: String?
    let completedAt: String
    let messageKey: String?
}

struct DeviceSyncGitRepositoryHint: Codable, Equatable, Sendable {
    let provider: String?
    let branch: String
    let remoteUrl: String?
    let commitOid: String?
}

struct DeviceSyncPendingContentSource: Codable, Equatable, Sendable {
    let contentSource: String
    let repository: DeviceSyncGitRepositoryHint?
    let skillIds: [String]
}

struct DeviceSyncState: Codable, Equatable, Sendable {
    let schemaVersion: UInt32
    let appliedSnapshotId: String?
    let localSequence: UInt64
    let maxRemoteSequences: [String: UInt64]
    let seenSnapshots: Set<String>
    let lastResult: DeviceSyncSyncResultSummary?
    let pendingContentSource: DeviceSyncPendingContentSource?
}

struct DeviceSyncStatus: Codable, Equatable, Sendable {
    private enum CodingKeys: String, CodingKey {
        case config
        case identity
        case state
        case remoteHeadFingerprint
        case frontier
        case recoveryBlocked
    }

    let config: DeviceSyncConfig
    let identity: DeviceSyncIdentity
    let state: DeviceSyncState
    let remoteHeadFingerprint: String?
    let frontier: [String]
    let recoveryBlocked: Bool

    init(
        config: DeviceSyncConfig,
        identity: DeviceSyncIdentity,
        state: DeviceSyncState,
        remoteHeadFingerprint: String?,
        frontier: [String],
        recoveryBlocked: Bool = false
    ) {
        self.config = config
        self.identity = identity
        self.state = state
        self.remoteHeadFingerprint = remoteHeadFingerprint
        self.frontier = frontier
        self.recoveryBlocked = recoveryBlocked
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        config = try container.decode(DeviceSyncConfig.self, forKey: .config)
        identity = try container.decode(DeviceSyncIdentity.self, forKey: .identity)
        state = try container.decode(DeviceSyncState.self, forKey: .state)
        remoteHeadFingerprint = try container.decodeIfPresent(
            String.self,
            forKey: .remoteHeadFingerprint
        )
        frontier = try container.decode([String].self, forKey: .frontier)
        recoveryBlocked = try container.decodeIfPresent(Bool.self, forKey: .recoveryBlocked) ?? false
    }
}

struct DeviceSyncErrorPayload: Codable, Equatable, Error, Sendable {
    let code: String
    let messageKey: String
    let arguments: [String: String]
    let retryable: Bool
    let operationId: String?
}

struct DeviceSyncEnvelope<Payload: Codable>: Codable {
    let ok: Bool
    let data: Payload?
    let error: DeviceSyncErrorPayload?
}

struct DeviceSyncPreviewRequest: Encodable {
    let enabledAgentIds: [String]
}

struct DeviceSyncTokenRequest: Encodable {
    let previewToken: String
    let enabledAgentIds: [String]
}

struct DeviceSyncTransactionRequest: Encodable {
    let transactionId: String
}

struct DeviceSyncPasswordRequest: Encodable {
    let password: String
}

struct DeviceSyncMasterKeyRequest: Encodable {
    let masterKeyB64: String
}

struct DeviceSyncVaultSession: Codable, Equatable, Sendable {
    let status: DeviceSyncStatus
    let masterKeyB64: String
}

struct DeviceSyncRestoreResponse: Codable, Equatable, Sendable {
    let restored: Bool
}

struct DeviceSyncConnectionReport: Codable, Equatable, Sendable {
    let connected: Bool
}

struct DeviceSyncChangeCounts: Codable, Equatable, Sendable {
    let added: UInt64
    let updated: UInt64
    let deleted: UInt64
    let conflicts: UInt64
    let skipped: UInt64
}

struct DeviceSyncPreviewSummary: Codable, Equatable, Sendable {
    let skills: DeviceSyncChangeCounts
    let agentLinks: DeviceSyncChangeCounts
    let skillEnv: DeviceSyncChangeCounts
    let preferences: DeviceSyncChangeCounts
    let warnings: UInt64
}

struct DeviceSyncPreviewItem: Codable, Equatable, Sendable, Identifiable {
    let component: String
    let action: String
    let id: String
    let detail: String?
    let destructive: Bool
}

extension DeviceSyncPreviewItem {
    var stableId: String { "\(component):\(action):\(id)" }
}

struct DeviceSyncPreviewResponse: Codable, Equatable, Sendable {
    let previewToken: String
    let direction: String
    let expiresAt: String
    let remoteHeadFingerprint: String
    let localFingerprint: String
    let summary: DeviceSyncPreviewSummary
    let items: [DeviceSyncPreviewItem]

    var id: String { previewToken }
}

struct DeviceSyncResult: Codable, Equatable, Sendable {
    let direction: String
    let snapshotId: String?
    let operationId: String
    let warnings: [String]
}

struct DeviceSyncPreferenceMutation: Codable, Equatable, Sendable {
    let key: String
    let enabledAgentIds: [String]
}

struct DeviceSyncPrepareApplyResponse: Codable, Equatable, Sendable {
    let transactionId: String
    let preferenceMutations: [DeviceSyncPreferenceMutation]
    let recoveryPath: String?
}

struct DeviceSyncRollbackResponse: Codable, Equatable, Sendable {
    let rolledBack: Bool
}

struct DeviceSyncFinalizeResponse: Codable, Equatable, Sendable {
    let finalized: Bool
}

struct DeviceSyncRecoveryResponse: Codable, Equatable, Sendable {
    let recovered: UInt64
    let rolledBackTransactionIds: [String]
    let committedTransactionIds: [String]

    init(
        recovered: UInt64,
        rolledBackTransactionIds: [String] = [],
        committedTransactionIds: [String] = []
    ) {
        self.recovered = recovered
        self.rolledBackTransactionIds = rolledBackTransactionIds
        self.committedTransactionIds = committedTransactionIds
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        recovered = try container.decode(UInt64.self, forKey: .recovered)
        rolledBackTransactionIds = try container.decodeIfPresent(
            [String].self,
            forKey: .rolledBackTransactionIds
        ) ?? []
        committedTransactionIds = try container.decodeIfPresent(
            [String].self,
            forKey: .committedTransactionIds
        ) ?? []
    }
}

struct DeviceSyncSnapshotListItem: Codable, Equatable, Sendable, Identifiable {
    let snapshotId: String
    let deviceId: String
    let parentIds: [String]
    let createdAt: String
    let updatedAt: String
    let isFrontier: Bool

    var id: String { snapshotId }
}
