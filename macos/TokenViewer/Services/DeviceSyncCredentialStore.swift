import Foundation
import Security

enum DeviceSyncSecretKind: String, CaseIterable, Sendable {
    case webDAVPassword = "webdav-password"
    case s3SecretKey = "s3-secret-key"
    case s3SessionToken = "s3-session-token"
}

final class DeviceSyncCredentialStore: @unchecked Sendable {
    static let shared = DeviceSyncCredentialStore()

    static let service = "com.tokenviewer.device-sync"

    private static let retryDelaysNanoseconds: [UInt64] = [50_000_000, 200_000_000]

    /// Keychain access can briefly fail while the keychain is locked or the
    /// security daemon is unavailable. Retry only those OSStatus values; a
    /// bad identifier or permission failure must remain immediately visible.
    static let transientKeychainStatuses: Set<OSStatus> = [
        errSecNotAvailable,
        errSecInteractionNotAllowed,
    ]

    private init() {}

    func saveProfileSecret(
        _ value: String,
        profileId: String,
        kind: DeviceSyncSecretKind
    ) throws {
        guard !value.isEmpty else { throw DeviceSyncCredentialError.emptyValue }
        let account = try profileAccount(profileId: profileId, kind: kind)
        try save(data: Data(value.utf8), account: account)
    }

    func saveProfileSecretAsync(
        _ value: String,
        profileId: String,
        kind: DeviceSyncSecretKind
    ) async throws {
        try await Task.detached(priority: .userInitiated) { [self] in
            try saveProfileSecret(value, profileId: profileId, kind: kind)
        }.value
    }

    func profileSecret(
        profileId: String,
        kind: DeviceSyncSecretKind
    ) throws -> String? {
        let account = try profileAccount(profileId: profileId, kind: kind)
        guard let data = try read(account: account) else { return nil }
        return String(data: data, encoding: .utf8)
    }

    func profileSecretAsync(
        profileId: String,
        kind: DeviceSyncSecretKind
    ) async throws -> String? {
        try await Task.detached(priority: .userInitiated) { [self] in
            try profileSecret(profileId: profileId, kind: kind)
        }.value
    }

    func deleteProfileSecret(
        profileId: String,
        kind: DeviceSyncSecretKind
    ) throws {
        let account = try profileAccount(profileId: profileId, kind: kind)
        try delete(account: account)
    }

    func deleteProfileSecretAsync(
        profileId: String,
        kind: DeviceSyncSecretKind
    ) async throws {
        try await Task.detached(priority: .userInitiated) { [self] in
            try deleteProfileSecret(profileId: profileId, kind: kind)
        }.value
    }

    func saveMasterKey(_ key: Data, vaultId: String) throws {
        guard key.count == 32 else { throw DeviceSyncCredentialError.invalidMasterKey }
        try save(data: key, account: try vaultAccount(vaultId: vaultId))
    }

    func saveMasterKeyAsync(_ key: Data, vaultId: String) async throws {
        try await Task.detached(priority: .userInitiated) { [self] in
            try saveMasterKey(key, vaultId: vaultId)
        }.value
    }

    func masterKey(vaultId: String) throws -> Data? {
        try read(account: try vaultAccount(vaultId: vaultId))
    }

    func masterKeyAsync(vaultId: String) async throws -> Data? {
        try await Task.detached(priority: .userInitiated) { [self] in
            try masterKey(vaultId: vaultId)
        }.value
    }

    func deleteMasterKey(vaultId: String) throws {
        try delete(account: try vaultAccount(vaultId: vaultId))
    }

    func deleteMasterKeyAsync(vaultId: String) async throws {
        try await Task.detached(priority: .userInitiated) { [self] in
            try deleteMasterKey(vaultId: vaultId)
        }.value
    }

    static func profileAccount(profileId: String, kind: DeviceSyncSecretKind) throws -> String {
        guard isSafeIdentifier(profileId) else {
            throw DeviceSyncCredentialError.invalidIdentifier
        }
        return "\(profileId):\(kind.rawValue)"
    }

    static func vaultAccount(vaultId: String) throws -> String {
        guard isSafeIdentifier(vaultId) else {
            throw DeviceSyncCredentialError.invalidIdentifier
        }
        return "\(vaultId):master-key"
    }

