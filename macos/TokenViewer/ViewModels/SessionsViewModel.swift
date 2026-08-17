import Foundation

enum SessionDateRange: String, CaseIterable, Identifiable {
    case all
    case sevenDays
    case thirtyDays
    case ninetyDays

    var id: String { rawValue }

    var days: Int? {
        switch self {
        case .all: nil
        case .sevenDays: 7
        case .thirtyDays: 30
        case .ninetyDays: 90
        }
    }
}

@MainActor
final class SessionsViewModel: ObservableObject {
    static let shared = SessionsViewModel()

    @Published var sessions: [SessionEntry] = []
    /// Empty means "all agents".
    @Published var selectedAgent = ""
    @Published var selectedRange: SessionDateRange = .all
    @Published var selectedProject = ""
    @Published var searchText = ""
    @Published var isScanning = false
    @Published var isLoading = false
    /// Distinct agent sources discovered on disk — the dynamic filter, never hardcoded.
    @Published var agentSources: [String] = []
    @Published var totalCount = 0

    private let pageSize: Int32 = 500
    private var hasStarted = false
    private var loadGeneration = 0

    private init() {}

    // MARK: - Cache-first display + background incremental scan

    /// Show cached rows immediately, then rescan in the background so the UI
    /// never blocks on the (potentially slow) filesystem walk.
    func start() {
        guard !hasStarted else { return }
        hasStarted = true
        AgentRegistry.shared.refreshInstallStatus()
        loadCachedMetadata()
        reload(reset: true)
        scanInBackground()
    }

    func refresh() {
        scanInBackground()
    }

    private func scanInBackground() {
        guard !isScanning else { return }
        isScanning = true
        Task.detached { [weak self] in
            _ = CoreBridge.shared.sessionsScan()
            let sources = CoreBridge.shared.sessionsSources()
            await MainActor.run { [weak self] in
                guard let self else { return }
                self.isScanning = false
                self.agentSources = Self.sortSources(sources)
                self.reload(reset: true)
            }
        }
    }

    // MARK: - Filter

    func selectAgent(_ source: String) {
        guard source != selectedAgent else { return }
        selectedAgent = source
        reload(reset: true)
    }

    var projects: [String] {
        Array(Set(sessions.map(\.project).filter { !$0.isEmpty }))
            .sorted { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }
    }

    var filteredSessions: [SessionEntry] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        let cutoff = selectedRange.days.flatMap {
            Calendar.current.date(byAdding: .day, value: -$0, to: Date())
        }
        return sessions.filter { session in
            if !selectedProject.isEmpty, session.project != selectedProject {
                return false
            }
            if let cutoff, let active = session.lastActiveDate, active < cutoff {
                return false
            }
            if !query.isEmpty {
                let haystack = [
                    session.displayTitle,
                    session.project,
                    session.cwd,
                    session.model,
                    session.source,
                    session.rawSessionID,
                ].joined(separator: " ")
                if haystack.range(of: query, options: [.caseInsensitive, .diacriticInsensitive]) == nil {
                    return false
                }
            }
            return true
        }
    }

    private func loadCachedMetadata() {
        Task.detached { [weak self] in
            let sources = CoreBridge.shared.sessionsSources()
            await MainActor.run { [weak self] in
                self?.agentSources = Self.sortSources(sources)
            }
        }
    }

    private static func sortSources(_ sources: [String]) -> [String] {
        sources.sorted {
            AgentRegistry.shared.displayName(for: $0)
                .localizedCaseInsensitiveCompare(AgentRegistry.shared.displayName(for: $1))
                == .orderedAscending
        }
    }

    // MARK: - Loading

    private func reload(reset: Bool) {
        guard reset else { return }
        loadGeneration &+= 1
        let generation = loadGeneration
        let source = selectedAgent
        isLoading = true
        Task.detached { [weak self] in
            let count = CoreBridge.shared.sessionsCount(source: source)
            var rows: [SessionEntry] = []
            var offset: Int32 = 0
            while rows.count < count {
                guard let data = CoreBridge.shared.sessionsList(
                    source: source,
                    offset: offset,
                    limit: self?.pageSize ?? 500
                ) else { break }
                let page = (try? JSONDecoder().decode([SessionEntry].self, from: data)) ?? []
                if page.isEmpty { break }
                rows.append(contentsOf: page)
                offset += Int32(page.count)
                if page.count < Int(self?.pageSize ?? 500) { break }
            }
            await MainActor.run { [weak self] in
                guard let self, generation == self.loadGeneration else { return }
                self.isLoading = false
                self.totalCount = count
                self.sessions = rows
            }
        }
    }

    // MARK: - Rename (persisted)

    func rename(_ session: SessionEntry, to newTitle: String) {
        let trimmed = newTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        guard CoreBridge.shared.sessionsRename(id: session.id, title: trimmed) else {
            ToastCenter.shared.error(L10n.shared.sessionRenameFailed)
            return
        }
        if let idx = sessions.firstIndex(where: { $0.id == session.id }) {
            sessions[idx].custom_title = trimmed.isEmpty ? nil : trimmed
        }
        ToastCenter.shared.success(L10n.shared.sessionRenamed)
    }
}
