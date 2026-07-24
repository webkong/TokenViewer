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

final class SkillEnvironmentManager: @unchecked Sendable {
    static let shared = SkillEnvironmentManager()

    private let header = "# Managed by TokenViewer. Edit values in TokenViewer to avoid conflicts."
    private let valuePrefix = "# tokenviewer-value "
    private let sourceBlock = """
    # >>> TokenViewer skill environment >>>
    [ -f "$HOME/.tokenviewer/skill-env.sh" ] && . "$HOME/.tokenviewer/skill-env.sh"
    # <<< TokenViewer skill environment <<<
    """
    private let lock = NSLock()

    private init() {}

    func value(for name: String) -> String? {
        guard Self.isValidName(name) else { return nil }
        lock.lock()
        defer { lock.unlock() }
        return readValues()[name]
    }

    func save(_ value: String, for name: String) throws {
        guard Self.isValidName(name) else {
            throw SkillEnvironmentError.invalidName
        }

        lock.lock()
        defer { lock.unlock() }
        var values = readValues()
        values[name] = value
        try writeValues(values)
        try configureShellProfiles()
    }

    func remove(_ name: String) throws {
        guard Self.isValidName(name) else {
            throw SkillEnvironmentError.invalidName
        }

        lock.lock()
        defer { lock.unlock() }
        var values = readValues()
        values.removeValue(forKey: name)
        try writeValues(values)
        try configureShellProfiles()
    }

    private var environmentFileURL: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".tokenviewer", isDirectory: true)
            .appendingPathComponent("skill-env.sh")
    }

    private func readValues() -> [String: String] {
        guard let content = try? String(contentsOf: environmentFileURL, encoding: .utf8) else {
            return [:]
        }
        var values: [String: String] = [:]
        for line in content.components(separatedBy: .newlines) where line.hasPrefix(valuePrefix) {
            let payload = String(line.dropFirst(valuePrefix.count))
            guard let separator = payload.firstIndex(of: " ") else { continue }
            let name = String(payload[..<separator])
            let encoded = String(payload[payload.index(after: separator)...])
            guard Self.isValidName(name),
                  let data = Data(base64Encoded: encoded),
                  let value = String(data: data, encoding: .utf8) else {
                continue
            }
            values[name] = value
        }
        return values
    }

    private func writeValues(_ values: [String: String]) throws {
        let fileManager = FileManager.default
        let directory = environmentFileURL.deletingLastPathComponent()
        do {
            try fileManager.createDirectory(
                at: directory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            var lines = [header]
            for (name, value) in values.sorted(by: { $0.key < $1.key }) {
                guard let data = value.data(using: .utf8) else {
                    throw SkillEnvironmentError.encodingFailed
                }
                lines.append("\(valuePrefix)\(name) \(data.base64EncodedString())")
                lines.append("export \(name)=\(shellQuote(value))")
            }
            lines.append("")
            try lines.joined(separator: "\n").write(
                to: environmentFileURL,
                atomically: true,
                encoding: .utf8
            )
            try fileManager.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: environmentFileURL.path
            )
        } catch let error as SkillEnvironmentError {
            throw error
        } catch {
            throw SkillEnvironmentError.fileWriteFailed
        }
    }

    private func configureShellProfiles() throws {
        let home = FileManager.default.homeDirectoryForCurrentUser
        for filename in [".zshrc", ".bashrc"] {
            let url = home.appendingPathComponent(filename)
            let existing = (try? String(contentsOf: url, encoding: .utf8)) ?? ""
            guard !existing.contains("# >>> TokenViewer skill environment >>>") else {
                continue
            }
            let separator = existing.isEmpty || existing.hasSuffix("\n") ? "" : "\n"
            do {
                try "\(existing)\(separator)\(sourceBlock)\n".write(
                    to: url,
                    atomically: true,
                    encoding: .utf8
                )
            } catch {
                throw SkillEnvironmentError.fileWriteFailed
            }
        }
    }

    private func shellQuote(_ value: String) -> String {
        "'\(value.replacingOccurrences(of: "'", with: "'\"'\"'"))'"
    }

    private static func isValidName(_ name: String) -> Bool {
        guard let first = name.first,
              first == "_" || first.isASCII && first.isLetter else {
            return false
        }
        return name.dropFirst().allSatisfy {
            $0 == "_" || $0.isASCII && ($0.isLetter || $0.isNumber)
        }
    }
}

enum SkillEnvironmentError: LocalizedError {
    case invalidName
    case encodingFailed
    case fileWriteFailed

    var errorDescription: String? {
        switch self {
        case .invalidName:
            return "Invalid environment variable name"
        case .encodingFailed:
            return "Failed to encode environment variable"
        case .fileWriteFailed:
            return "Failed to update the shell environment configuration"
        }
    }
}
