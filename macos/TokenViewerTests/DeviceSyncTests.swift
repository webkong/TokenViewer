import Foundation
import Security
import XCTest
@testable import TokenViewer

@MainActor
final class DeviceSyncTests: XCTestCase {
    func testDeviceSyncErrorLocalizationHasChineseAndEnglishParity() {
        let codes = [
            "invalid_config",
            "credential_missing",
            "authentication_failed",
            "network_unreachable",
            "rate_limited",
            "protocol_unsupported",
            "remote_directory_unavailable",
            "vault_not_found",
            "vault_auth_failed",
            "object_too_large",
            "archive_unsafe",
            "integrity_failed",
            "operation_in_progress",
            "stale_preview",
            "remote_changed",
            "conflict_requires_resolution",
            "link_target_occupied",
            "apply_failed",
            "rollback_failed",
            "partial_failure",
            "immutable_object_conflict",
            "remote_rollback_detected",
            "recovery_blocked",
            "internal_error",
            "invalid_transaction_id",
            "invalid_recovery_path",
            "unknown_preference_key",
            "invalid_preference_mutation",
            "preference_write_failed",
            "journal_write_failed",
            "journal_read_failed",
            "journal_corrupt",
            "journal_cleanup_failed",
            "recovery_ambiguous",
            "recovery_incomplete",
            "recovery_failed",
        ]
        let originalLanguage = L10n.shared.language
        defer { L10n.shared.language = originalLanguage }

        for language in [AppLanguage.zh, .en] {
            L10n.shared.language = language
            for code in codes {
                XCTAssertFalse(
                    L10n.shared.deviceSyncErrorMessage(code).isEmpty,
                    "Missing (language) Device Sync message for (code)"
                )
            }
        }
    }

    func testDeviceSyncErrorEnvelopeDecodesWithSnakeCaseFields() throws {
        let data = Data(
            """
            {
              "ok": false,
              "data": null,
              "error": {
                "code": "stale_preview",
                "message_key": "deviceSync.error.stalePreview",
                "arguments": {"device_name": "test-device"},
                "retryable": true,
                "operation_id": "operation-id"
              }
            }
            """.utf8
        )
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let envelope = try decoder.decode(
            DeviceSyncEnvelope<DeviceSyncPreviewResponse>.self,
            from: data
        )

        XCTAssertFalse(envelope.ok)
        XCTAssertEqual(envelope.error?.code, "stale_preview")
        XCTAssertEqual(envelope.error?.messageKey, "deviceSync.error.stalePreview")
        XCTAssertEqual(envelope.error?.arguments["device_name"], "test-device")
        XCTAssertEqual(envelope.error?.operationId, "operation-id")
    }

