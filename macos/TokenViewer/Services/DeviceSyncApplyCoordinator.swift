import Combine
import Foundation

#if canImport(Darwin)
import Darwin
#endif

protocol DeviceSyncApplyCore: AnyObject, Sendable {
    func deviceSyncRawPrepareApply(previewToken: String) async throws -> DeviceSyncPrepareApplyResponse
    func deviceSyncRawCommitApply(transactionId: String) async throws -> DeviceSyncResult
    func deviceSyncRawRollbackApply(transactionId: String) async throws -> DeviceSyncRollbackResponse
    func deviceSyncRawFinalizeApply(transactionId: String) async throws -> DeviceSyncFinalizeResponse
    func deviceSyncRawRecoverPendingApply() async throws -> DeviceSyncRecoveryResponse
    func deviceSyncRawRestoreMasterKeyIfAvailable() async throws -> Bool
}

extension CoreBridge: DeviceSyncApplyCore {}

protocol DeviceSyncPreferenceStore: AnyObject, Sendable {
    func value(forKey key: String) throws -> String?
    func setValue(_ value: String?, forKey key: String) throws
}

final class UserDefaultsDeviceSyncPreferenceStore: DeviceSyncPreferenceStore, @unchecked Sendable {
    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    func value(forKey key: String) throws -> String? {
        guard let value = defaults.object(forKey: key) else { return nil }
        guard let value = value as? String else {
            throw DeviceSyncPreferenceStoreError.nonStringValue
        }
        return value
    }

    func setValue(_ value: String?, forKey key: String) throws {
        if let value {
            defaults.set(value, forKey: key)
        } else {
            defaults.removeObject(forKey: key)
        }
    }
}

enum DeviceSyncPreferenceStoreError: Error, Sendable {
    case nonStringValue
}

struct DeviceSyncApplyError: LocalizedError, Equatable, Sendable {
    let code: String
    let messageKey: String
    let operationId: String?
    let recoveryPath: String?
    let stage: String

    var errorDescription: String? {
        L10n.shared.deviceSyncErrorMessage(code)
    }
}

enum DeviceSyncApplyState: Equatable, Sendable {
    case idle
    case applying(operationId: String?)
    case recoveryBlocked(operationId: String?, recoveryPath: String?, code: String)
    case failed(operationId: String?, recoveryPath: String?, code: String)
}

enum DeviceSyncRecoveryState: Equatable, Sendable {
    case ready
    case recovering
    case blocked(operationId: String?, recoveryPath: String?, code: String)
}

struct DeviceSyncSessionToken: Equatable, Sendable {
    let generation: UInt64
    let profileId: String?
    let operationId: String
}

@MainActor
final class DeviceSyncSessionGate: ObservableObject {
    @Published private(set) var recoveryState: DeviceSyncRecoveryState = .ready

    private(set) var generation: UInt64 = 0
    private(set) var profileId: String?
    private var activeOperationId: String?
    private var blockedDetails: (operationId: String?, recoveryPath: String?, code: String)?

    var isRecoveryBlocked: Bool {
        if case .blocked = recoveryState { return true }
        return false
    }

    var isOperationInProgress: Bool { activeOperationId != nil }

    func updateProfile(_ profileId: String?) {
        guard self.profileId != profileId else { return }
        self.profileId = profileId
        generation &+= 1
        if recoveryState == .recovering {
            recoveryState = .ready
        }
    }

    func beginOperation() throws -> DeviceSyncSessionToken {
        if isRecoveryBlocked {
            throw gateFailure(
                code: "recovery_blocked",
                operationId: blockedDetails?.operationId,
                recoveryPath: blockedDetails?.recoveryPath,
                stage: "gate"
            )
        }
        guard activeOperationId == nil else {
            throw gateFailure(
                code: "operation_in_progress",
                operationId: activeOperationId,
                recoveryPath: nil,
                stage: "gate"
            )
        }
        let operationId = UUID().uuidString
        activeOperationId = operationId
        return DeviceSyncSessionToken(
            generation: generation,
            profileId: profileId,
            operationId: operationId
        )
    }

    func beginRecovery() throws -> DeviceSyncSessionToken {
        guard activeOperationId == nil else {
            throw gateFailure(
                code: "operation_in_progress",
                operationId: activeOperationId,
                recoveryPath: nil,
                stage: "recovery"
            )
        }
        generation &+= 1
        let operationId = UUID().uuidString
        activeOperationId = operationId
        recoveryState = .recovering
        return DeviceSyncSessionToken(
            generation: generation,
            profileId: profileId,
            operationId: operationId
        )
    }

    func endOperation(_ token: DeviceSyncSessionToken) {
        guard activeOperationId == token.operationId else { return }
        activeOperationId = nil
    }

    func isCurrent(_ token: DeviceSyncSessionToken) -> Bool {
        generation == token.generation && profileId == token.profileId
    }

    func markRecoveryBlocked(
        operationId: String?,
        recoveryPath: String?,
        code: String
    ) {
        if case let .blocked(currentOperationId, currentRecoveryPath, currentCode) = recoveryState,
           currentOperationId == operationId,
           currentRecoveryPath == recoveryPath,
           currentCode == code {
            return
        }
        blockedDetails = (operationId, recoveryPath, code)
        recoveryState = .blocked(
            operationId: operationId,
            recoveryPath: recoveryPath,
            code: code
        )
        generation &+= 1
    }

