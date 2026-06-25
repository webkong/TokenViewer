import Foundation
import Security

final class KeychainManager {
    static let shared = KeychainManager()

    private let servicePrefix = "com.tokenviewer.git-token"
    private let account = "git-sync"

    private init() {}

    func saveToken(_ token: String, for provider: String) throws {
        let service = serviceName(for: provider)
        guard let data = token.data(using: .utf8) else {
            throw KeychainError.encodingFailed
        }

        let deleteQuery: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account,
        ]
        SecItemDelete(deleteQuery as CFDictionary)

        let addQuery: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account,
            kSecValueData: data,
            kSecAttrAccessible: kSecAttrAccessibleWhenUnlocked,
        ]

        let status = SecItemAdd(addQuery as CFDictionary, nil)
        guard status == errSecSuccess else {
            throw KeychainError.saveFailed(status: status)
        }
    }

    func getToken(for provider: String) -> String? {
        let service = serviceName(for: provider)
        let query: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account,
            kSecReturnData: true,
            kSecMatchLimit: kSecMatchLimitOne,
        ]

        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        guard status == errSecSuccess,
              let data = result as? Data
        else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    func deleteToken(for provider: String) throws {
        let service = serviceName(for: provider)
        let query: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrAccount: account,
        ]

        let status = SecItemDelete(query as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw KeychainError.deleteFailed(status: status)
        }
    }

    private func serviceName(for provider: String) -> String {
        "\(servicePrefix).\(provider)"
    }
}

enum KeychainError: LocalizedError {
    case encodingFailed
    case saveFailed(status: OSStatus)
    case deleteFailed(status: OSStatus)

    var errorDescription: String? {
        switch self {
        case .encodingFailed:
            return "Failed to encode token data"
        case .saveFailed(let status):
            return "Failed to save token to Keychain (OSStatus: \(status))"
        case .deleteFailed(let status):
            return "Failed to delete token from Keychain (OSStatus: \(status))"
        }
    }
}
