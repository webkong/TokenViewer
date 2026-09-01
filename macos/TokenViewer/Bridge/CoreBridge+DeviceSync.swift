import Foundation

extension CoreBridge {
    func deviceSyncGetConfig() async throws -> DeviceSyncStatus {
        let status = try await deviceSyncRawGetConfig()
        await DeviceSyncApplyCoordinator.shared.observeStatus(status)
        return status
    }

    func deviceSyncGetStatus() async throws -> DeviceSyncStatus {
        let status = try await deviceSyncRawGetStatus()
        await DeviceSyncApplyCoordinator.shared.observeStatus(status)
        return status
    }

    func deviceSyncSetConfig(_ config: DeviceSyncConfig) async throws -> DeviceSyncConfig {
        let result = try await DeviceSyncApplyCoordinator.shared.runDeviceSyncOperation { [self] in
            try await deviceSyncRawSetConfig(config)
        }
        await DeviceSyncApplyCoordinator.shared.updateProfile(config.profileId)
        return result
    }

    func deviceSyncTestConnection() async throws -> DeviceSyncConnectionReport {
        try await DeviceSyncApplyCoordinator.shared.runDeviceSyncOperation { [self] in
            try await deviceSyncRawTestConnection()
        }
    }

    func deviceSyncCreateVault(password: String) async throws -> DeviceSyncStatus {
        let status = try await DeviceSyncApplyCoordinator.shared.runDeviceSyncOperation { [self] in
            let session = try await deviceSyncRawCreateVault(password: password)
            try await persistMasterKey(from: session)
            return session.status
        }
        await DeviceSyncApplyCoordinator.shared.observeStatus(status)
        return status
    }

    func deviceSyncJoinVault(password: String) async throws -> DeviceSyncStatus {
        let status = try await DeviceSyncApplyCoordinator.shared.runDeviceSyncOperation { [self] in
            let session = try await deviceSyncRawJoinVault(password: password)
            try await persistMasterKey(from: session)
            return session.status
        }
        await DeviceSyncApplyCoordinator.shared.observeStatus(status)
        return status
    }

    @discardableResult
    func deviceSyncRestoreMasterKeyIfAvailable() async throws -> Bool {
        try await DeviceSyncApplyCoordinator.shared.restoreMasterKeyIfAvailable()
    }

    func deviceSyncPreviewPush(enabledAgentIds: [String]) async throws -> DeviceSyncPreviewResponse {
        try await DeviceSyncApplyCoordinator.shared.runDeviceSyncOperation { [self] in
            try await deviceSyncRawPreviewPush(enabledAgentIds: enabledAgentIds)
        }
    }

    func deviceSyncPush(
        previewToken: String,
        enabledAgentIds: [String]
    ) async throws -> DeviceSyncResult {
        try await DeviceSyncApplyCoordinator.shared.runDeviceSyncOperation { [self] in
            try await deviceSyncRawPush(
                previewToken: previewToken,
                enabledAgentIds: enabledAgentIds
            )
        }
    }

    func deviceSyncPreviewPull(enabledAgentIds: [String]) async throws -> DeviceSyncPreviewResponse {
        try await DeviceSyncApplyCoordinator.shared.runDeviceSyncOperation { [self] in
            try await deviceSyncRawPreviewPull(enabledAgentIds: enabledAgentIds)
        }
    }

    func deviceSyncPrepareApply(previewToken: String) async throws -> DeviceSyncPrepareApplyResponse {
        try await DeviceSyncApplyCoordinator.shared.runDeviceSyncOperation { [self] in
            try await deviceSyncRawPrepareApply(previewToken: previewToken)
        }
    }

    func deviceSyncCommitApply(transactionId: String) async throws -> DeviceSyncResult {
        try await DeviceSyncApplyCoordinator.shared.runDeviceSyncOperation { [self] in
            try await deviceSyncRawCommitApply(transactionId: transactionId)
        }
    }

    func deviceSyncRollbackApply(transactionId: String) async throws -> DeviceSyncRollbackResponse {
        try await DeviceSyncApplyCoordinator.shared.runDeviceSyncOperation { [self] in
            try await deviceSyncRawRollbackApply(transactionId: transactionId)
        }
    }