    @discardableResult
    func finishRecovery(_ token: DeviceSyncSessionToken, clearBlock: Bool) -> Bool {
        let current = isCurrent(token)
        endOperation(token)
        guard current else { return false }
        guard clearBlock else {
            if let blockedDetails {
                recoveryState = .blocked(
                    operationId: blockedDetails.operationId,
                    recoveryPath: blockedDetails.recoveryPath,
                    code: blockedDetails.code
                )
            }
            return true
        }
        blockedDetails = nil
        recoveryState = .ready
        generation &+= 1
        return true
    }

    func invalidateGeneration() {
        generation &+= 1
        if recoveryState == .recovering {
            recoveryState = .ready
        }
    }

    private func gateFailure(
        code: String,
        operationId: String?,
        recoveryPath: String?,
        stage: String
    ) -> DeviceSyncApplyError {
        DeviceSyncApplyError(
            code: code,
            messageKey: "deviceSync.apply.\(code)",
            operationId: operationId,
            recoveryPath: recoveryPath,
            stage: stage
        )
    }
}

enum DeviceSyncApplyJournalPhase: String, Codable {
    case prepared
    case preferencesWritten = "preferences_written"
    case rustCommitted = "rust_committed"
    case rollbackRequested = "rollback_requested"
    case rolledBack = "rolled_back"
}

struct DeviceSyncApplyJournal: Codable, Equatable, Sendable {
    static let schemaVersion = 1

    let schemaVersion: Int
    let transactionId: String
    let recoveryPath: String?
    let oldSkillsEnabledProviders: String?
    let newSkillsEnabledProviders: String?
    var phase: DeviceSyncApplyJournalPhase

    init(
        transactionId: String,
        recoveryPath: String?,
        oldSkillsEnabledProviders: String?,
        newSkillsEnabledProviders: String?,
        phase: DeviceSyncApplyJournalPhase
    ) {
        self.schemaVersion = Self.schemaVersion
        self.transactionId = transactionId
        self.recoveryPath = recoveryPath
        self.oldSkillsEnabledProviders = oldSkillsEnabledProviders
        self.newSkillsEnabledProviders = newSkillsEnabledProviders
        self.phase = phase
    }
}

