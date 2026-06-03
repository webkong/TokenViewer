import Foundation

@MainActor
final class UpdateChecker: ObservableObject {
    static let shared = UpdateChecker()

    /// GitHub repo to check releases against.
    static let repo = "webkong/TokenViewer"

    @Published var status: String = ""
    @Published var busy = false
    @Published var latestURL: URL?

    private var currentVersion: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.1.0"
    }

    func check() {
        busy = true
        status = "Checking…"
        latestURL = nil
        Task { [weak self] in
            guard let self else { return }
            let result = await Self.fetchLatest()
            await MainActor.run {
                self.busy = false
                guard let (tag, htmlURL) = result else {
                    self.status = "Unable to check for updates"
                    return
                }
                let latest = tag.hasPrefix("v") ? String(tag.dropFirst()) : tag
                if Self.isNewer(latest, than: self.currentVersion) {
                    self.status = "Update available: \(tag)"
                    self.latestURL = htmlURL
                } else {
                    self.status = "You're on the latest version (\(self.currentVersion))"
                }
            }
        }
    }

    private static func fetchLatest() async -> (String, URL?)? {
        guard let url = URL(string: "https://api.github.com/repos/\(repo)/releases/latest") else { return nil }
        var req = URLRequest(url: url); req.timeoutInterval = 10
        req.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        guard let (data, resp) = try? await URLSession.shared.data(for: req),
              let http = resp as? HTTPURLResponse, http.statusCode == 200,
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let tag = json["tag_name"] as? String else { return nil }
        let htmlURL = (json["html_url"] as? String).flatMap(URL.init)
        return (tag, htmlURL)
    }

    /// Semver compare a vs b (dot-separated numeric).
    private static func isNewer(_ a: String, than b: String) -> Bool {
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