    func testDeviceSyncConfigUsesRustWireKeysAndContainsNoCredentialFields() throws {
        let config = DeviceSyncConfig(
            profileId: "profile-id",
            enabled: false,
            vaultId: "vault-id",
            provider: DeviceSyncProviderConfig(
                kind: "local_folder",
                remotePrefix: "tokenviewer-sync",
                localRoot: "/tmp/device-sync"
            )
        )
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        let data = try encoder.encode(config)
        let object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: data) as? [String: Any]
        )

        XCTAssertEqual(object["profile_id"] as? String, "profile-id")
        XCTAssertEqual(
            (object["provider"] as? [String: Any])?["local_root"] as? String,
            "/tmp/device-sync"
        )
        XCTAssertNil(object["password"])
        XCTAssertNil(object["secret_key"])
        XCTAssertNil(object["session_token"])
    }

    func testDeviceSyncProviderConfigDefaultsInsecureAndIgnoresUnknownFields() throws {
        let data = Data(
            """
            {
              "kind": "webdav",
              "endpoint": "https://dav.example.com/dav/",
              "remote_prefix": "tokenviewer-sync",
              "username": "user@example.com",
              "future_provider_field": "ignored"
            }
            """.utf8
        )
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let provider = try decoder.decode(DeviceSyncProviderConfig.self, from: data)

        XCTAssertEqual(provider.kind, DeviceSyncProviderConfig.webDAVKind)
        XCTAssertEqual(provider.remotePrefix, "tokenviewer-sync")
        XCTAssertFalse(provider.insecure)
    }

    func testDeviceSyncProviderCredentialScopeChangesWithProfileAndEndpoint() {
        let provider = DeviceSyncProviderConfig(
            kind: DeviceSyncProviderConfig.webDAVKind,
            endpoint: "https://dav.example.com/dav/",
            remotePrefix: "tokenviewer-sync",
            username: "user@example.com"
        )
        let base = DeviceSyncConfig(profileId: "profile-a", provider: provider)
        let same = DeviceSyncProviderCredentialScope(config: base)
        XCTAssertEqual(same, DeviceSyncProviderCredentialScope(config: base))

        var endpointChanged = base
        endpointChanged.provider?.endpoint = "https://dav.example.com/other/"
        XCTAssertNotEqual(same, DeviceSyncProviderCredentialScope(config: endpointChanged))

        var profileChanged = base
        profileChanged.profileId = "profile-b"
        XCTAssertNotEqual(same, DeviceSyncProviderCredentialScope(config: profileChanged))

        var transportChanged = base
        transportChanged.provider?.insecure = true
        XCTAssertNotEqual(same, DeviceSyncProviderCredentialScope(config: transportChanged))
        XCTAssertNil(DeviceSyncProviderCredentialScope(config: DeviceSyncConfig(profileId: "local")))
    }

    func testDeviceSyncProviderCredentialEnvelopeAndConnectionReportDecode() throws {
        let configuredData = Data(
            """
            {"ok":true,"data":{"configured":true,"cleared":null},"error":null}
            """.utf8
        )
        let reportData = Data(
            """
            {"ok":true,"data":{"connected":true,"provider":"webdav","writable":true},"error":null}
            """.utf8
        )
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let configured = try decoder.decode(
            DeviceSyncEnvelope<DeviceSyncCredentialMutationResponse>.self,
            from: configuredData
        )
        let report = try decoder.decode(
            DeviceSyncEnvelope<DeviceSyncConnectionReport>.self,
            from: reportData
        )

        XCTAssertTrue(configured.data?.configured == true)
        XCTAssertEqual(report.data, DeviceSyncConnectionReport(connected: true))
    }

    func testDeviceSyncCredentialAccountsAreProfileAndVaultScoped() throws {
        XCTAssertEqual(
            try DeviceSyncCredentialStore.profileAccount(
                profileId: "profile-id",
                kind: .webDAVPassword
            ),
            "profile-id:webdav-password"
        )
        XCTAssertEqual(
            try DeviceSyncCredentialStore.vaultAccount(vaultId: "vault-id"),
            "vault-id:master-key"
        )
        XCTAssertThrowsError(
            try DeviceSyncCredentialStore.profileAccount(
                profileId: "profile/id",
                kind: .s3SecretKey
            )
        )
        XCTAssertThrowsError(
            try DeviceSyncCredentialStore.vaultAccount(vaultId: "vault:id")
        )
        XCTAssertThrowsError(
            try DeviceSyncCredentialStore.profileAccount(
                profileId: "profile\n id",
                kind: .webDAVPassword
            )
        )
        XCTAssertThrowsError(
            try DeviceSyncCredentialStore.profileAccount(
                profileId: "profile\u{7f}id",
                kind: .webDAVPassword
            )
        )
    }

    func testDeviceSyncNutstorePresetAndHTTPStatusLocalization() {
        XCTAssertEqual(
            DeviceSyncProviderPreset.nutstore.defaultEndpoint,
            "https://dav.jianguoyun.com/dav/"
        )
        XCTAssertEqual(
            DeviceSyncProviderPreset.koofr.defaultEndpoint,
            "https://app.koofr.net/dav/Koofr/"
        )
        XCTAssertNil(DeviceSyncProviderPreset.synology.defaultEndpoint)
        XCTAssertNil(DeviceSyncProviderPreset.nextcloud.defaultEndpoint)
        XCTAssertEqual(
            DeviceSyncProviderPreset.detect(endpoint: "https://nas.example.com:5006/"),
            .synology
        )
        XCTAssertEqual(
            DeviceSyncProviderPreset.detect(endpoint: "https://cloud.example.com/remote.php/dav/files/alice/"),
            .nextcloud
        )
        XCTAssertEqual(
            DeviceSyncProviderPreset.detect(endpoint: "https://dav.example.com/custom/"),
            .customWebDAV
        )
        let originalLanguage = L10n.shared.language
        defer { L10n.shared.language = originalLanguage }

        L10n.shared.language = .zh
        XCTAssertEqual(L10n.shared.deviceSyncHTTPStatus("412"), "HTTP 状态：412")
        XCTAssertEqual(L10n.shared.deviceSyncOperation("propfind"), "请求操作：propfind")
        L10n.shared.language = .en
        XCTAssertEqual(L10n.shared.deviceSyncHTTPStatus("412"), "HTTP status: 412")
        XCTAssertEqual(L10n.shared.deviceSyncOperation("propfind"), "Request operation: propfind")
    }

    func testDeviceSyncPreviewItemUsesStableIdentity() {
        let item = DeviceSyncPreviewItem(
            component: "skills",
            action: "update",
            id: "example",
            detail: nil,
            destructive: true
        )
        XCTAssertEqual(item.id, "example")
        XCTAssertEqual(item.stableId, "skills:update:example")
    }

    func testSessionGateBlocksConcurrentAndRecoveryOperationsUntilRecoverySucceeds() throws {
        let gate = DeviceSyncSessionGate()
        let first = try gate.beginOperation()
        XCTAssertTrue(gate.isOperationInProgress)

        XCTAssertThrowsError(try gate.beginOperation()) { error in
            XCTAssertEqual((error as? DeviceSyncApplyError)?.code, "operation_in_progress")
        }
        gate.endOperation(first)

        gate.markRecoveryBlocked(
            operationId: "tx-blocked",
            recoveryPath: "/tmp/recovery/tx-blocked",
            code: "rollback_failed"
        )
        XCTAssertThrowsError(try gate.beginOperation()) { error in
            XCTAssertEqual((error as? DeviceSyncApplyError)?.code, "recovery_blocked")
        }

        let recovery = try gate.beginRecovery()
        XCTAssertEqual(gate.recoveryState, .recovering)
        XCTAssertTrue(gate.finishRecovery(recovery, clearBlock: true))
        XCTAssertFalse(gate.isRecoveryBlocked)
        XCTAssertEqual(gate.recoveryState, .ready)
    }

    func testObservedStatusPublishesRecoveryBlockAndProfile() throws {
        let coordinator = DeviceSyncApplyCoordinator(
            core: FakeDeviceSyncApplyCore(),
            preferenceStore: FakeDeviceSyncPreferenceStore(),
            journalDirectory: FileManager.default.temporaryDirectory
                .appendingPathComponent("TokenViewerDeviceSyncTests-(UUID().uuidString)")
        )
        let status = makeStatus(profileId: "observed-profile", recoveryBlocked: true)

        coordinator.observeStatus(status)

        XCTAssertEqual(coordinator.sessionGate.profileId, "observed-profile")
        XCTAssertEqual(coordinator.recoveryState, .blocked(
            operationId: nil,
            recoveryPath: nil,
            code: "recovery_blocked"
        ))
        XCTAssertEqual(coordinator.state, .recoveryBlocked(
            operationId: nil,
            recoveryPath: nil,
            code: "recovery_blocked"
        ))
        let generation = coordinator.sessionGate.generation

        coordinator.observeStatus(status)

        XCTAssertEqual(coordinator.sessionGate.generation, generation)
    }

    func testObservedRecoveryBlockRejectsDeviceSyncOperationsUntilRecovery() async throws {
        let coordinator = DeviceSyncApplyCoordinator(
            core: FakeDeviceSyncApplyCore(),
            preferenceStore: FakeDeviceSyncPreferenceStore(),
            journalDirectory: FileManager.default.temporaryDirectory
                .appendingPathComponent("TokenViewerDeviceSyncTests-(UUID().uuidString)")
        )
        coordinator.observeStatus(makeStatus(profileId: "blocked-profile", recoveryBlocked: true))

        await XCTAssertThrowsErrorAsync(
            try await coordinator.runDeviceSyncOperation { XCTFail("blocked operation ran") }
        ) { error in
            XCTAssertEqual((error as? DeviceSyncApplyError)?.code, "recovery_blocked")
        }
    }

    func testStaleGenerationFailureCannotOverwriteNewProfileState() async throws {
        let coordinator = DeviceSyncApplyCoordinator(
            core: FakeDeviceSyncApplyCore(),
            preferenceStore: FakeDeviceSyncPreferenceStore(),
            journalDirectory: FileManager.default.temporaryDirectory
                .appendingPathComponent("TokenViewerDeviceSyncTests-(UUID().uuidString)")
        )
        let oldError = bridgeError(code: "rollback_failed", operationId: "tx-old")
        let operation = Task { @MainActor in
            await XCTAssertThrowsErrorAsync(
                try await coordinator.runDeviceSyncOperation {
                    try await Task.sleep(nanoseconds: 100_000_000)
                    throw oldError
                }
            )
        }

        try await Task.sleep(nanoseconds: 10_000_000)
        coordinator.updateProfile("new-profile")
        try await operation.value

        XCTAssertEqual(coordinator.sessionGate.profileId, "new-profile")
        XCTAssertEqual(coordinator.recoveryState, .ready)
        XCTAssertEqual(coordinator.state, .idle)
    }

    func testDeviceSyncOperationPreservesStructuredBridgeError() async throws {
        let coordinator = DeviceSyncApplyCoordinator(
            core: FakeDeviceSyncApplyCore(),
            preferenceStore: FakeDeviceSyncPreferenceStore(),
            journalDirectory: FileManager.default.temporaryDirectory
                .appendingPathComponent("TokenViewerDeviceSyncTests-\(UUID().uuidString)")
        )
        let payload = DeviceSyncErrorPayload(
            code: "authentication_failed",
            messageKey: "deviceSync.error.authenticationFailed",
            arguments: ["http_status": "401"],
            retryable: false,
            operationId: nil
        )

        await XCTAssertThrowsErrorAsync(
            try await coordinator.runDeviceSyncOperation {
                throw DeviceSyncBridgeError.core(payload)
            }
        ) { error in
            XCTAssertEqual(error as? DeviceSyncBridgeError, .core(payload))
        }
    }

    func testKeychainRetrySucceedsAfterTransientFailure() throws {
        var attempts = 0
        var delays: [UInt64] = []
        let value = try DeviceSyncCredentialStore.retrying(
            maxAttempts: 3,
            sleep: { delays.append($0) }
        ) {
            attempts += 1
            if attempts < 3 {
                throw DeviceSyncCredentialError.keychain(status: errSecInteractionNotAllowed)
            }
            return "repaired-credential"
        }

        XCTAssertEqual(value, "repaired-credential")
        XCTAssertEqual(attempts, 3)
        XCTAssertEqual(delays, [50_000_000, 200_000_000])
    }

    func testKeychainRetryDoesNotRetryPermanentFailure() {
        var attempts = 0

        XCTAssertThrowsError(
            try DeviceSyncCredentialStore.retrying(
                maxAttempts: 3,
                sleep: { _ in }
            ) {
                attempts += 1
                throw DeviceSyncCredentialError.keychain(status: errSecParam)
            }
        ) { error in
            XCTAssertEqual(
                error as? DeviceSyncCredentialError,
                .keychain(status: errSecParam)
            )
        }

        XCTAssertEqual(attempts, 1)
    }

    func testApplyCommitsPreferencesAndCleansJournal() async throws {
        let journalDirectory = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: journalDirectory) }
        let core = FakeDeviceSyncApplyCore()
        core.prepareResponse = preparedResponse(transactionId: "tx-success")
        let preferences = FakeDeviceSyncPreferenceStore(values: [
            DeviceSyncApplyCoordinator.preferenceKey: "[\"local\"]",
        ])
        let coordinator = DeviceSyncApplyCoordinator(
            core: core,
            preferenceStore: preferences,
            journalDirectory: journalDirectory
        )

        let result = try await coordinator.apply(previewToken: "preview")

        XCTAssertEqual(result.operationId, "operation")
        XCTAssertEqual(preferences.values[DeviceSyncApplyCoordinator.preferenceKey], "[\"remote\"]")
        XCTAssertEqual(core.prepareCallCount, 1)
        XCTAssertEqual(core.commitCallCount, 1)
        XCTAssertEqual(core.rollbackCallCount, 0)
        XCTAssertEqual(core.finalizeCallCount, 1)
        XCTAssertEqual(coordinator.state, .idle)
        XCTAssertFalse(FileManager.default.fileExists(atPath: coordinator.journalURL(for: "tx-success").path))
    }

    func testPreferenceWriteFailureRollsBackRustAndRestoresOldValue() async throws {
        let journalDirectory = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: journalDirectory) }
        let core = FakeDeviceSyncApplyCore()
        core.prepareResponse = preparedResponse(transactionId: "tx-preference")
        let preferences = FakeDeviceSyncPreferenceStore(values: [
            DeviceSyncApplyCoordinator.preferenceKey: "[\"local\"]",
        ])
        preferences.failNextSet = true
        let coordinator = DeviceSyncApplyCoordinator(
            core: core,
            preferenceStore: preferences,
            journalDirectory: journalDirectory
        )

        await XCTAssertThrowsErrorAsync(try await coordinator.apply(previewToken: "preview")) { error in
            let failure = error as? DeviceSyncApplyError
            XCTAssertEqual(failure?.code, "apply_failed")
            XCTAssertEqual(failure?.operationId, "tx-preference")
        }

        XCTAssertEqual(preferences.values[DeviceSyncApplyCoordinator.preferenceKey], "[\"local\"]")
        XCTAssertEqual(core.rollbackCallCount, 1)
        XCTAssertEqual(core.finalizeCallCount, 0)
        XCTAssertEqual(coordinator.state, .failed(
            operationId: "tx-preference",
            recoveryPath: preparedResponse(transactionId: "tx-preference").recoveryPath,
            code: "apply_failed"
        ))
        XCTAssertFalse(FileManager.default.fileExists(atPath: coordinator.journalURL(for: "tx-preference").path))
    }

    func testCommitFailureCallsRollbackAndRestoresOldValue() async throws {
        let journalDirectory = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: journalDirectory) }
        let core = FakeDeviceSyncApplyCore()
        core.prepareResponse = preparedResponse(transactionId: "tx-commit")
        core.commitError = bridgeError(code: "apply_failed", operationId: "tx-commit")
        let preferences = FakeDeviceSyncPreferenceStore(values: [
            DeviceSyncApplyCoordinator.preferenceKey: "[\"local\"]",
        ])
        let coordinator = DeviceSyncApplyCoordinator(
            core: core,
            preferenceStore: preferences,
            journalDirectory: journalDirectory
        )

        await XCTAssertThrowsErrorAsync(try await coordinator.apply(previewToken: "preview")) { error in
            let failure = error as? DeviceSyncApplyError
            XCTAssertEqual(failure?.code, "apply_failed")
            XCTAssertEqual(failure?.operationId, "tx-commit")
        }

        XCTAssertEqual(preferences.values[DeviceSyncApplyCoordinator.preferenceKey], "[\"local\"]")
        XCTAssertEqual(core.rollbackCallCount, 1)
        XCTAssertEqual(coordinator.state, .failed(
            operationId: "tx-commit",
            recoveryPath: preparedResponse(transactionId: "tx-commit").recoveryPath,
            code: "apply_failed"
        ))
    }

    func testRollbackFailurePreservesTransactionAndRecoveryPath() async throws {
        let journalDirectory = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: journalDirectory) }
        let core = FakeDeviceSyncApplyCore()
        core.prepareResponse = preparedResponse(transactionId: "tx-rollback")
        core.commitError = bridgeError(code: "apply_failed", operationId: "tx-rollback")
        core.rollbackError = bridgeError(code: "rollback_failed", operationId: "tx-rollback")
        let preferences = FakeDeviceSyncPreferenceStore(values: [
            DeviceSyncApplyCoordinator.preferenceKey: "[\"local\"]",
        ])
        let coordinator = DeviceSyncApplyCoordinator(
            core: core,
            preferenceStore: preferences,
            journalDirectory: journalDirectory
        )

        await XCTAssertThrowsErrorAsync(try await coordinator.apply(previewToken: "preview")) { error in
            let failure = error as? DeviceSyncApplyError
            XCTAssertEqual(failure?.code, "rollback_failed")
            XCTAssertEqual(failure?.operationId, "tx-rollback")
            XCTAssertEqual(failure?.recoveryPath, preparedResponse(transactionId: "tx-rollback").recoveryPath)
        }

        XCTAssertEqual(preferences.values[DeviceSyncApplyCoordinator.preferenceKey], "[\"remote\"]")
        XCTAssertEqual(core.rollbackCallCount, 1)
        XCTAssertTrue(FileManager.default.fileExists(atPath: coordinator.journalURL(for: "tx-rollback").path))
        let journalData = try Data(contentsOf: coordinator.journalURL(for: "tx-rollback"))
        let journal = try JSONDecoder().decode(DeviceSyncApplyJournal.self, from: journalData)
        XCTAssertEqual(journal.phase, .rollbackRequested)
        if case let .recoveryBlocked(operationId, recoveryPath, code) = coordinator.state {
            XCTAssertEqual(operationId, "tx-rollback")
            XCTAssertEqual(recoveryPath, preparedResponse(transactionId: "tx-rollback").recoveryPath)
            XCTAssertEqual(code, "rollback_failed")
        } else {
            XCTFail("Expected recovery-blocked state")
        }
    }

    func testUnknownPreferenceKeyIsRejectedBeforeWrite() async throws {
        let journalDirectory = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: journalDirectory) }
        let core = FakeDeviceSyncApplyCore()
        core.prepareResponse = preparedResponse(
            transactionId: "tx-unknown",
            mutations: [DeviceSyncPreferenceMutation(key: "arbitraryUserDefaultsKey", enabledAgentIds: ["remote"])]
        )
        let preferences = FakeDeviceSyncPreferenceStore(values: [
            DeviceSyncApplyCoordinator.preferenceKey: "[\"local\"]",
        ])
        let coordinator = DeviceSyncApplyCoordinator(
            core: core,
            preferenceStore: preferences,
            journalDirectory: journalDirectory
        )

        await XCTAssertThrowsErrorAsync(try await coordinator.apply(previewToken: "preview")) { error in
            XCTAssertEqual((error as? DeviceSyncApplyError)?.code, "unknown_preference_key")
        }

        XCTAssertEqual(preferences.values[DeviceSyncApplyCoordinator.preferenceKey], "[\"local\"]")
        XCTAssertEqual(core.rollbackCallCount, 1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: coordinator.journalURL(for: "tx-unknown").path))
    }

    func testRecoveryRestoresPreferencesForRolledBackJournal() async throws {
        let journalDirectory = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: journalDirectory) }
        let core = FakeDeviceSyncApplyCore()
        core.recoveryResponse = DeviceSyncRecoveryResponse(
            recovered: 1,
            rolledBackTransactionIds: ["tx-recover"],
            committedTransactionIds: []
        )
        let preferences = FakeDeviceSyncPreferenceStore(values: [
            DeviceSyncApplyCoordinator.preferenceKey: "[\"remote\"]",
        ])
        let coordinator = DeviceSyncApplyCoordinator(
            core: core,
            preferenceStore: preferences,
            journalDirectory: journalDirectory
        )
        try writeJournal(
            DeviceSyncApplyJournal(
                transactionId: "tx-recover",
                recoveryPath: recoveryPath(for: "tx-recover"),
                oldSkillsEnabledProviders: "[\"local\"]",
                newSkillsEnabledProviders: "[\"remote\"]",
                phase: .preferencesWritten
            ),
            to: coordinator.journalURL(for: "tx-recover")
        )

        _ = try await coordinator.recoverPendingApply()

        XCTAssertEqual(preferences.values[DeviceSyncApplyCoordinator.preferenceKey], "[\"local\"]")
        XCTAssertEqual(coordinator.state, .idle)
        XCTAssertFalse(FileManager.default.fileExists(atPath: coordinator.journalURL(for: "tx-recover").path))
    }

    func testRecoveryPreservesJournalWhenPreferenceRestoreFails() async throws {
        let journalDirectory = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: journalDirectory) }
        let core = FakeDeviceSyncApplyCore()
        core.recoveryResponse = DeviceSyncRecoveryResponse(
            recovered: 1,
            rolledBackTransactionIds: ["tx-recover-failure"],
            committedTransactionIds: []
        )
        let preferences = FakeDeviceSyncPreferenceStore(values: [
            DeviceSyncApplyCoordinator.preferenceKey: "[\"remote\"]",
        ])
        preferences.failAllSets = true
        let coordinator = DeviceSyncApplyCoordinator(
            core: core,
            preferenceStore: preferences,
            journalDirectory: journalDirectory
        )
        let journal = DeviceSyncApplyJournal(
            transactionId: "tx-recover-failure",
            recoveryPath: recoveryPath(for: "tx-recover-failure"),
            oldSkillsEnabledProviders: "[\"local\"]",
            newSkillsEnabledProviders: "[\"remote\"]",
            phase: .preferencesWritten
        )
        try writeJournal(journal, to: coordinator.journalURL(for: journal.transactionId))

        await XCTAssertThrowsErrorAsync(try await coordinator.recoverPendingApply()) { error in
            let failure = error as? DeviceSyncApplyError
            XCTAssertEqual(failure?.code, "preference_write_failed")
            XCTAssertEqual(failure?.stage, "preference_restore")
            XCTAssertEqual(failure?.operationId, journal.transactionId)
            XCTAssertEqual(failure?.recoveryPath, journal.recoveryPath)
        }

        XCTAssertTrue(coordinator.isRecoveryBlocked)
        XCTAssertTrue(FileManager.default.fileExists(atPath: coordinator.journalURL(for: journal.transactionId).path))
    }

    func testRecoveryErrorEntersRecoveryBlockedStateWithOperationId() async {
        let core = FakeDeviceSyncApplyCore()
        core.recoveryError = bridgeError(code: "recovery_failed", operationId: "tx-startup")
        let preferences = FakeDeviceSyncPreferenceStore()
        let coordinator = DeviceSyncApplyCoordinator(
            core: core,
            preferenceStore: preferences,
            journalDirectory: FileManager.default.temporaryDirectory
                .appendingPathComponent("TokenViewerDeviceSyncTests-\(UUID().uuidString)")
        )

        await XCTAssertThrowsErrorAsync(try await coordinator.recoverPendingApply())
        XCTAssertEqual(coordinator.state, .recoveryBlocked(
            operationId: "tx-startup",
            recoveryPath: nil,
            code: "recovery_failed"
        ))
        XCTAssertTrue(coordinator.suppressDeviceSyncListener)
    }

    func testSuccessfulRetryRecoveryClearsBlockAndAllowsNewOperation() async throws {
        let journalDirectory = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: journalDirectory) }
        let coordinator = DeviceSyncApplyCoordinator(
            core: FakeDeviceSyncApplyCore(),
            preferenceStore: FakeDeviceSyncPreferenceStore(),
            journalDirectory: journalDirectory
        )
        coordinator.markRecoveryBlocked(
            bridgeError(code: "rollback_failed", operationId: "tx-retry")
        )

        _ = try await coordinator.retryRecovery()
        XCTAssertFalse(coordinator.isRecoveryBlocked)
        XCTAssertEqual(coordinator.recoveryState, .ready)
        XCTAssertEqual(coordinator.state, .idle)
        let value = try await coordinator.runDeviceSyncOperation { 42 }
        XCTAssertEqual(value, 42)
    }

    func testTemporaryRecoveryCredentialFailureCanBeRetriedAfterCredentialsRecover() async throws {
        let journalDirectory = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: journalDirectory) }
        let core = FakeDeviceSyncApplyCore()
        core.restoreResult = false
        let coordinator = DeviceSyncApplyCoordinator(
            core: core,
            preferenceStore: FakeDeviceSyncPreferenceStore(),
            journalDirectory: journalDirectory
        )
        coordinator.markRecoveryBlocked(
            bridgeError(code: "recovery_failed", operationId: "tx-keychain-retry")
        )

        await XCTAssertThrowsErrorAsync(try await coordinator.retryRecovery()) { error in
            XCTAssertEqual((error as? DeviceSyncApplyError)?.code, "recovery_blocked")
        }
        XCTAssertTrue(coordinator.isRecoveryBlocked)

        core.restoreResult = true
        _ = try await coordinator.retryRecovery()

        XCTAssertFalse(coordinator.isRecoveryBlocked)
        XCTAssertEqual(coordinator.recoveryState, .ready)
    }

    func testSuccessfulRustRecoveryDoesNotClearAnExternalBlock() async throws {
        let journalDirectory = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: journalDirectory) }
        let coordinator = DeviceSyncApplyCoordinator(
            core: FakeDeviceSyncApplyCore(),
            preferenceStore: FakeDeviceSyncPreferenceStore(),
            journalDirectory: journalDirectory
        )
        coordinator.markRecoveryBlocked(
            bridgeError(code: "keychain_unavailable", operationId: "tx-keychain")
        )

        _ = try await coordinator.recoverPendingApply()
        XCTAssertTrue(coordinator.isRecoveryBlocked)
        XCTAssertEqual(coordinator.state, .recoveryBlocked(
            operationId: "tx-keychain",
            recoveryPath: nil,
            code: "keychain_unavailable"
        ))
        await XCTAssertThrowsErrorAsync(
            try await coordinator.runDeviceSyncOperation { XCTFail("blocked operation ran") }
        ) { error in
            XCTAssertEqual((error as? DeviceSyncApplyError)?.code, "recovery_blocked")
        }
    }

    func testOldGenerationApplyCannotOverwriteRecoveryBlock() async throws {
        let journalDirectory = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: journalDirectory) }
        let core = FakeDeviceSyncApplyCore()
        core.prepareResponse = preparedResponse(transactionId: "tx-generation")
        core.commitDelayNanoseconds = 150_000_000
        let coordinator = DeviceSyncApplyCoordinator(
            core: core,
            preferenceStore: FakeDeviceSyncPreferenceStore(),
            journalDirectory: journalDirectory
        )

        let applyTask = Task { @MainActor in
            try? await coordinator.apply(previewToken: "preview")
        }
        try await Task.sleep(nanoseconds: 20_000_000)
        coordinator.markRecoveryBlocked(
            bridgeError(code: "rollback_failed", operationId: "tx-new-state")
        )
        _ = await applyTask.value

        XCTAssertEqual(coordinator.state, .recoveryBlocked(
            operationId: "tx-new-state",
            recoveryPath: nil,
            code: "rollback_failed"
        ))
    }

    func testCommittedRecoveryCleansJournalWithoutRestoringRemotePreference() async throws {
        let journalDirectory = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: journalDirectory) }
        let core = FakeDeviceSyncApplyCore()
        core.recoveryResponse = DeviceSyncRecoveryResponse(
            recovered: 1,
            rolledBackTransactionIds: [],
            committedTransactionIds: ["tx-committed"]
        )
        let preferences = FakeDeviceSyncPreferenceStore(values: [
            DeviceSyncApplyCoordinator.preferenceKey: "[\"remote\"]",
        ])
        let coordinator = DeviceSyncApplyCoordinator(
            core: core,
            preferenceStore: preferences,
            journalDirectory: journalDirectory
        )
        let journal = DeviceSyncApplyJournal(
            transactionId: "tx-committed",
            recoveryPath: recoveryPath(for: "tx-committed"),
            oldSkillsEnabledProviders: "[\"local\"]",
            newSkillsEnabledProviders: "[\"remote\"]",
            phase: .rustCommitted
        )
        try writeJournal(journal, to: coordinator.journalURL(for: journal.transactionId))

        _ = try await coordinator.recoverPendingApply()

        XCTAssertEqual(preferences.values[DeviceSyncApplyCoordinator.preferenceKey], "[\"remote\"]")
        XCTAssertFalse(FileManager.default.fileExists(atPath: coordinator.journalURL(for: journal.transactionId).path))
    }

    private func makeTemporaryDirectory() throws -> URL {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("TokenViewerDeviceSyncTests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory
    }

    private func preparedResponse(
        transactionId: String,
        mutations: [DeviceSyncPreferenceMutation] = [
            DeviceSyncPreferenceMutation(key: DeviceSyncApplyCoordinator.preferenceKey, enabledAgentIds: ["remote"]),
        ]
    ) -> DeviceSyncPrepareApplyResponse {
        DeviceSyncPrepareApplyResponse(
            transactionId: transactionId,
            preferenceMutations: mutations,
            recoveryPath: recoveryPath(for: transactionId)
        )
    }

    private func makeStatus(profileId: String, recoveryBlocked: Bool) -> DeviceSyncStatus {
        DeviceSyncStatus(
            config: DeviceSyncConfig(profileId: profileId),
            identity: DeviceSyncIdentity(
                schemaVersion: 1,
                deviceId: "device-id",
                displayName: "Test Device",
                createdAt: "2026-09-01T00:00:00.000Z"
            ),
            state: DeviceSyncState(
                schemaVersion: 1,
                appliedSnapshotId: nil,
                localSequence: 0,
                maxRemoteSequences: [:],
                seenSnapshots: [],
                lastResult: nil,
                pendingContentSource: nil
            ),
            remoteHeadFingerprint: nil,
            frontier: [],
            recoveryBlocked: recoveryBlocked
        )
    }

    private func recoveryPath(for transactionId: String) -> String {
        "/tmp/tokenviewer-device-sync-recovery/\(transactionId)"
    }

    private func bridgeError(code: String, operationId: String) -> DeviceSyncBridgeError {
        .core(DeviceSyncErrorPayload(
            code: code,
            messageKey: "deviceSync.error.\(code)",
            arguments: [:],
            retryable: false,
            operationId: operationId
        ))
    }

    private func writeJournal(_ journal: DeviceSyncApplyJournal, to url: URL) throws {
        try JSONEncoder().encode(journal).write(to: url, options: .atomic)
    }
}