actor DeviceSyncApplyWorker {
    private static let maxJournalBytes = 1_048_576
    private static let preferenceKey = "skillsEnabledProviders"
    private static let allowedPreferenceKeys: Set<String> = [preferenceKey]

    private let core: DeviceSyncApplyCore
    private let preferences: DeviceSyncPreferenceStore
    private let journalDirectory: URL
    private let fileManager: FileManager

    init(
        core: DeviceSyncApplyCore,
        preferenceStore: DeviceSyncPreferenceStore,
        journalDirectory: URL,
        fileManager: FileManager = .default
    ) {
        self.core = core
        self.preferences = preferenceStore
        self.journalDirectory = journalDirectory
        self.fileManager = fileManager
    }

    func restoreMasterKeyIfAvailable() async throws -> Bool {
        try await core.deviceSyncRawRestoreMasterKeyIfAvailable()
    }

    func apply(previewToken: String) async throws -> DeviceSyncResult {
        var transactionId: String?
        var recoveryPath: String?
        var journal: DeviceSyncApplyJournal?
        var journalPersisted = false
        var rustCommitted = false

        do {
            let prepared = try await core.deviceSyncRawPrepareApply(previewToken: previewToken)
            guard isSafeIdentifier(prepared.transactionId) else {
                throw makeFailure(
                    code: "invalid_transaction_id",
                    stage: "prepare",
                    operationId: nil,
                    recoveryPath: prepared.recoveryPath
                )
            }
            guard isSafeRecoveryPath(
                prepared.recoveryPath,
                transactionId: prepared.transactionId
            ) else {
                throw makeFailure(
                    code: "invalid_recovery_path",
                    stage: "prepare",
                    operationId: prepared.transactionId,
                    recoveryPath: prepared.recoveryPath
                )
            }
            transactionId = prepared.transactionId
            recoveryPath = prepared.recoveryPath

            let newPreference = try preferenceMutationValue(
                prepared.preferenceMutations,
                transactionId: transactionId,
                recoveryPath: recoveryPath
            )
            let oldPreference: String?
            if newPreference != nil {
                oldPreference = try preferences.value(forKey: Self.preferenceKey)
            } else {
                oldPreference = nil
            }

            var nextJournal = DeviceSyncApplyJournal(
                transactionId: prepared.transactionId,
                recoveryPath: prepared.recoveryPath,
                oldSkillsEnabledProviders: oldPreference,
                newSkillsEnabledProviders: newPreference,
                phase: .prepared
            )
            journal = nextJournal
            try writeJournal(nextJournal)
            journalPersisted = true

            if let newPreference {
                try preferences.setValue(newPreference, forKey: Self.preferenceKey)
                guard try preferences.value(forKey: Self.preferenceKey) == newPreference else {
                    throw makeFailure(
                        code: "preference_write_failed",
                        stage: "preference_write",
                        operationId: transactionId,
                        recoveryPath: recoveryPath
                    )
                }
            }

            try transitionJournal(&nextJournal, to: .preferencesWritten)
            journal = nextJournal

            let result: DeviceSyncResult
            do {
                result = try await core.deviceSyncRawCommitApply(transactionId: prepared.transactionId)
                rustCommitted = true
            } catch {
                if isTerminalCommitFailure(error) {
                    // Rust has durably committed the files even when state
                    // persistence reported recovery_blocked. Preserve that
                    // fact locally whenever the Swift journal is writable.
                    rustCommitted = true
                    try transitionJournal(&nextJournal, to: .rustCommitted)
                    journal = nextJournal
                }
                throw error
            }

            try transitionJournal(&nextJournal, to: .rustCommitted)
            journal = nextJournal

            _ = try await core.deviceSyncRawFinalizeApply(transactionId: prepared.transactionId)
            try removeJournal(nextJournal)
            return result
        } catch {
            let failure = failureFor(
                error,
                stage: rustCommitted ? "finalize" : "apply",
                operationId: transactionId ?? errorOperationId(error),
                recoveryPath: recoveryPath ?? errorRecoveryPath(error)
            )

            // A committed Rust transaction cannot be rolled back. Keep the
            // durable journal so startup recovery can converge state.
            if rustCommitted {
                throw failure
            }

            // If the first Swift journal write failed, continuing would leave
            // a Rust transaction with no Swift-side recovery intent.
            if journal != nil && !journalPersisted {
                throw failure
            }

            if var rollbackJournal = journal {
                do {
                    try transitionJournal(&rollbackJournal, to: .rollbackRequested)
                    journal = rollbackJournal
                } catch {
                    // The Rust transaction must not be touched, and the old
                    // preference must not be restored, until this intent is
                    // durable in the Swift journal.
                    throw error
                }
            }

            let rollbackError = await rollback(transactionId: transactionId)
            if rollbackError != nil {
                throw makeFailure(
                    code: "rollback_failed",
                    stage: "rollback",
                    operationId: transactionId ?? failure.operationId,
                    recoveryPath: recoveryPath ?? journal?.recoveryPath
                )
            }

            do {
                try restorePreference(from: journal)
            } catch {
                throw makeFailure(
                    code: "rollback_failed",
                    stage: "preference_restore",
                    operationId: transactionId ?? failure.operationId,
                    recoveryPath: recoveryPath ?? journal?.recoveryPath
                )
            }

            if var journal {
                do {
                    try transitionJournal(&journal, to: .rolledBack)
                    try removeJournal(journal)
                } catch {
                    throw makeFailure(
                        code: "journal_cleanup_failed",
                        stage: "journal_cleanup",
                        operationId: transactionId ?? failure.operationId,
                        recoveryPath: recoveryPath ?? journal.recoveryPath
                    )
                }
            }
            throw failure
        }
    }

    func recoverPendingApply() async throws -> DeviceSyncRecoveryResponse {
        let response = try await core.deviceSyncRawRecoverPendingApply()
        let journals = try loadJournals()
        let rolledBack = Set(response.rolledBackTransactionIds)
        let committed = Set(response.committedTransactionIds)

        guard rolledBack.isDisjoint(with: committed) else {
            throw makeFailure(
                code: "recovery_ambiguous",
                stage: "recovery",
                operationId: nil,
                recoveryPath: nil
            )
        }

        for journal in journals {
            let transactionId = journal.transactionId
            if committed.contains(transactionId) {
                guard journal.phase != .rolledBack && journal.phase != .rollbackRequested else {
                    throw makeFailure(
                        code: "recovery_ambiguous",
                        stage: "recovery",
                        operationId: transactionId,
                        recoveryPath: journal.recoveryPath
                    )
                }
                try removeJournal(journal)
                continue
            }

            if rolledBack.contains(transactionId) {
                guard journal.phase != .rustCommitted else {
                    throw makeFailure(
                        code: "recovery_ambiguous",
                        stage: "recovery",
                        operationId: transactionId,
                        recoveryPath: journal.recoveryPath
                    )
                }
                var completed = journal
                if completed.phase != .rolledBack {
                    if completed.phase != .rollbackRequested {
                        try transitionJournal(&completed, to: .rollbackRequested)
                    }
                    try restorePreferenceOrThrow(from: completed)
                    try transitionJournal(&completed, to: .rolledBack)
                }
                try removeJournal(completed)
                continue
            }

            let recoveryPathIsAbsent = journal.recoveryPath.map {
                !fileManager.fileExists(atPath: $0)
            } ?? true
            if recoveryPathIsAbsent {
                switch journal.phase {
                case .prepared, .preferencesWritten, .rollbackRequested:
                    var completed = journal
                    if completed.phase != .rollbackRequested {
                        try transitionJournal(&completed, to: .rollbackRequested)
                    }
                    try restorePreferenceOrThrow(from: completed)
                    try transitionJournal(&completed, to: .rolledBack)
                    try removeJournal(completed)
                    continue
                case .rustCommitted, .rolledBack:
                    try removeJournal(journal)
                    continue
                }
            }

            // A recovery directory that has no recorded Rust outcome is
            // ambiguous. Do not infer the result from the Swift phase.
            throw makeFailure(
                code: "recovery_incomplete",
                stage: "recovery",
                operationId: transactionId,
                recoveryPath: journal.recoveryPath
            )
        }
        return response
    }

    private func preferenceMutationValue(
        _ mutations: [DeviceSyncPreferenceMutation],
        transactionId: String?,
        recoveryPath: String?
    ) throws -> String? {
        var seenKeys = Set<String>()
        guard mutations.count <= Self.allowedPreferenceKeys.count else {
            throw makeFailure(
                code: "invalid_preference_mutation",
                stage: "preference_validation",
                operationId: transactionId,
                recoveryPath: recoveryPath
            )
        }

        var value: String?
        for mutation in mutations {
            guard Self.allowedPreferenceKeys.contains(mutation.key),
                  seenKeys.insert(mutation.key).inserted else {
                throw makeFailure(
                    code: "unknown_preference_key",
                    stage: "preference_validation",
                    operationId: transactionId,
                    recoveryPath: recoveryPath
                )
            }
            var ids = Set<String>()
            guard mutation.enabledAgentIds.allSatisfy({
                isSafeIdentifier($0) && ids.insert($0).inserted
            }) else {
                throw makeFailure(
                    code: "invalid_preference_mutation",
                    stage: "preference_validation",
                    operationId: transactionId,
                    recoveryPath: recoveryPath
                )
            }
            let data = try JSONEncoder().encode(mutation.enabledAgentIds)
            guard let encoded = String(data: data, encoding: .utf8) else {
                throw makeFailure(
                    code: "invalid_preference_mutation",
                    stage: "preference_validation",
                    operationId: transactionId,
                    recoveryPath: recoveryPath
                )
            }
            value = encoded
        }
        return value
    }

    private func rollback(transactionId: String?) async -> Error? {
        guard let transactionId else { return nil }
        do {
            _ = try await core.deviceSyncRawRollbackApply(transactionId: transactionId)
            return nil
        } catch {
            return error
        }
    }

    private func restorePreference(from journal: DeviceSyncApplyJournal?) throws {
        guard let journal, journal.newSkillsEnabledProviders != nil else { return }
        try preferences.setValue(journal.oldSkillsEnabledProviders, forKey: Self.preferenceKey)
        guard try preferences.value(forKey: Self.preferenceKey) == journal.oldSkillsEnabledProviders else {
            throw DeviceSyncPreferenceStoreError.nonStringValue
        }
    }

    private func restorePreferenceOrThrow(from journal: DeviceSyncApplyJournal) throws {
        do {
            try restorePreference(from: journal)
        } catch let error as DeviceSyncApplyError {
            throw error
        } catch {
            throw makeFailure(
                code: "preference_write_failed",
                stage: "preference_restore",
                operationId: journal.transactionId,
                recoveryPath: journal.recoveryPath
            )
        }
    }

    private func writeJournal(_ journal: DeviceSyncApplyJournal) throws {
        guard isSafeIdentifier(journal.transactionId) else {
            throw makeFailure(
                code: "invalid_transaction_id",
                stage: "journal_write",
                operationId: journal.transactionId,
                recoveryPath: journal.recoveryPath
            )
        }
        guard isSafeRecoveryPath(journal.recoveryPath, transactionId: journal.transactionId) else {
            throw makeFailure(
                code: "invalid_recovery_path",
                stage: "journal_write",
                operationId: journal.transactionId,
                recoveryPath: journal.recoveryPath
            )
        }
        try ensureJournalDirectory()
        let url = journalURL(for: journal.transactionId)
        if fileManager.fileExists(atPath: url.path) || isSymbolicLink(url) {
            try validateJournalFile(
                url,
                operationId: journal.transactionId,
                recoveryPath: journal.recoveryPath,
                code: "journal_write_failed",
                stage: "journal_write"
            )
        }

        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        do {
            let data = try encoder.encode(journal)
            guard data.count <= Self.maxJournalBytes else {
                throw makeFailure(
                    code: "journal_write_failed",
                    stage: "journal_write",
                    operationId: journal.transactionId,
                    recoveryPath: journal.recoveryPath
                )
            }
            try writeDataAtomicallyAndSync(data, to: url)
            try fileManager.setAttributes(
                [.posixPermissions: NSNumber(value: 0o600)],
                ofItemAtPath: url.path
            )
            try validateJournalFile(
                url,
                operationId: journal.transactionId,
                recoveryPath: journal.recoveryPath,
                code: "journal_write_failed",
                stage: "journal_write"
            )
        } catch let error as DeviceSyncApplyError {
            throw error
        } catch {
            throw makeFailure(
                code: "journal_write_failed",
                stage: "journal_write",
                operationId: journal.transactionId,
                recoveryPath: journal.recoveryPath
            )
        }
    }

    private func transitionJournal(
        _ journal: inout DeviceSyncApplyJournal,
        to phase: DeviceSyncApplyJournalPhase
    ) throws {
        guard journalPhaseTransitionAllowed(from: journal.phase, to: phase) else {
            throw makeFailure(
                code: "recovery_ambiguous",
                stage: "journal_write",
                operationId: journal.transactionId,
                recoveryPath: journal.recoveryPath
            )
        }
        var next = journal
        next.phase = phase
        try writeJournal(next)
        journal = next
    }

    private func removeJournal(_ journal: DeviceSyncApplyJournal) throws {
        try validateJournalDirectory(stage: "journal_cleanup", code: "journal_cleanup_failed")
        let url = journalURL(for: journal.transactionId)
        guard fileManager.fileExists(atPath: url.path) || isSymbolicLink(url) else { return }
        do {
            try validateJournalFile(
                url,
                operationId: journal.transactionId,
                recoveryPath: journal.recoveryPath,
                code: "journal_cleanup_failed",
                stage: "journal_cleanup"
            )
            try fileManager.removeItem(at: url)
            try syncDirectory(journalDirectory)
        } catch let error as DeviceSyncApplyError {
            throw error
        } catch {
            throw makeFailure(
                code: "journal_cleanup_failed",
                stage: "journal_cleanup",
                operationId: journal.transactionId,
                recoveryPath: journal.recoveryPath
            )
        }
    }

    private func loadJournals() throws -> [DeviceSyncApplyJournal] {
        try validateJournalDirectory(stage: "recovery", code: "journal_read_failed")
        guard fileManager.fileExists(atPath: journalDirectory.path) else { return [] }
        let urls: [URL]
        do {
            urls = try fileManager.contentsOfDirectory(
                at: journalDirectory,
                includingPropertiesForKeys: [.isSymbolicLinkKey, .isRegularFileKey],
                options: []
            )
        } catch {
            throw makeFailure(
                code: "journal_read_failed",
                stage: "recovery",
                operationId: nil,
                recoveryPath: journalDirectory.path
            )
        }

        return try urls
            .filter { $0.pathExtension == "json" }
            .sorted { $0.lastPathComponent < $1.lastPathComponent }
            .map { url in
                do {
                    let fileName = url.deletingPathExtension().lastPathComponent
                    guard url.deletingLastPathComponent().standardizedFileURL
                            == journalDirectory.standardizedFileURL,
                          isSafeIdentifier(fileName) else {
                        throw makeFailure(
                            code: "journal_corrupt",
                            stage: "recovery",
                            operationId: isSafeIdentifier(fileName) ? fileName : nil,
                            recoveryPath: journalDirectory.path
                        )
                    }
                    try validateJournalFile(
                        url,
                        operationId: fileName,
                        recoveryPath: journalDirectory.path,
                        code: "journal_corrupt",
                        stage: "recovery"
                    )
                    let values = try url.resourceValues(forKeys: [.fileSizeKey])
                    guard let fileSize = values.fileSize, fileSize <= Self.maxJournalBytes else {
                        throw makeFailure(
                            code: "journal_corrupt",
                            stage: "recovery",
                            operationId: fileName,
                            recoveryPath: journalDirectory.path
                        )
                    }
                    let journal = try JSONDecoder().decode(
                        DeviceSyncApplyJournal.self,
                        from: Data(contentsOf: url)
                    )
                    guard journal.schemaVersion == DeviceSyncApplyJournal.schemaVersion,
                          isSafeIdentifier(journal.transactionId),
                          journalURL(for: journal.transactionId).standardizedFileURL
                            == url.standardizedFileURL,
                          isSafeRecoveryPath(
                              journal.recoveryPath,
                              transactionId: journal.transactionId
                          ) else {
                        throw makeFailure(
                            code: "journal_corrupt",
                            stage: "recovery",
                            operationId: journal.transactionId,
                            recoveryPath: journal.recoveryPath
                        )
                    }
                    return journal
                } catch let error as DeviceSyncApplyError {
                    throw error
                } catch {
                    throw makeFailure(
                        code: "journal_corrupt",
                        stage: "recovery",
                        operationId: url.deletingPathExtension().lastPathComponent,
                        recoveryPath: nil
                    )
                }
            }
    }

    private func ensureJournalDirectory() throws {
        do {
            try validateJournalDirectory(stage: "journal_write", code: "journal_write_failed")
            try fileManager.createDirectory(
                at: journalDirectory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: NSNumber(value: 0o700)]
            )
            try fileManager.setAttributes(
                [.posixPermissions: NSNumber(value: 0o700)],
                ofItemAtPath: journalDirectory.path
            )
            try validateJournalDirectory(stage: "journal_write", code: "journal_write_failed")
        } catch let error as DeviceSyncApplyError {
            throw error
        } catch {
            throw makeFailure(
                code: "journal_write_failed",
                stage: "journal_write",
                operationId: nil,
                recoveryPath: journalDirectory.path
            )
        }
    }

    private func validateJournalDirectory(stage: String, code: String) throws {
        let directory = journalDirectory.standardizedFileURL
        guard directory.isFileURL,
              directory.path.hasPrefix("/"),
              directory.path.utf8.count <= 4_096 else {
            throw makeFailure(
                code: code,
                stage: stage,
                operationId: nil,
                recoveryPath: journalDirectory.path
            )
        }
        if isSymbolicLink(directory) {
            throw makeFailure(
                code: code,
                stage: stage,
                operationId: nil,
                recoveryPath: journalDirectory.path
            )
        }

        let resolved = directory.resolvingSymlinksInPath().standardizedFileURL
        if fileManager.fileExists(atPath: directory.path) {
            let values = try resolved.resourceValues(forKeys: [.isDirectoryKey])
            guard values.isDirectory == true else {
                throw makeFailure(
                    code: code,
                    stage: stage,
                    operationId: nil,
                    recoveryPath: journalDirectory.path
                )
            }
        } else {
            var current = resolved
            while !fileManager.fileExists(atPath: current.path) {
                let parent = current.deletingLastPathComponent()
                guard parent.path != current.path else {
                    throw makeFailure(
                        code: code,
                        stage: stage,
                        operationId: nil,
                        recoveryPath: journalDirectory.path
                    )
                }
                current = parent
            }
            let values = try current.resourceValues(forKeys: [.isDirectoryKey])
            guard values.isDirectory == true else {
                throw makeFailure(
                    code: code,
                    stage: stage,
                    operationId: nil,
                    recoveryPath: journalDirectory.path
                )
            }
        }
    }

    private func validateJournalFile(
        _ url: URL,
        operationId: String?,
        recoveryPath: String?,
        code: String,
        stage: String
    ) throws {
        guard url.deletingLastPathComponent().standardizedFileURL
                == journalDirectory.standardizedFileURL,
              !isSymbolicLink(url),
              fileManager.fileExists(atPath: url.path) else {
            throw makeFailure(
                code: code,
                stage: stage,
                operationId: operationId,
                recoveryPath: recoveryPath
            )
        }
        let values = try url.resourceValues(forKeys: [.isRegularFileKey])
        guard values.isRegularFile == true else {
            throw makeFailure(
                code: code,
                stage: stage,
                operationId: operationId,
                recoveryPath: recoveryPath
            )
        }
    }

    private func writeDataAtomicallyAndSync(_ data: Data, to url: URL) throws {
        try data.write(to: url, options: .atomic)
        let file = try FileHandle(forWritingTo: url)
        try file.synchronize()
        try file.close()
        try syncDirectory(url.deletingLastPathComponent())
    }

    private func syncDirectory(_ url: URL) throws {
        #if canImport(Darwin)
        let descriptor = open(url.path, O_RDONLY | O_DIRECTORY)
        guard descriptor >= 0 else { throw CocoaError(.fileWriteUnknown) }
        defer { close(descriptor) }
        guard fsync(descriptor) == 0 else { throw CocoaError(.fileWriteUnknown) }
        #endif
    }

    private func journalURL(for transactionId: String) -> URL {
        journalDirectory.appendingPathComponent("\(transactionId).json", isDirectory: false)
    }

    private func isSymbolicLink(_ url: URL) -> Bool {
        (try? fileManager.destinationOfSymbolicLink(atPath: url.path)) != nil
    }

    private func isSafeRecoveryPath(_ path: String?, transactionId: String) -> Bool {
        guard let path else { return true }
        guard !path.isEmpty,
              path.utf8.count <= 4_096,
              path.hasPrefix("/"),
              !path.contains("\0"),
              !path.unicodeScalars.contains(where: { $0.value < 0x20 || $0.value == 0x7F }) else {
            return false
        }
        let url = URL(fileURLWithPath: path).standardizedFileURL
        guard url.lastPathComponent == transactionId, !isSymbolicLink(url) else { return false }
        if fileManager.fileExists(atPath: url.path) {
            let values = try? url.resourceValues(forKeys: [.isDirectoryKey])
            return values?.isDirectory == true
        }
        return true
    }

    private func isSafeIdentifier(_ value: String) -> Bool {
        guard !value.isEmpty, value.utf8.count <= 1_024, value != ".", value != ".." else {
            return false
        }
        return value.unicodeScalars.allSatisfy { scalar in
            let code = scalar.value
            let isLetter = (code >= 0x41 && code <= 0x5A) || (code >= 0x61 && code <= 0x7A)
            let isDigit = code >= 0x30 && code <= 0x39
            return isLetter || isDigit || code == 0x2D || code == 0x5F || code == 0x2E
        }
    }
}

