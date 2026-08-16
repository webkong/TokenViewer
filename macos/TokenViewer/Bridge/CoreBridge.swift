import Foundation

struct CodexHomeInfo: Codable, Hashable, Identifiable, Sendable {
    let path: String
    let source: String
    let exists: Bool
    let hasSessions: Bool
    let hasAuth: Bool
    let hasConfig: Bool
    let isUserConfigured: Bool

    var id: String { path }
}

private struct CodexHomesUpdateResponse: Codable {
    let ok: Bool
    let homes: [CodexHomeInfo]?
    let error: String?
}

private struct SetPricingResponse: Codable {
    let ok: Bool
    let models: Int?
    let error: String?
}

/// Swift wrapper around the Rust FFI core.
/// All FFI calls are serialized through a private queue so the Rust handle is
/// never accessed concurrently from the main thread and background sync tasks.
final class CoreBridge: @unchecked Sendable {
    static let shared = CoreBridge()

    private var handle: OpaquePointer?
    private let queue = DispatchQueue(label: "com.tokenviewer.core")

    private init() {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let dbPath = "\(home)/.tokenviewer/data.db"
        handle = tt_init(dbPath)
    }

    var isReady: Bool { queue.sync { handle != nil } }

    /// Explicitly tear down the Rust handle (flushes SQLite WAL). Call on app quit.
    func shutdown() {
        queue.sync {
            if let h = handle { tt_destroy(h) }
            handle = nil
        }
    }

    func syncAll() -> Data? {
        call { tt_sync_all($0) }
    }

    func getCodexHomes(force: Bool = false) -> [CodexHomeInfo] {
        guard let data = call({ tt_get_codex_homes($0, force ? 1 : 0) }) else { return [] }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return (try? decoder.decode([CodexHomeInfo].self, from: data)) ?? []
    }

    func setCodexAdditionalHomes(_ paths: [String]) -> Result<[CodexHomeInfo], Error> {
        guard let payload = try? JSONEncoder().encode(paths),
              let json = String(data: payload, encoding: .utf8),
              let data = call({ handle in
                  json.withCString { tt_set_codex_additional_homes(handle, $0) }
              }) else {
            return .failure(CoreBridgeError.invalidResponse)
        }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        guard let response = try? decoder.decode(CodexHomesUpdateResponse.self, from: data) else {
            return .failure(CoreBridgeError.invalidResponse)
        }
        if response.ok {
            return .success(response.homes ?? [])
        }
        return .failure(CoreBridgeError.operationFailed(response.error ?? "Unknown error"))
    }

    /// Install a LiteLLM pricing table into the Rust runtime. `json` is a JSON
    /// object of model -> { input_cost_per_token, output_cost_per_token, ... }.
    /// Runs on the serial queue (harmless — the pricing table is a process-global
    /// static) and returns the number of models installed on success.
    func setPricing(_ json: String) -> Result<Int, Error> {
        let data: Data? = queue.sync {
            guard let ptr = json.withCString({ tt_set_pricing($0) }) else { return nil }
            defer { tt_free_string(ptr) }
            return String(cString: ptr).data(using: .utf8)
        }
        guard let data else { return .failure(CoreBridgeError.invalidResponse) }
        guard let response = try? JSONDecoder().decode(SetPricingResponse.self, from: data) else {
            return .failure(CoreBridgeError.invalidResponse)
        }
        if response.ok {
            return .success(response.models ?? 0)
        }
        return .failure(CoreBridgeError.operationFailed(response.error ?? "Unknown error"))
    }

    func rebuildAll() -> Data? {
        call { tt_rebuild_all($0) }
    }

    func getAgentStatus() -> Data? {
        call { tt_get_agent_status($0) }
    }

    func querySummary(from: String, to: String) -> Data? {
        call { tt_query_summary($0, from, to) }
    }

    func queryDaily(from: String, to: String) -> Data? {
        call { tt_query_daily($0, from, to) }
    }

    func queryHourly(from: String, to: String) -> Data? {
        call { tt_query_hourly($0, from, to) }
    }

    func queryAgentTrend(from: String, to: String, source: String, hourly: Bool) -> Data? {
        call { tt_query_agent_trend($0, from, to, source, hourly ? 1 : 0) }
    }

    func queryModelBreakdown(from: String, to: String) -> Data? {
        call { tt_query_model_breakdown($0, from, to) }
    }

    func queryProjectUsage() -> Data? {
        call { tt_query_project_usage($0) }
    }

    func queryHeatmap(weeks: Int32 = 52) -> Data? {
        call { tt_query_heatmap($0, weeks) }
    }

    /// Run an FFI call on the serial queue, copy the returned C string into Data,
    /// and free it. Returns nil if the handle is gone or the call returns null.
    func call(_ body: (OpaquePointer) -> UnsafeMutablePointer<CChar>?) -> Data? {
        queue.sync {
            guard let h = handle, let ptr = body(h) else { return nil }
            defer { tt_free_string(ptr) }
            return String(cString: ptr).data(using: .utf8)
        }
    }
}

private enum CoreBridgeError: LocalizedError {
    case invalidResponse
    case operationFailed(String)

    var errorDescription: String? {
        switch self {
        case .invalidResponse: return "Invalid core response"
        case .operationFailed(let message): return message
        }
    }
}
