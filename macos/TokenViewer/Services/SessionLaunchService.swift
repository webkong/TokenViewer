import AppKit
import Foundation

/// Describes how to resume a session for one agent. Adapters are pure data
/// (binary + argv builders) so the set can grow without touching launch logic.
struct SessionCommandAdapter {
    let source: String
    let binary: String
    /// argv tail (including the resume selector) given the raw session id.
    let resumeArgs: (String) -> [String]
}

/// Registry mapping an agent source id to its resume command, plus the default
/// per-agent YOLO (auto-approve) flags used by resumable sessions.
final class SessionCommandRegistry {
    static let shared = SessionCommandRegistry()

    /// Canonical YOLO flag map (source → space-separated flags), mirroring
    /// Orca's `YOLO_TUI_AGENT_ARGS` verbatim. `goose` is omitted — its YOLO is
    /// the env var `GOOSE_MODE=auto`, not a flag.
    static let yoloArgsBySource: [(source: String, args: String)] = [
        ("claude", "--dangerously-skip-permissions"),
        ("codex", "--dangerously-bypass-approvals-and-sandbox"),
        ("grok", "--permission-mode bypassPermissions"),
        ("claude-agent-teams", "--dangerously-skip-permissions"),
        ("openclaude", "--dangerously-skip-permissions"),
        ("gemini", "--yolo"),
        ("antigravity", "--dangerously-skip-permissions"),
        ("aider", "--yes-always"),
        ("amp", "--dangerously-allow-all"),
        ("kiro", "--trust-all-tools"),
        ("crush", "--yolo"),
        ("autohand", "--unrestricted"),
        ("cline", "--auto-approve true"),
        ("command-code", "--yolo"),
        ("continue", "--allow \"*\""),
        ("cursor", "--yolo"),
        ("kimi", "--yolo"),
        ("mistral-vibe", "--agent auto-approve"),
        ("qwen-code", "--approval-mode yolo"),
        ("rovo", "--yolo"),
        ("hermes", "--yolo"),
        ("copilot", "--yolo"),
        ("devin", "--permission-mode bypass"),
        ("ante", "--yolo"),
        ("trae", "--yolo"),
    ]

    /// All YOLO-capable agents, in Settings display order.
    static var yoloSources: [String] { yoloArgsBySource.map(\.source) }

    static func defaultYoloArgsString(for source: String) -> String {
        yoloArgsBySource.first { $0.source == source }?.args ?? ""
    }

    private var adapters: [String: SessionCommandAdapter] = [:]
    private var order: [String] = []

    private init() {
        register(SessionCommandAdapter(
            source: "claude",
            binary: "claude",
            resumeArgs: { ["--resume", $0] }
        ))
        register(SessionCommandAdapter(
            source: "codex",
            binary: "codex",
            resumeArgs: { ["resume", $0] }
        ))
        register(SessionCommandAdapter(
            source: "grok",
            binary: "grok",
            resumeArgs: { ["--resume", $0] }
        ))
    }

    private func register(_ adapter: SessionCommandAdapter) {
        adapters[adapter.source] = adapter
        order.append(adapter.source)
    }

    func adapter(for source: String) -> SessionCommandAdapter? {
        adapters[source]
    }

    /// Resumable agents in stable registration order.
    var resumableSources: [String] { order }
}

enum SessionLaunchError: LocalizedError {
    case unsupportedAgent(String)
    case invalidSessionID(String)
    case invalidCWD(String)
    case terminalLaunchFailed(String)

    var errorDescription: String? {
        switch self {
        case .unsupportedAgent(let source):
            return L10n.shared.sessionUnsupportedAgent(Self.fallbackName(source))
        case .invalidSessionID(let id):
            return L10n.shared.sessionInvalidID(id)
        case .invalidCWD(let cwd):
            return L10n.shared.sessionInvalidCWD(cwd)
        case .terminalLaunchFailed(let detail):
            return L10n.shared.sessionTerminalLaunchFailed(detail)
        }
    }