@MainActor
final class DeviceSyncApplyCoordinator: ObservableObject {
    static let shared = DeviceSyncApplyCoordinator()
    nonisolated static let preferenceKey = "skillsEnabledProviders"

    private let worker: DeviceSyncApplyWorker
    private let journalDirectory: URL
    let sessionGate: DeviceSyncSessionGate
    private var hasExternalRecoveryBlock = false

    @Published private(set) var state: DeviceSyncApplyState = .idle
    @Published private(set) var recoveryState: DeviceSyncRecoveryState = .ready

    init(
        core: DeviceSyncApplyCore = CoreBridge.shared,
        defaults: UserDefaults = .standard,
        journalDirectory: URL? = nil,
        fileManager: FileManager = .default,
        sessionGate: DeviceSyncSessionGate? = nil
    ) {
        self.sessionGate = sessionGate ?? DeviceSyncSessionGate()
        self.journalDirectory = journalDirectory ?? Self.defaultJournalDirectory(fileManager: fileManager)
        self.worker = DeviceSyncApplyWorker(
            core: core,
            preferenceStore: UserDefaultsDeviceSyncPreferenceStore(defaults: defaults),
            journalDirectory: self.journalDirectory,
            fileManager: fileManager
        )
    }

    init(
        core: DeviceSyncApplyCore,
        preferenceStore: DeviceSyncPreferenceStore,
        journalDirectory: URL,
        fileManager: FileManager = .default,
        sessionGate: DeviceSyncSessionGate? = nil
    ) {
        self.sessionGate = sessionGate ?? DeviceSyncSessionGate()
        self.journalDirectory = journalDirectory
        self.worker = DeviceSyncApplyWorker(
            core: core,
            preferenceStore: preferenceStore,
            journalDirectory: journalDirectory,
            fileManager: fileManager
        )
    }