private final class FakeDeviceSyncApplyCore: DeviceSyncApplyCore, @unchecked Sendable {
    var prepareResponse = DeviceSyncPrepareApplyResponse(
        transactionId: "tx-default",
        preferenceMutations: [],
        recoveryPath: nil
    )
    var commitResponse = DeviceSyncResult(
        direction: "pull",
        snapshotId: "snapshot",
        operationId: "operation",
        warnings: []
    )
    var recoveryResponse = DeviceSyncRecoveryResponse(recovered: 0)
    var commitError: Error?
    var rollbackError: Error?
    var finalizeError: Error?
    var recoveryError: Error?
    var restoreResult = true
    var restoreError: Error?
    private(set) var prepareCallCount = 0
    private(set) var commitCallCount = 0
    private(set) var rollbackCallCount = 0
    private(set) var finalizeCallCount = 0
    var commitDelayNanoseconds: UInt64 = 0

    func deviceSyncRawPrepareApply(previewToken: String) async throws -> DeviceSyncPrepareApplyResponse {
        prepareCallCount += 1
        return prepareResponse
    }

    func deviceSyncRawCommitApply(transactionId: String) async throws -> DeviceSyncResult {
        commitCallCount += 1
        if let commitError { throw commitError }
        if commitDelayNanoseconds > 0 {
            try await Task.sleep(nanoseconds: commitDelayNanoseconds)
        }
        return commitResponse
    }