    /// Nonisolated fallback name so `errorDescription` needn't hop to the main
    /// actor (AgentRegistry is main-actor isolated).
    private static func fallbackName(_ source: String) -> String {
        if source == "codex" { return "ChatGPT" }
        return source
            .split(separator: "-")
            .map { part in
                let lower = part.lowercased()
                return lower.prefix(1).uppercased() + lower.dropFirst()
            }
            .joined(separator: " ")
    }
}

/// Builds a validated resume command and opens it in an external Terminal at
/// the session's working directory.
final class SessionLaunchService {
    static let shared = SessionLaunchService()

    private let registry = SessionCommandRegistry.shared

    /// Shell-safe session id: alphanumeric + a narrow punctuation set only.
    /// Rejects anything that could inject flags, paths, or metacharacters.
    private static let sessionIDPattern = #"^[A-Za-z0-9][A-Za-z0-9._:-]{0,199}$"#

    func launch(_ session: SessionEntry) throws {
        guard let adapter = registry.adapter(for: session.source) else {
            throw SessionLaunchError.unsupportedAgent(session.source)
        }

        let rawID = session.rawSessionID
        guard rawID.range(of: Self.sessionIDPattern, options: .regularExpression) != nil else {
            throw SessionLaunchError.invalidSessionID(rawID)
        }

        // Validate cwd: must resolve to an existing directory.
        var cwd = session.cwd.trimmingCharacters(in: .whitespacesAndNewlines)
        if !cwd.isEmpty {
            let expanded = (cwd as NSString).expandingTildeInPath
            var isDir: ObjCBool = false
            guard FileManager.default.fileExists(atPath: expanded, isDirectory: &isDir), isDir.boolValue else {
                throw SessionLaunchError.invalidCWD(cwd)
            }
            cwd = expanded
        }

        // Per-agent YOLO flags (configurable in Settings → Sessions). Global
        // flags must precede Codex's `resume` subcommand; this ordering matches
        // the CLIs' documented forms for every adapter.
        let yoloArgs = SessionYoloStore.shared.args(for: session.source) ?? []
        let argv = [adapter.binary] + yoloArgs + adapter.resumeArgs(rawID)

        // Isolated Codex homes need CODEX_HOME pointed at the home the session
        // lives under, otherwise `codex resume` can't find it.
        var prefix = ""
        if adapter.source == "codex", !session.codex_home.isEmpty {
            let defaultHome = FileManager.default.homeDirectoryForCurrentUser.path + "/.codex"
            let home = (session.codex_home as NSString).standardizingPath
            if home != (defaultHome as NSString).standardizingPath {
                prefix = "CODEX_HOME=\(shellQuote(session.codex_home)) "
            }
        } else if adapter.source == "codex" {
            // A normal-home resume must not inherit an Orca/account-routed
            // Codex home from the parent process.
            prefix = "unset CODEX_HOME ORCA_CODEX_HOME; "
        }

        let command = prefix + argv.map(shellQuote).joined(separator: " ")
        let shellCommand = cwd.isEmpty ? command : "cd \(shellQuote(cwd)) && \(command)"
        try runInTerminal(shellCommand)
    }

    /// POSIX single-quote a string so it survives shell parsing unchanged.
    private func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    /// Launch Terminal.app and run the command in a fresh window at cwd.
    private func runInTerminal(_ shellCommand: String) throws {
        // Pass the command as argv rather than interpolating it into AppleScript.
        // This keeps quotes, newlines and project names from becoming script code.
        let script = """
        on run argv
            tell application "Terminal"
                activate
                do script (item 1 of argv)
            end tell
        end run
        """
        let task = Process()
        task.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        task.arguments = ["-e", script, shellCommand]
        let errorPipe = Pipe()
        task.standardError = errorPipe
        do {
            try task.run()
        } catch {
            throw SessionLaunchError.terminalLaunchFailed(error.localizedDescription)
        }
        task.waitUntilExit()
        guard task.terminationStatus == 0 else {
            let data = errorPipe.fileHandleForReading.readDataToEndOfFile()
            let detail = String(data: data, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            throw SessionLaunchError.terminalLaunchFailed(
                detail?.isEmpty == false ? detail! : "osascript \(task.terminationStatus)"
            )
        }
    }
}
