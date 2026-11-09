import AppKit
import Foundation

enum SessionLaunchApplication: String, CaseIterable, Identifiable {
    static let storageKey = "sessionLaunchApplication"

    case terminal
    case iTerm2
    case ghostty
    case cmux
    case orca

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .terminal: "Terminal"
        case .iTerm2: "iTerm2"
        case .ghostty: "Ghostty"
        case .cmux: "cmux"
        case .orca: "Orca"
        }
    }

    var isInstalled: Bool {
        switch self {
        case .terminal:
            true
        case .iTerm2:
            NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.googlecode.iterm2") != nil
        case .ghostty:
            NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.mitchellh.ghostty") != nil
                || FileManager.default.fileExists(atPath: "/Applications/Ghostty.app")
        case .cmux:
            SessionLaunchService.firstExecutable(in: SessionLaunchService.cmuxExecutablePaths) != nil
        case .orca:
            SessionLaunchService.firstExecutable(in: SessionLaunchService.orcaExecutablePaths) != nil
        }
    }
}

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
    case parentSessionUnavailable

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
        case .parentSessionUnavailable:
            return L10n.shared.sessionParentUnavailable
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

/// Origin metadata is read from the rollout header, independently of the usage
/// cache. Old cached sessions therefore gain routing without a database rebuild.
struct SessionLaunchContext {
    var isDesktop = false
    var isSubagent = false
    var parentID: String?
    var cwd: String?

    func usesDesktop(for session: SessionEntry, preferOriginal: Bool) -> Bool {
        // A deep link cannot select an isolated CODEX_HOME/account.
        let defaultHome = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".codex").standardizedFileURL.path
        let home = (session.codex_home as NSString).standardizingPath
        return preferOriginal && isDesktop && (home.isEmpty || home == defaultHome)
    }
}

/// Routes known desktop sessions to their original app; other sessions use the
/// configured terminal. Subagents always resolve to a resumable parent first.
final class SessionLaunchService {
    static let shared = SessionLaunchService()
    static let preferOriginalApplicationKey = "sessionPreferOriginalApplication"
    static let desktopBundleIdentifier = "com.openai.codex"

    static let cmuxExecutablePaths = [
        "/usr/local/bin/cmux",
        "/opt/homebrew/bin/cmux",
        "/Applications/cmux.app/Contents/Resources/bin/cmux",
    ]

    static let orcaExecutablePaths = [
        "/usr/local/bin/orca",
        "/opt/homebrew/bin/orca",
        "/Applications/Orca.app/Contents/Resources/bin/orca",
    ]

    private let registry = SessionCommandRegistry.shared

    /// Shell-safe session id: alphanumeric + a narrow punctuation set only.
    /// Rejects anything that could inject flags, paths, or metacharacters.
    private static let sessionIDPattern = #"^[A-Za-z0-9][A-Za-z0-9._:-]{0,199}$"#

    /// Resolve a recorded cwd (tilde-expanded) to an existing directory path,
    /// or nil when it is empty or no longer exists. Used both to pre-validate
    /// rows in the Sessions list and inside `launch`.
    static func resolvedCWD(_ cwd: String) -> String? {
        let trimmed = cwd.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let expanded = (trimmed as NSString).expandingTildeInPath
        var isDir: ObjCBool = false
        guard FileManager.default.fileExists(atPath: expanded, isDirectory: &isDir), isDir.boolValue else {
            return nil
        }
        return expanded
    }

    /// Whether a session can be launched given its recorded cwd: an empty cwd
    /// launches from the home directory, otherwise the directory must exist.
    static func isCWDUsable(_ cwd: String) -> Bool {
        let trimmed = cwd.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty || resolvedCWD(trimmed) != nil
    }

    static func launchContext(for session: SessionEntry) -> SessionLaunchContext {
        guard session.source == "codex" else { return SessionLaunchContext() }
        return codexContext(filePath: session.file_path, expectedID: session.rawSessionID) ?? SessionLaunchContext()
    }

