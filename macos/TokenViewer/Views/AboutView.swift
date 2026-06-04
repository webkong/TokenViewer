import SwiftUI

struct AboutView: View {
    @ObservedObject private var updater = UpdateChecker.shared
    @ObservedObject private var l10n = L10n.shared
    @State private var autoDownloadVersion: String?

    private let version = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.1.0"

    var body: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 16) {
                Text(l10n.about).font(.system(size: 24, weight: .bold))

                // App info
                SettingsCard(title: l10n.about) {
                    row("TokenViewer", "v\(version)")
                    Divider()
                    row(l10n.engine, "tokenviewer-core (Rust)")
                    Divider()
                    row(l10n.storage, "SQLite · local-only")
                    Divider()
                    HStack {
                        Text(l10n.github).font(.system(size: 13))
                        Spacer()
                        Button(UpdateChecker.repo) {
                            if let u = URL(string: "https://github.com/\(UpdateChecker.repo)") {
                                NSWorkspace.shared.open(u)
                            }
                        }
                        .buttonStyle(.plain)
                        .font(.system(size: 12))
                        .foregroundStyle(TVColor.brand)
                    }
                }

                // Updates
                SettingsCard(title: l10n.updates) {
                    VStack(alignment: .leading, spacing: 10) {
                        HStack(alignment: .center, spacing: 12) {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(l10n.softwareUpdate)
                                    .font(.system(size: 13))
                                if !updater.status.isEmpty {
                                    Text(updater.status)
                                        .font(.system(size: 10))
                                        .foregroundStyle(.secondary)
                                }
                                if let lastChecked = updater.lastCheckedAt {
                                    Text("\(l10n.lastChecked) \(lastChecked.formatted(date: .abbreviated, time: .shortened))")
                                        .font(.system(size: 10))
                                        .foregroundStyle(.secondary)
                                }
                            }
                            Spacer()
                            updateActionView
                        }

                        if case .failed(let message) = updater.state {
                            Text(message)
                                .font(.system(size: 10))
                                .foregroundStyle(.orange)
                        }
                    }
                }
            }
            .padding(20)

            Text(l10n.copyrightFooter(year: Calendar.current.component(.year, from: Date())))
                .font(.system(size: 11))
                .foregroundStyle(.tertiary)
                .frame(maxWidth: .infinity, alignment: .center)
            Button("tokenviewer.webkong.top") {
                NSWorkspace.shared.open(URL(string: "https://tokenviewer.webkong.top")!)
            }
            .buttonStyle(.plain)
            .font(.system(size: 11))
            .foregroundStyle(TVColor.brand)
            .frame(maxWidth: .infinity, alignment: .center)
            .padding(.bottom, 20)
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .task(id: updater.state) {
            guard case .available(let availableVersion) = updater.state else { return }
            guard autoDownloadVersion != availableVersion else { return }
            autoDownloadVersion = availableVersion
            updater.install(autoTriggered: true)
        }
    }

    private func row(_ label: String, _ value: String) -> some View {
        HStack {
            Text(label).font(.system(size: 13))
            Spacer()
            Text(value).font(.system(size: 12)).foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var updateActionView: some View {
        switch updater.state {
        case .checking, .downloading:
            ProgressView()
                .controlSize(.small)
        case .available:
            Button(l10n.download) { updater.install() }
        default:
            Button(updater.busy ? l10n.checkingUpdates : l10n.checkNow) { updater.check() }
                .disabled(updater.busy)
        }
    }
}
