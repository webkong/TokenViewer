import Foundation

/// Fetches LiteLLM's model pricing table and installs it into the Rust core's
/// runtime override table (`tt_set_pricing`), mirroring TokenTracker's
/// litellm-fetcher.js: a 24h disk cache, upstream fetch only when the cache is
/// stale, and graceful degradation (stale cache → skip) on network failure.
///
/// Local-first: on total failure we simply skip installation — the Rust builtin
/// table (`core/src/pricing/data.rs`) still covers known models, so costs never
/// silently break. The runtime override only upgrades lookup precision.
final class LiteLLMPricingService {
    static let shared = LiteLLMPricingService()

    private static let pricingURL = URL(
        string: "https://raw.githubusercontent.com/BerriAI/litellm/main/model_prices_and_context_window.json"
    )!
    private static let ttl: TimeInterval = 24 * 60 * 60
    private static let fetchTimeout: TimeInterval = 10
    private static let costFields = [
        "input_cost_per_token",
        "output_cost_per_token",
        "cache_read_input_token_cost",
        "cache_creation_input_token_cost",
    ]

    private var isRunning = false
    private let lock = NSLock()

    /// Kick off a one-shot background refresh. Safe to call multiple times;
    /// concurrent calls are coalesced.
    func start() {
        lock.lock()
        if isRunning {
            lock.unlock()
            return
        }
        isRunning = true
        lock.unlock()
        Task.detached(priority: .utility) { [self] in
            await refresh()
            finishRunning()
        }
    }

    private func finishRunning() {
        lock.lock()
        isRunning = false
        lock.unlock()
    }

    /// Load (fresh cache → upstream → stale cache → skip) and install into Rust.
    func refresh() async {
        let cacheURL = Self.cacheFileURL()

        if Self.isFresh(cacheURL), let fresh = try? Self.loadCache(at: cacheURL) {
            await Self.install(fresh)
            return
        }

        do {
            let upstream = try await Self.fetchUpstream()
            let slim = Self.slim(upstream)
            Self.writeCache(slim, to: cacheURL)
            await Self.install(slim)
        } catch {
            if let stale = try? Self.loadCache(at: cacheURL) {
                await Self.install(stale)
            }
        }
    }

    // MARK: - Install

    /// Send the slimmed JSON to the Rust runtime, then refresh displayed costs
    /// so an asynchronous startup fetch cannot leave the initial dashboard on
    /// the builtin fallback table.
    private static func install(_ payload: [String: Any]) async {
        guard let json = try? JSONSerialization.data(withJSONObject: payload) else { return }
        let raw = String(data: json, encoding: .utf8) ?? ""
        guard case .success = CoreBridge.shared.setPricing(raw) else { return }
        await MainActor.run {
            UsageViewModel.shared.refresh()
        }
    }

    // MARK: - Upstream

    private static func fetchUpstream() async throws -> [String: Any] {
        var request = URLRequest(url: pricingURL)
        request.timeoutInterval = fetchTimeout
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse, (200...299).contains(http.statusCode) else {
            throw URLError(.badServerResponse)
        }
        guard let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw URLError(.cannotParseResponse)
        }
        return obj
    }

    // MARK: - Slimming (mirror of litellm-fetcher.js writeCache)

    /// Persist only the four cost fields per model (plus a `_meta` block) to
    /// keep the cache small and human-inspectable. `_*` meta keys are skipped.
    private static func slim(_ raw: [String: Any]) -> [String: Any] {
        var out: [String: Any] = [:]
        for (name, entry) in raw {
            if name.hasPrefix("_") { continue }
            guard let entry = entry as? [String: Any] else { continue }
            var kept: [String: Any] = [:]
            for field in costFields {
                if let value = entry[field] as? NSNumber, value.doubleValue.isFinite {
                    kept[field] = value.doubleValue
                }
            }
            if !kept.isEmpty {
                out[name] = kept
            }
        }
        out["_meta"] = [
            "source": pricingURL.absoluteString,
            "cached_at": ISO8601DateFormatter().string(from: Date()),
            "kept_models": out.count,
        ]
        return out
    }

    // MARK: - Disk cache

    private static func cacheFileURL() -> URL {
        let home = FileManager.default.homeDirectoryForCurrentUser
        return home.appendingPathComponent(".tokenviewer/cache/litellm-pricing.json")
    }

    private static func isFresh(_ url: URL) -> Bool {
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: url.path),
              let mtime = attrs[.modificationDate] as? Date else {
            return false
        }
        return Date().timeIntervalSince(mtime) < ttl
    }

    private static func loadCache(at url: URL) throws -> [String: Any] {
        let data = try Data(contentsOf: url)
        guard let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw URLError(.cannotParseResponse)
        }
        return obj
    }

    private static func writeCache(_ payload: [String: Any], to url: URL) {
        try? FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        guard let data = try? JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys]) else {
            return
        }
        try? (data + Data("\n".utf8)).write(to: url, options: .atomic)
    }
}