    var isApplyGuardActive: Bool { sessionGate.isOperationInProgress }
    var isRecoveryBlocked: Bool { sessionGate.isRecoveryBlocked }

    /// Filesystem listeners can use this to avoid scheduling reverse sync
    /// while an async apply or recovery operation is still running.
    var suppressDeviceSyncListener: Bool {
        isApplyGuardActive || isRecoveryBlocked || sessionGate.recoveryState == .recovering
    }

    func updateProfile(_ profileId: String?) {
        sessionGate.updateProfile(profileId)
        recoveryState = sessionGate.recoveryState
    }

    /// Consume a status read without routing it through the operation gate.
    /// Status is the one read allowed while recovery is blocked, so it is also
    /// the source that makes a Rust-side recovery block visible to observers.
    func observeStatus(_ status: DeviceSyncStatus) {
        updateProfile(status.config.profileId)
        guard status.recoveryBlocked else { return }
        guard sessionGate.recoveryState != .recovering else { return }
        guard !sessionGate.isRecoveryBlocked else {
            recoveryState = sessionGate.recoveryState
            return
        }
        let failure = makeFailure(
            code: "recovery_blocked",
            stage: "status",
            operationId: nil,
            recoveryPath: nil
        )
        recordRecoveryBlock(failure)
    }

