import AppKit
import Foundation

@MainActor
final class UpdateChecker: ObservableObject {
    static let shared = UpdateChecker()
    static let repo = "webkong/TokenViewer"

    enum State: Equatable {
        case idle
        case checking
        case upToDate(version: String)
        case available(version: String)
        case downloading(version: String)
        case failed(String)
    }

    @Published private(set) var state: State = .idle
    @Published private(set) var latestRelease: ReleaseInfo?

    // Legacy properties for AboutView compatibility
    var busy: Bool { if case .checking = state { return true }; if case .downloading = state { return true }; return false }
    var latestURL: URL? { latestRelease?.releaseURL }
    var status: String {
        switch state {
        case .idle: return ""
        case .checking: return "Checking…"
        case .upToDate(let v): return "Up to date (v\(v))"
        case .available(let v): return "v\(v) available"
        case .downloading(let v): return "Downloading v\(v)…"
        case .failed(let msg): return msg
        }
    }

    private var currentVersion: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.1.0"
    }
    private var isChecking = false
    private var autoCheckTimer: Timer?
    private static let lastHandledKey = "tokenviewer.lastAutoHandledUpdateVersion"

    struct ReleaseInfo: Equatable {
        let version: String
        let releaseURL: URL
        let pkgURL: URL?
        let notes: String
    }

    // MARK: - Public

    func startAutoCheck() {
        autoCheckTimer?.invalidate()
        autoCheckTimer = Timer.scheduledTimer(withTimeInterval: 3600 * 6, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in self?.checkAuto() }
        }
        Task { checkAuto() }
    }

    func check() {
        Task { await performCheck(auto: false) }
    }

    func install() {
        guard let release = latestRelease else { return }
        Task { await downloadAndInstall(release: release, rememberAuto: false) }
    }

    // MARK: - Private

    private func checkAuto() {
        Task { await performCheck(auto: true) }
    }

    private func performCheck(auto: Bool) async {
        guard !isChecking else { return }
        isChecking = true
        state = .checking
        defer { isChecking = false }

        do {
            let release = try await fetchLatest()
            latestRelease = release
            if isNewer(release.version, than: currentVersion) {
                state = .available(version: release.version)
                if auto, UserDefaults.standard.string(forKey: Self.lastHandledKey) != release.version {
                    await downloadAndInstall(release: release, rememberAuto: true)
                }
            } else {
                state = .upToDate(version: currentVersion)
            }
        } catch {
            state = .failed("Could not check for updates")
        }
    }

    private func downloadAndInstall(release: ReleaseInfo, rememberAuto: Bool) async {
        state = .downloading(version: release.version)

        do {
            let installURL: URL
            if let pkgURL = release.pkgURL {
                let (tmp, _) = try await URLSession.shared.download(for: URLRequest(url: pkgURL))
                let dest = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first!
                    .appendingPathComponent("TokenViewer-\(release.version)-Installer.pkg")
                try? FileManager.default.removeItem(at: dest)
                try FileManager.default.moveItem(at: tmp, to: dest)
                installURL = dest
            } else {
                installURL = release.releaseURL
            }

            guard presentConfirmation(for: release) else {
                state = .available(version: release.version); return
            }

            guard NSWorkspace.shared.open(installURL) else {
                state = .failed("Could not open installer"); return
            }

            if rememberAuto {
                UserDefaults.standard.set(release.version, forKey: Self.lastHandledKey)
            }
            state = .idle

        } catch {
            // Fallback: open release page in browser
            NSWorkspace.shared.open(release.releaseURL)
            state = .available(version: release.version)
        }
    }

    private func presentConfirmation(for release: ReleaseInfo) -> Bool {
        let alert = NSAlert()
        alert.alertStyle = .informational
        alert.messageText = "TokenViewer \(release.version) is available"
        alert.informativeText = "Install the new version now?"
        alert.addButton(withTitle: "Install Now")
        alert.addButton(withTitle: "Later")
        if !release.notes.isEmpty {
            let scroll = NSScrollView(frame: NSRect(x: 0, y: 0, width: 360, height: 160))
            scroll.borderType = .bezelBorder
            scroll.hasVerticalScroller = true
            let tv = NSTextView(frame: scroll.bounds)
            tv.isEditable = false; tv.drawsBackground = false
            tv.font = .systemFont(ofSize: 12); tv.string = release.notes
            scroll.documentView = tv
            alert.accessoryView = scroll
        }
        return alert.runModal() == .alertFirstButtonReturn
    }

    // MARK: - Network

    private func fetchLatest() async throws -> ReleaseInfo {
        let url = URL(string: "https://api.github.com/repos/\(Self.repo)/releases/latest")!
        var req = URLRequest(url: url)
        req.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        req.setValue("TokenViewer/\(currentVersion)", forHTTPHeaderField: "User-Agent")
        req.timeoutInterval = 15
        let (data, resp) = try await URLSession.shared.data(for: req)
        guard let http = resp as? HTTPURLResponse, (200...299).contains(http.statusCode) else {
            throw URLError(.badServerResponse)
        }
        let json = try JSONDecoder().decode(GitHubRelease.self, from: data)
        let version = json.tagName.trimmingCharacters(in: .whitespaces)
            .replacingOccurrences(of: #"^[vV]"#, with: "", options: .regularExpression)
        guard !version.isEmpty else { throw URLError(.cannotParseResponse) }
        let releaseURL = json.htmlURL ?? URL(string: "https://github.com/\(Self.repo)/releases/latest")!
        let pkgURL = json.assets.first { $0.browserDownloadURL.pathExtension.lowercased() == "pkg" }?.browserDownloadURL
            ?? json.assets.first { $0.browserDownloadURL.pathExtension.lowercased() == "dmg" }?.browserDownloadURL
        let notes = json.body?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return ReleaseInfo(version: version, releaseURL: releaseURL, pkgURL: pkgURL, notes: notes)
    }

    private func isNewer(_ a: String, than b: String) -> Bool {
        let pa = a.split(separator: ".").map { Int($0) ?? 0 }
        let pb = b.split(separator: ".").map { Int($0) ?? 0 }
        for i in 0..<max(pa.count, pb.count) {
            let x = i < pa.count ? pa[i] : 0
            let y = i < pb.count ? pb[i] : 0
            if x != y { return x > y }
        }
        return false
    }
}

// MARK: - Decodable

private struct GitHubRelease: Decodable {
    let tagName: String
    let htmlURL: URL?
    let body: String?
    let assets: [Asset]
    struct Asset: Decodable {
        let browserDownloadURL: URL
        private enum CodingKeys: String, CodingKey { case browserDownloadURL = "browser_download_url" }
    }
    private enum CodingKeys: String, CodingKey {
        case tagName = "tag_name"; case htmlURL = "html_url"; case body; case assets
    }
}
