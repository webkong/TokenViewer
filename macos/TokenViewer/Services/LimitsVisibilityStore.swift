import Foundation

enum LimitsVisibilityStore {
    static let allSources: [String] = [
        "claude",
        "codex",
        "cursor",
        "gemini",
        "kiro",
        "copilot",
        "kimi",
        "antigravity",
        "zed",
        "trae",
        "windsurf",
        "qoder",
        "workbuddy",
    ]

    static let defaultsValue = allSources.joined(separator: ",")

    static func visibleSet(from raw: String) -> Set<String> {
        let parts = raw
            .split(separator: ",")
            .map { String($0).trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        return Set(parts)
    }

    static func rawValue(from visible: Set<String>) -> String {
        allSources.filter { visible.contains($0) }.joined(separator: ",")
    }

    static func displayName(for source: String) -> String {
        switch source {
        case "claude": return "Claude"
        case "codex": return "Codex"
        case "cursor": return "Cursor"
        case "gemini": return "Gemini"
        case "kiro": return "Kiro"
        case "copilot": return "GitHub Copilot"
        case "kimi": return "Kimi"
        case "antigravity": return "Antigravity"
        case "zed": return "Zed"
        case "trae": return "Trae"
        case "windsurf": return "Windsurf"
        case "qoder": return "Qoder"
        case "workbuddy": return "WorkBuddy"
        default:
            return source.capitalized
        }
    }
}