    func deviceSyncFinalizeApply(transactionId: String) async throws -> DeviceSyncFinalizeResponse {
        try await DeviceSyncApplyCoordinator.shared.runDeviceSyncOperation { [self] in
            try await deviceSyncRawFinalizeApply(transactionId: transactionId)
        }
    }

    func deviceSyncRecoverPendingApply() async throws -> DeviceSyncRecoveryResponse {
        try await DeviceSyncApplyCoordinator.shared.recoverPendingApply()
    }

    func deviceSyncListSnapshots() async throws -> [DeviceSyncSnapshotListItem] {
        try await DeviceSyncApplyCoordinator.shared.runDeviceSyncOperation { [self] in
            try await deviceSyncRawListSnapshots()
        }
    }

    // MARK: Raw calls used by DeviceSyncApplyWorker

    func deviceSyncRawGetConfig() async throws -> DeviceSyncStatus {
        try decodeDeviceSync(
            await callAsync { tt_device_sync_get_config($0) },
            as: DeviceSyncStatus.self
        )
    }

    func deviceSyncRawGetStatus() async throws -> DeviceSyncStatus {
        try decodeDeviceSync(
            await callAsync { tt_device_sync_get_status($0) },
            as: DeviceSyncStatus.self
        )
    }

    func deviceSyncRawSetConfig(_ config: DeviceSyncConfig) async throws -> DeviceSyncConfig {
        try decodeDeviceSync(
            await callDeviceSyncJSON(config) { tt_device_sync_set_config($0, $1) },
            as: DeviceSyncConfig.self
        )
    }

    func deviceSyncRawTestConnection() async throws -> DeviceSyncConnectionReport {
        try decodeDeviceSync(
            await callAsync { tt_device_sync_test_connection($0) },
            as: DeviceSyncConnectionReport.self
        )
    }

    func deviceSyncRawCreateVault(password: String) async throws -> DeviceSyncVaultSession {
        try decodeDeviceSync(
            await callDeviceSyncJSON(DeviceSyncPasswordRequest(password: password)) {
                tt_device_sync_create_vault($0, $1)
            },
            as: DeviceSyncVaultSession.self
        )
    }

    func deviceSyncRawJoinVault(password: String) async throws -> DeviceSyncVaultSession {
        try decodeDeviceSync(
            await callDeviceSyncJSON(DeviceSyncPasswordRequest(password: password)) {
                tt_device_sync_join_vault($0, $1)
            },
            as: DeviceSyncVaultSession.self
        )
    }

    func deviceSyncRawRestoreMasterKeyIfAvailable() async throws -> Bool {
        let status = try await deviceSyncRawGetStatus()
        guard let vaultId = status.config.vaultId else { return false }
        guard let key = try await DeviceSyncCredentialStore.shared.masterKeyAsync(vaultId: vaultId) else {
            return false
        }
        let request = DeviceSyncMasterKeyRequest(masterKeyB64: key.base64EncodedString())
        _ = try decodeDeviceSync(
            await callDeviceSyncJSON(request) { tt_device_sync_set_master_key($0, $1) },
            as: DeviceSyncRestoreResponse.self
        )
        return true
    }

    func deviceSyncRawPreviewPush(enabledAgentIds: [String]) async throws -> DeviceSyncPreviewResponse {
        try decodeDeviceSync(
            await callDeviceSyncJSON(DeviceSyncPreviewRequest(enabledAgentIds: enabledAgentIds)) {
                tt_device_sync_preview_push($0, $1)
            },
            as: DeviceSyncPreviewResponse.self
        )
    }

    func deviceSyncRawPush(
        previewToken: String,
        enabledAgentIds: [String]
    ) async throws -> DeviceSyncResult {
        try decodeDeviceSync(
            await callDeviceSyncJSON(
                DeviceSyncTokenRequest(
                    previewToken: previewToken,
                    enabledAgentIds: enabledAgentIds
                )
            ) { tt_device_sync_push($0, $1) },
            as: DeviceSyncResult.self
        )
    }

    func deviceSyncRawPreviewPull(enabledAgentIds: [String]) async throws -> DeviceSyncPreviewResponse {
        try decodeDeviceSync(
            await callDeviceSyncJSON(DeviceSyncPreviewRequest(enabledAgentIds: enabledAgentIds)) {
                tt_device_sync_preview_pull($0, $1)
            },
            as: DeviceSyncPreviewResponse.self
        )
    }