    private static func codexContext(filePath: String, expectedID: String) -> SessionLaunchContext? {
        guard let handle = FileHandle(forReadingAtPath: filePath) else { return nil }
        defer { try? handle.close() }
        // Read only the first JSONL record, with a fixed bound even for corrupt files.
        var header = Data()
        while header.count < 1_048_576 {
            guard let chunk = try? handle.read(upToCount: 16_384), !chunk.isEmpty else { break }
            if let newline = chunk.firstIndex(of: 10) {
                header.append(chunk.prefix(upTo: newline))
                break
            }
            header.append(chunk)
        }
        guard let event = (try? JSONSerialization.jsonObject(with: header)) as? [String: Any],
              event["type"] as? String == "session_meta",
              let payload = event["payload"] as? [String: Any],
              payload["id"] as? String == expectedID else { return nil }
        let originator = (payload["originator"] as? String ?? "")
            .trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let source = payload["source"] as? [String: Any]
        let subagent = source?["subagent"] as? [String: Any]
        let spawn = subagent?["thread_spawn"] as? [String: Any]
        let isSubagent = payload["thread_source"] as? String == "subagent" || source?["subagent"] != nil
        // forked_from_id alone is a normal fork, not a subagent relationship.
        let parentID = isSubagent
            ? (spawn?["parent_thread_id"] as? String ?? payload["parent_thread_id"] as? String)
            : nil
        return SessionLaunchContext(
            isDesktop: ["codex desktop", "chatgpt desktop"].contains(originator),
            isSubagent: isSubagent,
            parentID: parentID,
            cwd: payload["cwd"] as? String
        )
    }