    func invalidateGeneration() {
        sessionGate.invalidateGeneration()
        recoveryState = sessionGate.recoveryState
    }

    @discardableResult
    func apply(previewToken: String) async throws -> DeviceSyncResult {
        guard !previewToken.isEmpty else {
            let failure = makeFailure(
                code: "invalid_preview",
                stage: "prepare",
                operationId: nil,
                recoveryPath: nil
            )
            state = .failed(operationId: nil, recoveryPath: nil, code: failure.code)
            throw failure
        }

        let token = try sessionGate.beginOperation()
        state = .applying(operationId: nil)
        do {
            let result = try await runDetached { [worker] in
                try await worker.apply(previewToken: previewToken)
            }
            let current = sessionGate.isCurrent(token)
            sessionGate.endOperation(token)
            if current {
                state = .idle
            }
            return result
        } catch {
            let failure = failureFor(
                error,
                stage: "apply",
                operationId: errorOperationId(error),
                recoveryPath: errorRecoveryPath(error)
            )
            let current = sessionGate.isCurrent(token)
            sessionGate.endOperation(token)
            if current {
                recordFailure(failure)
            }
            throw failure
        }
    }

    /// Run a non-Apply Device Sync operation through the same gate. Rust also
    /// enforces recovery_blocked, so direct bridge callers cannot bypass it.
    func runDeviceSyncOperation<T: Sendable>(
        operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        let token = try sessionGate.beginOperation()
        do {
            let result = try await runDetached(operation)
            sessionGate.endOperation(token)
            return result
        } catch {
            let failure = failureFor(
                error,
                stage: "operation",
                operationId: errorOperationId(error),
                recoveryPath: errorRecoveryPath(error)
            )
            let current = sessionGate.isCurrent(token)
            sessionGate.endOperation(token)
            if current && isRecoveryFailureCode(failure.code) {
                markRecoveryBlocked(failure)
            }
            throw failure
        }
    }

