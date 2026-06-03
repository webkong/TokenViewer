import SwiftUI

struct AboutView: View {
    @ObservedObject private var updater = UpdateChecker.shared
    @ObservedObject private var l10n = L10n.shared

    private let version = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.1.0"

    var body: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 16) {
                Text(l10n.about).font(.system(size: 24, weight: .bold))

                // App info
                SettingsCard(title: l10n.about) {
                    row("TokenViewer", "v\(version)")
                    Divider()
                    row("Engine", "tokenviewer-core (Rust)")
                    Divider()
                    row("Storage", "SQLite · local-only")
                    Divider()
                    HStack {
                        Text("GitHub").font(.system(size: 13))
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
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Software Update").font(.system(size: 13))
                            if !updater.status.isEmpty {
                                Text(updater.status).font(.system(size: 10)).foregroundStyle(.secondary)
                            }
                        }
                        Spacer()
                        if case .available = updater.state {
                            Button(l10n.download) { updater.install() }
                        }
                        Button(updater.busy ? "Checking…" : l10n.checkNow) { updater.check() }
                            .disabled(updater.busy)
                    }
                }
            }
            .padding(20)

            Text("© \(Calendar.current.component(.year, from: Date())) webkong. All rights reserved.")
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
    }

    private func row(_ label: String, _ value: String) -> some View {
        HStack {
            Text(label).font(.system(size: 13))
            Spacer()
            Text(value).font(.system(size: 12)).foregroundStyle(.secondary)
        }
    }
}
