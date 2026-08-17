import Foundation

/// Per-agent YOLO (auto-approve) launch configuration.
struct SessionYoloConfig: Codable, Identifiable, Hashable {
    var source: String
    var enabled: Bool
    /// Space-separated CLI flags appended when YOLO is enabled for this agent.
    var args: String

    var id: String { source }
}

/// Persistent per-agent YOLO settings. Defaults are seeded from the command
/// registry's canonical YOLO flag map (all disabled); user edits are stored as
/// JSON in UserDefaults and survive restarts.
final class SessionYoloStore: ObservableObject {
    static let shared = SessionYoloStore()

    @Published var configs: [SessionYoloConfig] = []

    private let key = "sessionYoloConfigs"

    private init() {
        load()
    }

    func config(for source: String) -> SessionYoloConfig? {
        configs.first { $0.source == source }
    }

    /// Tokenized flags for a source when YOLO is enabled, else `nil`.
    func args(for source: String) -> [String]? {
        guard let cfg = config(for: source), cfg.enabled else { return nil }
        let tokens = cfg.args.split(whereSeparator: { $0.isWhitespace }).map(String.init)
        return tokens.isEmpty ? nil : tokens
    }

    func setEnabled(_ source: String, _ enabled: Bool) {
        guard let idx = configs.firstIndex(where: { $0.source == source }) else { return }
        configs[idx].enabled = enabled
        persist()
    }

    func setArgs(_ source: String, _ args: String) {
        guard let idx = configs.firstIndex(where: { $0.source == source }) else { return }
        configs[idx].args = args
        persist()
    }

    /// Restore defaults (used by Settings → Reset Settings).
    func reset() {
        load(forceDefaults: true)
    }

    private func load(forceDefaults: Bool = false) {
        let sources = SessionCommandRegistry.yoloSources
        let defaults = sources.map { source in
            SessionYoloConfig(
                source: source,
                enabled: false,
                args: SessionCommandRegistry.defaultYoloArgsString(for: source)
            )
        }
        var bySource = Dictionary(uniqueKeysWithValues: defaults.map { ($0.source, $0) })
        if !forceDefaults,
           let raw = UserDefaults.standard.string(forKey: key),
           let data = raw.data(using: .utf8),
           let saved = try? JSONDecoder().decode([SessionYoloConfig].self, from: data) {
            for cfg in saved {
                bySource[cfg.source] = cfg
            }
        }
        configs = sources.compactMap { bySource[$0] }
    }

    private func persist() {
        if let data = try? JSONEncoder().encode(configs),
           let raw = String(data: data, encoding: .utf8) {
            UserDefaults.standard.set(raw, forKey: key)
        }
    }
}
