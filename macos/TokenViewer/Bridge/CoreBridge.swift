import Foundation

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

    func rebuildAll() -> Data? {
        call { tt_rebuild_all($0) }
    }

    func getProviderStatus() -> Data? {
        call { tt_get_provider_status($0) }
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

    func queryModelBreakdown(from: String, to: String) -> Data? {
        call { tt_query_model_breakdown($0, from, to) }
    }

    func queryHeatmap(weeks: Int32 = 52) -> Data? {
        call { tt_query_heatmap($0, weeks) }
    }

    /// Run an FFI call on the serial queue, copy the returned C string into Data,
    /// and free it. Returns nil if the handle is gone or the call returns null.
    private func call(_ body: (OpaquePointer) -> UnsafeMutablePointer<CChar>?) -> Data? {
        queue.sync {
            guard let h = handle, let ptr = body(h) else { return nil }
            defer { tt_free_string(ptr) }
            return String(cString: ptr).data(using: .utf8)
        }
    }
}
