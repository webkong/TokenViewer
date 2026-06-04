import SwiftUI
import ServiceManagement

struct SettingsView: View {
    @AppStorage("syncFrequencyMinutes") private var syncFrequency: Int = 30
    @AppStorage("panelShowSummary") private var panelShowSummary = true
    @AppStorage("panelShowLimits") private var panelShowLimits = true
    @AppStorage("panelShowHeatmap") private var panelShowHeatmap = true
    @AppStorage("panelShowTrend") private var panelShowTrend = true
    @AppStorage("panelShowModels") private var panelShowModels = true
    @State private var launchAtLogin = false
    @State private var showRebuildAlert = false
    @State private var providers: [ProviderStatus] = []
    @ObservedObject private var theme = ThemeManager.shared
    @ObservedObject private var currency = CurrencyStore.shared
    @ObservedObject private var l10n = L10n.shared
    @ObservedObject private var viewModel = UsageViewModel.shared

    private let dataDir: String = {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return "\(home)/.tokenviewer"
    }()

    var body: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 16) {
                Text(l10n.settingsTitle)
                    .font(.system(size: 24, weight: .bold))

                generalSection
                appearanceSection
                panelSection
                providersSection
                dataSection
            }
            .padding(20)
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .onAppear {
            if #available(macOS 13.0, *) {
                launchAtLogin = SMAppService.mainApp.status == .enabled
            }
            loadProviders()
        }
    }

    // MARK: Appearance

    private var appearanceSection: some View {
        SettingsCard(title: l10n.appearance) {
            HStack {
                Text(l10n.theme).font(.system(size: 13))
                Spacer()
                Picker("", selection: $theme.theme) {
                    Text(l10n.themeLight).tag(AppTheme.light.rawValue)
                    Text(l10n.themeDark).tag(AppTheme.dark.rawValue)
                    Text(l10n.themeSystem).tag(AppTheme.system.rawValue)
                }
                .pickerStyle(.segmented).labelsHidden().frame(width: 200)
            }
            Divider()
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(l10n.currency).font(.system(size: 13))
                    if currency.currency != "USD" {
                        Text("1 USD = \(String(format: "%.4f", currency.rate)) \(currency.currency)")
                            .font(.system(size: 10)).foregroundStyle(.secondary)
                    }
                }
                Spacer()
                Picker("", selection: $currency.currency) {
                    ForEach(CurrencyStore.supported, id: \.code) { c in
                        Text("\(c.code) \(c.symbol)").tag(c.code)
                    }
                }
                .pickerStyle(.menu).labelsHidden().frame(width: 120)
            }
            Divider()
            HStack {
                Text(l10n.languageLabel).font(.system(size: 13))
                Spacer()
                Picker("", selection: $l10n.language) {
                    ForEach(AppLanguage.allCases, id: \.self) { lang in
                        Text(lang.displayName).tag(lang)
                    }
                }
                .pickerStyle(.menu).labelsHidden().frame(width: 120)
            }
        }
    }

    // MARK: Menu Bar Panel

    private var panelSection: some View {
        SettingsCard(title: l10n.menuBarPanel) {
            Text(l10n.menuBarPanelDesc)
                .font(.system(size: 11)).foregroundStyle(.secondary)
            Divider()
            Toggle(l10n.summary, isOn: $panelShowSummary)
            Toggle(l10n.limits, isOn: $panelShowLimits)
            Toggle(l10n.trend, isOn: $panelShowTrend)
            Toggle(l10n.heatmap, isOn: $panelShowHeatmap)
            Toggle(l10n.topModels, isOn: $panelShowModels)
        }
    }

    // MARK: General

    private var generalSection: some View {
        SettingsCard(title: l10n.general) {
            Toggle(l10n.launchAtLogin, isOn: $launchAtLogin)
                .onChange(of: launchAtLogin) {
                    if #available(macOS 13.0, *) {
                        if launchAtLogin {
                            try? SMAppService.mainApp.register()
                        } else {
                            SMAppService.mainApp.unregister { _ in }
                        }
                    }
                }
            Divider()
            HStack {
                Text(l10n.syncFrequency).font(.system(size: 13))
                Picker("", selection: $syncFrequency) {
                    Text("2 min").tag(2)
                    Text("5 min").tag(5)
                    Text("15 min").tag(15)
                    Text("30 min").tag(30)
                    Text("1 hour").tag(60)
                    Text(l10n.manual).tag(0)
                }
                .pickerStyle(.menu).labelsHidden().frame(width: 100)
                .onChange(of: syncFrequency) { UsageViewModel.shared.startAutoSync() }
                Spacer()
                Button(action: { UsageViewModel.shared.sync() }) {
                    Image(systemName: "arrow.triangle.2.circlepath")
                        .font(.system(size: 13))
                        .rotationEffect(.degrees(viewModel.isLoading ? 360 : 0))
                        .animation(viewModel.isLoading
                            ? .linear(duration: 1).repeatForever(autoreverses: false)
                            : .default, value: viewModel.isLoading)
                }
                .buttonStyle(.plain)
                .disabled(viewModel.isLoading)
                .help(l10n.syncNow)
            }
        }
    }

    // MARK: Providers

    private var providersSection: some View {
        SettingsCard(title: l10n.providers) {
            let active = providers.filter { $0.record_count > 0 }
            if active.isEmpty {
                Text(l10n.noProviderData)
                    .font(.system(size: 12)).foregroundStyle(.secondary)
            } else {
                ForEach(active) { p in
                    HStack(spacing: 8) {
                        ProviderIcon(source: p.source, size: 14)
                        Text(p.source.capitalized).font(.system(size: 13, weight: .medium))
                        Spacer()
                        Text(l10n.recordsCount(Int(p.record_count)))
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(.secondary)
                    }
                    if p.id != active.last?.id { Divider() }
                }
            }
            Text(l10n.activeCount(active.count))
                .font(.system(size: 11)).foregroundStyle(.tertiary)
                .padding(.top, 2)
        }
    }

    // MARK: Data

    private var dataSection: some View {
        SettingsCard(title: l10n.dataManagement) {
            HStack {
                Text(l10n.directory).font(.system(size: 13))
                Spacer()
                Text(dataDir).font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(.secondary).textSelection(.enabled).lineLimit(1).truncationMode(.middle)
            }
            Divider()
            HStack {
                Button(l10n.openInFinder) {
                    NSWorkspace.shared.open(URL(fileURLWithPath: dataDir))
                }
                .font(.system(size: 13))
                Spacer()
            }

            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Button(l10n.rebuildData) { showRebuildAlert = true }
                        .font(.system(size: 13))
                        .disabled(viewModel.isLoading)
                    Spacer()
                }
                Text(l10n.rebuildDataHint)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                Text(l10n.rebuildDataDesc)
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)
            }
            .alert(l10n.rebuildConfirm, isPresented: $showRebuildAlert) {
                Button(l10n.cancel, role: .cancel) {}
                Button(l10n.rebuildData, role: .destructive) { UsageViewModel.shared.rebuildData() }
            } message: {
                Text(l10n.rebuildDataDesc)
            }
        }
    }

    private func loadProviders() {
        Task.detached {
            let data = CoreBridge.shared.getProviderStatus()
            let decoded = data.flatMap { try? JSONDecoder().decode([ProviderStatus].self, from: $0) } ?? []
            await MainActor.run { providers = decoded }
        }
    }

}

/// Rounded card container matching the dashboard's Card primitive.
struct SettingsCard<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title).font(.system(size: 12, weight: .semibold)).foregroundStyle(.secondary)
                .textCase(.uppercase)
            VStack(alignment: .leading, spacing: 10) { content }
                .padding(14)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(
                    RoundedRectangle(cornerRadius: 12)
                        .fill(Color(nsColor: .controlBackgroundColor))
                        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(.quaternary, lineWidth: 0.5))
                )
        }
    }
}