    private func profileAccount(profileId: String, kind: DeviceSyncSecretKind) throws -> String {
        try Self.profileAccount(profileId: profileId, kind: kind)
    }

    private func vaultAccount(vaultId: String) throws -> String {
        try Self.vaultAccount(vaultId: vaultId)
    }

    private func save(data: Data, account: String) throws {
        try Self.retrying {
            let query = baseQuery(account: account)
            let attributes: [CFString: Any] = [
                kSecValueData: data,
                kSecAttrAccessible: kSecAttrAccessibleWhenUnlocked,
            ]
            let updateStatus = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
            if updateStatus == errSecSuccess { return }
            guard updateStatus == errSecItemNotFound else {
                throw DeviceSyncCredentialError.keychain(status: updateStatus)
            }

            var addQuery = query
            addQuery[kSecValueData] = data
            addQuery[kSecAttrAccessible] = kSecAttrAccessibleWhenUnlocked
            let status = SecItemAdd(addQuery as CFDictionary, nil)
            guard status == errSecSuccess else {
                throw DeviceSyncCredentialError.keychain(status: status)
            }
        }
    }

    private func read(account: String) throws -> Data? {
        try Self.retrying {
            var query = baseQuery(account: account)
            query[kSecReturnData] = true
            query[kSecMatchLimit] = kSecMatchLimitOne

            var result: AnyObject?
            let status = SecItemCopyMatching(query as CFDictionary, &result)
            if status == errSecItemNotFound { return nil }
            guard status == errSecSuccess else {
                throw DeviceSyncCredentialError.keychain(status: status)
            }
            return result as? Data
        }
    }

    private func delete(account: String) throws {
        try Self.retrying {
            let status = SecItemDelete(baseQuery(account: account) as CFDictionary)
            guard status == errSecSuccess || status == errSecItemNotFound else {
                throw DeviceSyncCredentialError.keychain(status: status)
            }
        }
    }

    /// Internal for deterministic tests. The operation itself is synchronous
    /// so callers can use it around any Security.framework primitive without
    /// exposing credentials to the retry policy.
    static func retrying<T>(
        maxAttempts: Int = 3,
        sleep: (UInt64) -> Void = { nanoseconds in
            Thread.sleep(forTimeInterval: TimeInterval(nanoseconds) / 1_000_000_000)
        },
        operation: () throws -> T
    ) throws -> T {
        let attempts = max(1, maxAttempts)
        var attempt = 0
        while true {
            do {
                return try operation()
            } catch {
                attempt += 1
                guard attempt < attempts, isTransientKeychainError(error) else {
                    throw error
                }
                let delayIndex = min(attempt - 1, retryDelaysNanoseconds.count - 1)
                sleep(retryDelaysNanoseconds[delayIndex])
            }
        }
    }

    private static func isTransientKeychainError(_ error: Error) -> Bool {
        guard case let DeviceSyncCredentialError.keychain(status) = error else {
            return false
        }
        return transientKeychainStatuses.contains(status)
    }

    private func baseQuery(account: String) -> [CFString: Any] {
        [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: Self.service,
            kSecAttrAccount: account,
        ]
    }

    private static func isSafeIdentifier(_ value: String) -> Bool {
        guard !value.isEmpty,
              !value.contains(":"),
              !value.contains("/"),
              !value.contains("\\"),
              !value.contains("\0"),
              value.rangeOfCharacter(from: .controlCharacters) == nil else {
            return false
        }
        return value.utf8.count <= 1024
    }
}

enum DeviceSyncCredentialError: LocalizedError, Equatable, Sendable {
    case emptyValue
    case invalidIdentifier
    case invalidMasterKey
    case keychain(status: OSStatus)

    var errorDescription: String? {
        switch self {
        case .emptyValue:
            return "Device Sync credential is empty"
        case .invalidIdentifier:
            return "Invalid Device Sync identifier"
        case .invalidMasterKey:
            return "Device Sync master key must be 32 bytes"
        case let .keychain(status):
            return "Keychain operation failed (OSStatus: \(status))"
        }
    }
}