    /// Follows nested subagents within the same Codex home. Never resume a known
    /// child directly or substitute an ordinary fork's original conversation.
    static func resumableSession(for session: SessionEntry) throws -> SessionEntry {
        guard session.source == "codex" else { return session }
        var result = session
        var visited: Set<String> = []
        var filesByID: [String: String]?
        while true {
            guard visited.insert(result.rawSessionID).inserted, visited.count <= 32 else {
                throw SessionLaunchError.parentSessionUnavailable
            }
            let context = launchContext(for: result)
            guard context.isSubagent else { return result }
            guard let parentID = context.parentID, UUID(uuidString: parentID) != nil else {
                throw SessionLaunchError.parentSessionUnavailable
            }
            if filesByID == nil {
                var directory = URL(fileURLWithPath: session.file_path).deletingLastPathComponent()
                while directory.path != "/",
                      !["sessions", "archived_sessions"].contains(directory.lastPathComponent) {
                    directory.deleteLastPathComponent()
                }
                guard directory.path != "/" else { throw SessionLaunchError.parentSessionUnavailable }
                let home = directory.deletingLastPathComponent()
                var index: [String: String] = [:]
                for name in ["archived_sessions", "sessions"] {
                    let base = home.appendingPathComponent(name)
                    guard let enumerator = FileManager.default.enumerator(
                        at: base, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles]
                    ) else { continue }
                    for case let file as URL in enumerator where file.pathExtension == "jsonl" {
                        let stem = file.deletingPathExtension().lastPathComponent
                        let id = String(stem.suffix(36))
                        if UUID(uuidString: id) != nil { index[id] = file.path }
                    }
                }
                filesByID = index
            }
            guard let file = filesByID?[parentID] else { throw SessionLaunchError.parentSessionUnavailable }
            // A missing/malformed parent header must not silently authorize a resume.
            guard let parent = codexContext(filePath: file, expectedID: parentID) else {
                throw SessionLaunchError.parentSessionUnavailable
            }
            result.id = "codex:\(parentID)"
            result.file_path = file
            result.cwd = parent.cwd ?? ""
        }
    }

    func launch(_ originalSession: SessionEntry) async throws {
        let session = try Self.resumableSession(for: originalSession)
        let preferOriginal = UserDefaults.standard.object(forKey: Self.preferOriginalApplicationKey) as? Bool ?? true
        if Self.launchContext(for: session).usesDesktop(for: session, preferOriginal: preferOriginal) {
            guard UUID(uuidString: session.rawSessionID) != nil,
                  let url = URL(string: "codex://threads/\(session.rawSessionID)") else {
                throw SessionLaunchError.invalidSessionID(session.rawSessionID)
            }
            try await openDesktopSession(url)
            return
        }
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
            guard let resolved = Self.resolvedCWD(cwd) else {
                throw SessionLaunchError.invalidCWD(cwd)
            }
            cwd = resolved
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
            } else {
                prefix = "unset CODEX_HOME ORCA_CODEX_HOME && "
            }
        } else if adapter.source == "codex" {
            // A normal-home resume must not inherit an Orca/account-routed
            // Codex home from the parent process.
            prefix = "unset CODEX_HOME ORCA_CODEX_HOME && "
        }

        let command = prefix + argv.map(shellQuote).joined(separator: " ")
        let shellCommand = cwd.isEmpty ? command : "cd \(shellQuote(cwd)) && \(command)"
        let storedApplication = UserDefaults.standard.string(forKey: SessionLaunchApplication.storageKey)
        let application = SessionLaunchApplication(rawValue: storedApplication ?? "") ?? .terminal
        let title = session.displayTitle
            .replacingOccurrences(of: "\n", with: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        try run(
            shellCommand,
            cwd: cwd.isEmpty ? FileManager.default.homeDirectoryForCurrentUser.path : cwd,
            title: String(title.prefix(80)),
            in: application
        )
    }

    @MainActor
    private func openDesktopSession(_ url: URL) async throws {
        guard let appURL = NSWorkspace.shared.urlForApplication(withBundleIdentifier: Self.desktopBundleIdentifier) else {
            throw SessionLaunchError.terminalLaunchFailed(L10n.shared.sessionAgentNotInstalled("ChatGPT"))
        }
        // Target the known application explicitly instead of trusting a scheme's
        // default handler. Completion errors are propagated to the existing toast.
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            NSWorkspace.shared.open([url], withApplicationAt: appURL, configuration: .init()) { _, error in
                if let error {
                    continuation.resume(throwing: SessionLaunchError.terminalLaunchFailed(error.localizedDescription))
                } else {
                    continuation.resume()
                }
            }
        }
    }

    /// POSIX single-quote a string so it survives shell parsing unchanged.
    private func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    /// Launch the selected terminal and run the command in a fresh window.
    private func run(
        _ shellCommand: String,
        cwd: String,
        title: String,
        in application: SessionLaunchApplication
    ) throws {
        switch application {
        case .cmux:
            try runCLI(
                named: "cmux",
                executablePaths: Self.cmuxExecutablePaths,
                arguments: ["new-workspace", "--name", title, "--cwd", cwd, "--command", shellCommand],
                workingDirectory: cwd
            )
            activateApplication(bundleIdentifier: nil, fallbackPath: "/Applications/cmux.app")
            return
        case .orca:
            try runCLI(
                named: "Orca",
                executablePaths: Self.orcaExecutablePaths,
                arguments: ["terminal", "create", "--title", title, "--command", shellCommand, "--json"],
                workingDirectory: cwd
            )
            activateApplication(bundleIdentifier: "com.stablyai.orca", fallbackPath: "/Applications/Orca.app")
            return
        case .terminal, .iTerm2, .ghostty:
            break
        }

        // Pass the command as argv rather than interpolating it into AppleScript.
        // This keeps quotes, newlines and project names from becoming script code.
        let script: String
        switch application {
        case .terminal:
            script = """
            on run argv
                tell application "Terminal"
                    activate
                    do script (item 1 of argv)
                end tell
            end run
            """
        case .iTerm2:
            script = """
            on run argv
                tell application "iTerm2"
                    activate
                    set newWindow to (create window with default profile)
                    tell current session of newWindow to write text (item 1 of argv)
                end tell
            end run
            """
        case .ghostty:
            script = """
            on run argv
                tell application "Ghostty"
                    activate
                    set cfg to new surface configuration
                    set initial working directory of cfg to (item 2 of argv)
                    set initial input of cfg to ((item 1 of argv) & (ASCII character 10))
                    new window with configuration cfg
                end tell
            end run
            """
        case .cmux, .orca:
            return
        }
        let task = Process()
        task.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        task.arguments = ["-e", script, shellCommand, cwd]
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

    static func firstExecutable(in paths: [String]) -> String? {
        paths.first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    private func runCLI(
        named name: String,
        executablePaths: [String],
        arguments: [String],
        workingDirectory: String
    ) throws {
        guard let executable = Self.firstExecutable(in: executablePaths) else {
            throw SessionLaunchError.terminalLaunchFailed("\(name) CLI not found")
        }

        let task = Process()
        task.executableURL = URL(fileURLWithPath: executable)
        task.arguments = arguments
        task.currentDirectoryURL = URL(fileURLWithPath: workingDirectory, isDirectory: true)
        let outputPipe = Pipe()
        task.standardOutput = outputPipe
        task.standardError = outputPipe
        do {
            try task.run()
        } catch {
            throw SessionLaunchError.terminalLaunchFailed(error.localizedDescription)
        }
        task.waitUntilExit()
        guard task.terminationStatus == 0 else {
            let data = outputPipe.fileHandleForReading.readDataToEndOfFile()
            let detail = String(data: data, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            if name == "cmux", detail?.localizedCaseInsensitiveContains("broken pipe") == true {
                throw SessionLaunchError.terminalLaunchFailed(L10n.shared.sessionCmuxAccessRequired)
            }
            throw SessionLaunchError.terminalLaunchFailed(
                detail?.isEmpty == false ? detail! : "\(name) exited with \(task.terminationStatus)"
            )
        }
    }

    private func activateApplication(bundleIdentifier: String?, fallbackPath: String) {
        DispatchQueue.main.async {
            let url = bundleIdentifier.flatMap { NSWorkspace.shared.urlForApplication(withBundleIdentifier: $0) }
                ?? URL(fileURLWithPath: fallbackPath)
            guard FileManager.default.fileExists(atPath: url.path) else { return }
            NSWorkspace.shared.openApplication(at: url, configuration: .init())
        }
    }
}