    func deviceSyncRawRollbackApply(transactionId: String) async throws -> DeviceSyncRollbackResponse {
        rollbackCallCount += 1
        if let rollbackError { throw rollbackError }
        return DeviceSyncRollbackResponse(rolledBack: true)
    }

    func deviceSyncRawFinalizeApply(transactionId: String) async throws -> DeviceSyncFinalizeResponse {
        finalizeCallCount += 1
        if let finalizeError { throw finalizeError }
        return DeviceSyncFinalizeResponse(finalized: true)
    }

    func deviceSyncRawRecoverPendingApply() async throws -> DeviceSyncRecoveryResponse {
        if let recoveryError { throw recoveryError }
        return recoveryResponse
    }

    func deviceSyncRawRestoreMasterKeyIfAvailable() async throws -> Bool {
        if let restoreError { throw restoreError }
        return restoreResult
    }
}

private final class FakeDeviceSyncPreferenceStore: DeviceSyncPreferenceStore, @unchecked Sendable {
    var values: [String: String]
    var failNextSet = false
    var failAllSets = false

    init(values: [String: String] = [:]) {
        self.values = values
    }

    func value(forKey key: String) throws -> String? {
        values[key]
    }

    func setValue(_ value: String?, forKey key: String) throws {
        if failAllSets || failNextSet {
            failNextSet = false
            throw FakeDeviceSyncPreferenceError.writeFailed
        }
        if let value {
            values[key] = value
        } else {
            values.removeValue(forKey: key)
        }
    }
}

private enum FakeDeviceSyncPreferenceError: Error {
    case writeFailed
}

@MainActor
private func XCTAssertThrowsErrorAsync<T>(
    _ expression: @autoclosure () async throws -> T,
    _ errorHandler: (Error) -> Void = { _ in }
) async {
    do {
        _ = try await expression()
        XCTFail("Expected async expression to throw")
    } catch {
        errorHandler(error)
    }
}
