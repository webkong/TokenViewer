import Foundation

/// A resumable coding-agent session, decoded from the Rust `Session` JSON.
///
/// Field names are snake_case to match the FFI JSON exactly (no key strategy),
/// consistent with the other query result structs in this codebase.
struct SessionEntry: Codable, Identifiable, Hashable {
    var id: String
    var source: String
    var cwd: String
    var project: String
    var title: String
    var custom_title: String?
    var first_user_message: String
    var started_at: String
    var last_active_at: String
    var file_path: String
    var codex_home: String
    var model: String
    var total_tokens: UInt64
    var total_cost_usd: Double
    var turn_count: UInt32
    var edit_count: UInt32
    var duration_seconds: UInt64

    /// Title shown to the user: the manual override if present, else the auto title.
    var displayTitle: String {
        if let custom = custom_title?.trimmingCharacters(in: .whitespacesAndNewlines), !custom.isEmpty {
            return custom
        }
        if !title.isEmpty { return title }
        if !project.isEmpty { return project }
        return first_user_message
    }

    /// Raw per-agent session id, recovered by stripping the `"<source>:"` prefix.
    var rawSessionID: String {
        id.hasPrefix("\(source):") ? String(id.dropFirst(source.count + 1)) : id
    }

    var lastActiveDate: Date? {
        Self.isoWithFraction.date(from: last_active_at) ?? Self.iso.date(from: last_active_at)
    }

    private static let isoWithFraction: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()

    private static let iso: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime]
        return formatter
    }()
}
