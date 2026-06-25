import SwiftUI

struct AboutView: View {
    @ObservedObject private var updater = UpdateChecker.shared
    @ObservedObject private var l10n = L10n.shared
    @State private var autoDownloadVersion: String?
    @State private var showAgents = false
    @State private var allProviders: [SkillProvider] = []

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

                // Supported agents
                SettingsCard(title: l10n.aboutSupportedAgents) {
                    VStack(alignment: .leading, spacing: 2) {
                        Button {
                            withAnimation { showAgents.toggle() }
                        } label: {
                            HStack {
                                HStack(spacing: 6) {
                                    Image(systemName: "rectangle.stack.fill")
                                        .font(.system(size: 11))
                                        .foregroundStyle(.secondary)
                                    Text(l10n.aboutAgentCount(allProviders.count))
                                        .font(.system(size: 13))
                                }
                                Spacer()
                                HStack(spacing: 12) {
                                    HStack(spacing: 4) {
                                        Circle().fill(.green).frame(width: 6, height: 6)
                                        Text(l10n.aboutLimitsCount(limitsCount)).font(.caption).foregroundStyle(.secondary)
                                    }
                                    Text("·").foregroundStyle(.tertiary)
                                    HStack(spacing: 4) {
                                        Circle().fill(.secondary).frame(width: 6, height: 6)
                                        Text(l10n.aboutOtherCount(otherCount)).font(.caption).foregroundStyle(.secondary)
                                    }
                                    Image(systemName: showAgents ? "chevron.up" : "chevron.down")
                                        .font(.system(size: 11, weight: .semibold))
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                        .buttonStyle(.plain)

                        if showAgents {
                            Divider().padding(.vertical, 6)
                            // Limits agents
                            Text(l10n.aboutWithLimits).font(.system(size: 10, weight: .semibold)).foregroundStyle(.secondary)
                            FlowLayout(itemSpacing: 6, rowSpacing: 6) {
                                ForEach(allProviders.filter(\.hasLimits)) { p in
                                    chip(p)
                                }
                            }
                            Divider().padding(.vertical, 4)
                            // Other agents
                            Text(l10n.aboutWithoutLimits).font(.system(size: 10, weight: .semibold)).foregroundStyle(.secondary)
                            FlowLayout(itemSpacing: 6, rowSpacing: 6) {
                                ForEach(allProviders.filter { !$0.hasLimits }) { p in
                                    chip(p)
                                }
                            }
                        }
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
        .onAppear {
            loadProviders()
        }
    }

    private func loadProviders() {
        guard allProviders.isEmpty else { return }
        Task.detached {
            guard let data = CoreBridge.shared.skillsListAgents() else { return }
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            let providers = (try? decoder.decode([SkillProvider].self, from: data)) ?? []
            await MainActor.run { allProviders = providers }
        }
    }

    private var limitsCount: Int { allProviders.filter(\.hasLimits).count }
    private var otherCount: Int { allProviders.filter { !$0.hasLimits }.count }

    private func chip(_ p: SkillProvider) -> some View {
        HStack(spacing: 5) {
            ProviderIcon(source: p.source, size: 16)
            Text(TVColor.sourceDisplayName(p.source))
                .font(.system(size: 12))
                .lineLimit(1)
        }
        .padding(.horizontal, 10).padding(.vertical, 6)
        .background(Color(nsColor: .controlBackgroundColor), in: Capsule())
        .overlay(Capsule().strokeBorder(.quaternary, lineWidth: 0.5))
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