    @discardableResult
    func restoreMasterKeyIfAvailable() async throws -> Bool {
        let wasRecoveryBlocked = sessionGate.isRecoveryBlocked
        let token = try sessionGate.beginRecovery()
        recoveryState = sessionGate.recoveryState
        do {
            let restored = try await runDetached { [worker] in
                try await worker.restoreMasterKeyIfAvailable()
            }
            let current = sessionGate.finishRecovery(
                token,
                clearBlock: !hasExternalRecoveryBlock && !wasRecoveryBlocked
            )
            if current {
                recoveryState = sessionGate.recoveryState
            }
            return restored
        } catch {
            let failure = failureFor(
                error,
                stage: "master_key_restore",
                operationId: errorOperationId(error),
                recoveryPath: errorRecoveryPath(error)
            )
            let current = sessionGate.isCurrent(token)
            sessionGate.endOperation(token)
            if current {
                markRecoveryBlocked(failure)
            }
            throw failure
        }
    }

    @discardableResult
    func recoverPendingApply() async throws -> DeviceSyncRecoveryResponse {
        let token = try sessionGate.beginRecovery()
        recoveryState = sessionGate.recoveryState
        do {
            let response = try await runDetached { [worker] in
                try await worker.recoverPendingApply()
            }
            let clearBlock = !hasExternalRecoveryBlock
            let current = sessionGate.finishRecovery(token, clearBlock: clearBlock)
            if current {
                if clearBlock {
                    hasExternalRecoveryBlock = false
                }
                recoveryState = sessionGate.recoveryState
            }
            if current && clearBlock {
                state = .idle
            }
            return response
        } catch {
            let failure = failureFor(
                error,
                stage: "recovery",
                operationId: errorOperationId(error),
                recoveryPath: errorRecoveryPath(error)
            )
            let current = sessionGate.isCurrent(token)
            sessionGate.endOperation(token)
            if current {
                markRecoveryBlocked(failure)
            }
            throw failure
        }
    }

    /// Re-read Keychain credentials and run recovery in one observable
    /// operation. This is the supported way to clear a temporary block.
    @discardableResult
    func retryRecovery() async throws -> DeviceSyncRecoveryResponse {
        let token = try sessionGate.beginRecovery()
        recoveryState = sessionGate.recoveryState
        do {
            let restored = try await runDetached { [worker] in
                try await worker.restoreMasterKeyIfAvailable()
            }
            guard restored || !hasExternalRecoveryBlock else {
                throw makeFailure(
                    code: "recovery_blocked",
                    stage: "master_key_restore",
                    operationId: nil,
                    recoveryPath: nil
                )
            }
            let response = try await runDetached { [worker] in
                try await worker.recoverPendingApply()
            }
            let current = sessionGate.finishRecovery(token, clearBlock: true)
            if current {
                hasExternalRecoveryBlock = false
                recoveryState = sessionGate.recoveryState
                state = .idle
            }
            return response
        } catch {
            let failure = failureFor(
                error,
                stage: "recovery",
                operationId: errorOperationId(error),
                recoveryPath: errorRecoveryPath(error)
            )
            let current = sessionGate.isCurrent(token)
            sessionGate.endOperation(token)
            if current {
                markRecoveryBlocked(failure)
            }
            throw failure
        }
    }