    func deviceSyncRawPrepareApply(previewToken: String) async throws -> DeviceSyncPrepareApplyResponse {
        try decodeDeviceSync(
            await callDeviceSyncJSON(
                DeviceSyncTokenRequest(previewToken: previewToken, enabledAgentIds: [])
            ) { tt_device_sync_prepare_apply($0, $1) },
            as: DeviceSyncPrepareApplyResponse.self
        )
    }

    func deviceSyncRawCommitApply(transactionId: String) async throws -> DeviceSyncResult {
        try decodeDeviceSync(
            await callDeviceSyncJSON(DeviceSyncTransactionRequest(transactionId: transactionId)) {
                tt_device_sync_commit_apply($0, $1)
            },
            as: DeviceSyncResult.self
        )
    }

    func deviceSyncRawRollbackApply(transactionId: String) async throws -> DeviceSyncRollbackResponse {
        try decodeDeviceSync(
            await callDeviceSyncJSON(DeviceSyncTransactionRequest(transactionId: transactionId)) {
                tt_device_sync_rollback_apply($0, $1)
            },
            as: DeviceSyncRollbackResponse.self
        )
    }

    func deviceSyncRawFinalizeApply(transactionId: String) async throws -> DeviceSyncFinalizeResponse {
        try decodeDeviceSync(
            await callDeviceSyncJSON(DeviceSyncTransactionRequest(transactionId: transactionId)) {
                tt_device_sync_finalize_apply($0, $1)
            },
            as: DeviceSyncFinalizeResponse.self
        )
    }

    func deviceSyncRawRecoverPendingApply() async throws -> DeviceSyncRecoveryResponse {
        try decodeDeviceSync(
            await callAsync { tt_device_sync_recover_pending_apply($0) },
            as: DeviceSyncRecoveryResponse.self
        )
    }

    func deviceSyncRawListSnapshots() async throws -> [DeviceSyncSnapshotListItem] {
        try decodeDeviceSync(
            await callAsync { tt_device_sync_list_snapshots($0) },
            as: [DeviceSyncSnapshotListItem].self
        )
    }

    private func callDeviceSyncJSON<Request: Encodable>(
        _ request: Request,
        _ body: @escaping @Sendable (OpaquePointer, UnsafePointer<CChar>) -> UnsafeMutablePointer<CChar>?
    ) async throws -> Data {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        let payload = try encoder.encode(request)
        guard let json = String(data: payload, encoding: .utf8) else {
            throw DeviceSyncBridgeError.invalidResponse
        }
        guard let data = await callAsync({ handle in
            json.withCString { body(handle, $0) }
        }) else {
            throw DeviceSyncBridgeError.invalidResponse
        }
        return data
    }

    private func persistMasterKey(from session: DeviceSyncVaultSession) async throws {
        guard let vaultId = session.status.config.vaultId,
              let key = Data(base64Encoded: session.masterKeyB64),
              key.count == 32 else {
            throw DeviceSyncBridgeError.invalidResponse
        }
        try await DeviceSyncCredentialStore.shared.saveMasterKeyAsync(key, vaultId: vaultId)
    }

    private func decodeDeviceSync<Payload: Codable>(
        _ data: Data?,
        as: Payload.Type
    ) throws -> Payload {
        guard let data else { throw DeviceSyncBridgeError.invalidResponse }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let envelope: DeviceSyncEnvelope<Payload>
        do {
            envelope = try decoder.decode(DeviceSyncEnvelope<Payload>.self, from: data)
        } catch {
            throw DeviceSyncBridgeError.invalidResponse
        }
        guard envelope.ok, let payload = envelope.data else {
            throw envelope.error.map(DeviceSyncBridgeError.core)
                ?? DeviceSyncBridgeError.invalidResponse
        }
        return payload
    }
}

enum DeviceSyncBridgeError: LocalizedError, Equatable, Sendable {
    case invalidResponse
    case core(DeviceSyncErrorPayload)

    var errorDescription: String? {
        switch self {
        case .invalidResponse:
            return "Invalid Device Sync response"
        case let .core(error):
            return error.messageKey
        }
    }

    var code: String? {
        guard case let .core(error) = self else { return nil }
        return error.code
    }

    var operationId: String? {
        guard case let .core(error) = self else { return nil }
        return error.operationId
    }
}