    func markRecoveryBlocked(_ error: Error) {
        hasExternalRecoveryBlock = true
        let failure = failureFor(
            error,
            stage: "recovery",
            operationId: errorOperationId(error),
            recoveryPath: errorRecoveryPath(error)
        )
        recordRecoveryBlock(failure)
    }

    func journalURL(for transactionId: String) -> URL {
        journalDirectory
            .appendingPathComponent("\(transactionId).json", isDirectory: false)
    }

    private func recordFailure(_ failure: DeviceSyncApplyError) {
        if isRecoveryFailureCode(failure.code) {
            recordRecoveryBlock(failure)
        } else {
            state = .failed(
                operationId: failure.operationId,
                recoveryPath: failure.recoveryPath,
                code: failure.code
            )
        }
    }

    private func recordRecoveryBlock(_ failure: DeviceSyncApplyError) {
        state = .recoveryBlocked(
            operationId: failure.operationId,
            recoveryPath: failure.recoveryPath,
            code: failure.code
        )
        sessionGate.markRecoveryBlocked(
            operationId: failure.operationId,
            recoveryPath: failure.recoveryPath,
            code: failure.code
        )
        recoveryState = sessionGate.recoveryState
    }

    private func runDetached<T: Sendable>(
        _ operation: @escaping @Sendable () async throws -> T
    ) async throws -> T {
        try await Task.detached(priority: .userInitiated, operation: operation).value
    }

    private static func defaultJournalDirectory(fileManager: FileManager) -> URL {
        fileManager.homeDirectoryForCurrentUser
            .appendingPathComponent(".tokenviewer/device-sync/apply-journal", isDirectory: true)
    }
}

private func makeFailure(
    code: String,
    stage: String,
    operationId: String?,
    recoveryPath: String?
) -> DeviceSyncApplyError {
    DeviceSyncApplyError(
        code: code,
        messageKey: "deviceSync.apply.\(code)",
        operationId: operationId,
        recoveryPath: recoveryPath,
        stage: stage
    )
}

private func failureFor(
    _ error: Error,
    stage: String,
    operationId: String?,
    recoveryPath: String?
) -> DeviceSyncApplyError {
    if let failure = error as? DeviceSyncApplyError {
        return DeviceSyncApplyError(
            code: failure.code,
            messageKey: failure.messageKey,
            operationId: failure.operationId ?? operationId,
            recoveryPath: failure.recoveryPath ?? recoveryPath,
            stage: failure.stage
        )
    }
    if let bridgeError = error as? DeviceSyncBridgeError,
       case let .core(payload) = bridgeError {
        return DeviceSyncApplyError(
            code: payload.code,
            messageKey: payload.messageKey,
            operationId: payload.operationId ?? operationId,
            recoveryPath: recoveryPath ?? payload.arguments["recovery_path"],
            stage: stage
        )
    }
    let code: String
    switch stage {
    case "preference_validation": code = "invalid_preference_mutation"
    case "preference_write", "preference_restore": code = "preference_write_failed"
    case "journal_write": code = "journal_write_failed"
    case "journal_cleanup": code = "journal_cleanup_failed"
    case "recovery", "master_key_restore": code = "recovery_failed"
    default: code = "apply_failed"
    }
    return makeFailure(
        code: code,
        stage: stage,
        operationId: operationId,
        recoveryPath: recoveryPath
    )
}

private func errorOperationId(_ error: Error) -> String? {
    if let failure = error as? DeviceSyncApplyError { return failure.operationId }
    if let bridgeError = error as? DeviceSyncBridgeError { return bridgeError.operationId }
    return nil
}

private func errorRecoveryPath(_ error: Error) -> String? {
    (error as? DeviceSyncApplyError)?.recoveryPath
}

private func isRecoveryFailureCode(_ code: String) -> Bool {
    code == "recovery_blocked"
        || code == "rollback_failed"
        || code == "journal_write_failed"
        || code == "journal_cleanup_failed"
        || code == "recovery_ambiguous"
        || code == "recovery_incomplete"
        || code == "recovery_failed"
}

private func isTerminalCommitFailure(_ error: Error) -> Bool {
    guard let bridgeError = error as? DeviceSyncBridgeError,
          case let .core(payload) = bridgeError else {
        return false
    }
    // `recovery_blocked` is emitted after Rust has durably written the
    // committed journal but could not persist state. `rollback_failed` means
    // the apply is still unresolved and must go through the rollback path.
    return payload.code == "recovery_blocked"
}

private func journalPhaseTransitionAllowed(
    from current: DeviceSyncApplyJournalPhase,
    to next: DeviceSyncApplyJournalPhase
) -> Bool {
    current == next
        || (current == .prepared && (next == .preferencesWritten || next == .rollbackRequested))
        || (current == .preferencesWritten
            && (next == .rustCommitted || next == .rollbackRequested))
        || (current == .rollbackRequested && next == .rolledBack)
}
