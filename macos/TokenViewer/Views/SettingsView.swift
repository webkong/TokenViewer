import SwiftUI
import ServiceManagement

struct SettingsView: View {
    @AppStorage("syncFrequencyMinutes") private var syncFrequency: Int = 10
    @AppStorage("panelShowSummary") private var panelShowSummary = true
    @AppStorage("panelShowLimits") private var panelShowLimits = true
    @AppStorage("panelShowHeatmap") private var panelShowHeatmap = true
    @AppStorage("panelShowTrend") private var panelShowTrend = true
    @AppStorage("panelShowModels") private var panelShowModels = true
    @AppStorage("showDockIcon") private var showDockIcon = false
    @AppStorage("showMenuBarIcon") private var showMenuBarIcon = true
    @AppStorage("limitsVisibleSources") private var limitsVisibleSources = LimitsVisibilityStore.defaultsValue
    @AppStorage("sessionYoloConfirmed") private var sessionYoloConfirmed = false
    @State private var pendingYoloSource: String? = nil
    @State private var launchAtLogin = false
    @State private var showRebuildAlert = false
    @State private var showResetSettingsAlert = false
    @State private var codexHomes: [CodexHomeInfo] = []
    @State private var newCodexHome = ""
    @State private var isScanningCodexHomes = false
    @ObservedObject private var theme = ThemeManager.shared
    @ObservedObject private var currency = CurrencyStore.shared
    @ObservedObject private var l10n = L10n.shared
    @ObservedObject private var viewModel = UsageViewModel.shared
    @ObservedObject private var agentRegistry = AgentRegistry.shared
    @ObservedObject private var router = MainWindowRouter.shared
    @ObservedObject private var sessionYoloStore = SessionYoloStore.shared

    private let dataDir: String = {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return "\(home)/.tokenviewer"
    }()

    @State private var selectedSection: String = "general"

    var body: some View {
        HStack(spacing: 0) {
            // MARK: Sidebar
            List(selection: $selectedSection) {
                Section(l10n.settingsTitle) {
                    sidebarItem(id: "general", title: l10n.general, icon: "gear")
                    sidebarItem(id: "appearance", title: l10n.appearance, icon: "paintpalette")
                    sidebarItem(id: "menuBar", title: l10n.menuBarSectionTitle, icon: "menubar.rectangle")
                    sidebarItem(id: "sessions", title: l10n.sessions, icon: "bubble.left.and.bubble.right")
                    sidebarItem(id: "chatgpt", title: l10n.codexHomesTitle, icon: "terminal")
                    sidebarItem(id: "skills", title: l10n.skills, icon: "puzzlepiece.extension")
                    sidebarItem(id: "data", title: l10n.dataManagement, icon: "externaldrive")
                }
            }
            .listStyle(.sidebar)
            .frame(width: 200)

            Divider()

            // MARK: Content
            ScrollView(showsIndicators: false) {
                selectedSettingsSection
                    .padding(24)
                    .frame(maxWidth: .infinity, alignment: .topLeading)
            }
            .background(Color(nsColor: .windowBackgroundColor))
        }
        .clearInitialFocus(trigger: selectedSection)
        .onAppear {
            selectedSection = router.settingsSection
            if #available(macOS 13.0, *) {
                launchAtLogin = SMAppService.mainApp.status == .enabled
            }
            agentRegistry.loadIfNeeded()
            agentRegistry.refreshInstallStatus()
        }
        .onChange(of: router.settingsSection) { _, section in
            selectedSection = section
        }
    }

    @ViewBuilder
    private var selectedSettingsSection: some View {
        switch selectedSection {
        case "appearance": appearanceSection
        case "menuBar": menuBarSection
        case "sessions": sessionsSection
        case "chatgpt": codexHomesSection
        case "skills": skillsSection
        case "data": dataSection
        default: generalSection
        }
    }

    // MARK: ChatGPT / Codex homes

    private var codexHomesSection: some View {
        SettingsCard(title: l10n.codexHomesTitle) {
            Text(l10n.codexHomesDescription)
                .font(.system(size: 11))
                .foregroundStyle(.secondary)

            HStack(spacing: 6) {
                TextField(l10n.codexHomePlaceholder, text: $newCodexHome)
                    .textFieldStyle(.roundedBorder)
                Button(l10n.skillInstallChooseFolder) {
                    chooseCodexHome()
                }
                .tvActionButton(.secondary)
                Button(l10n.add) {
                    addCodexHome()
                }
                .tvActionButton(.primary)
                .disabled(newCodexHome.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                Button {
                    refreshCodexHomes(force: true)
                } label: {
                    TVSymbol(name: "arrow.triangle.2.circlepath")
                        .rotationEffect(.degrees(isScanningCodexHomes ? 360 : 0))
                        .animation(
                            isScanningCodexHomes
                                ? .linear(duration: 1).repeatForever(autoreverses: false)
                                : .default,
                            value: isScanningCodexHomes
                        )
                }
                .tvIconButton()
                .disabled(isScanningCodexHomes)
                .help(l10n.codexHomesRescan)
            }

            Divider()

            if codexHomes.isEmpty && !isScanningCodexHomes {
                Text(l10n.codexHomesEmpty)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            } else {
                VStack(spacing: 7) {
                    ForEach(codexHomes) { item in
                        HStack(spacing: 8) {
                            Image(systemName: item.exists ? "folder.fill" : "folder.badge.questionmark")
                                .foregroundStyle(item.exists ? Color.accentColor : Color.secondary)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(item.path)
                                    .font(.system(size: 11, design: .monospaced))
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                                    .textSelection(.enabled)
                                HStack(spacing: 6) {
                                    Text(l10n.codexHomeSource(item.source))
                                    if item.hasSessions { Text(l10n.codexHomeSessions) }
                                    if item.hasAuth { Text(l10n.codexHomeAuth) }
                                    if !item.exists { Text(l10n.codexHomeMissing) }
                                }
                                .font(.system(size: 9))
                                .foregroundStyle(.secondary)
                            }
                            Spacer()
                            if item.exists {
                                Button {
                                    NSWorkspace.shared.open(URL(fileURLWithPath: item.path))
                                } label: {
                                    TVSymbol(name: "folder")
                                }
                                .tvIconButton()
                                .help(l10n.openInFinder)
                            }
                            if item.isUserConfigured {
                                Button(role: .destructive) {
                                    removeCodexHome(item.path)
                                } label: {
                                    TVSymbol(name: "trash", color: .red)
                                }
                                .tvIconButton()
                                .help(l10n.skillDelete)
                            }
                        }
                        .padding(.vertical, 2)
                    }
                }
            }
        }
        .onAppear {
            if codexHomes.isEmpty {
                refreshCodexHomes(force: false)
            }
        }
    }

    private func refreshCodexHomes(force: Bool) {
        guard !isScanningCodexHomes else { return }
        isScanningCodexHomes = true
        Task {
            let homes = await Task.detached {
                CoreBridge.shared.getCodexHomes(force: force)
            }.value
            codexHomes = homes
            isScanningCodexHomes = false
        }
    }

    private func chooseCodexHome() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = l10n.add
        if panel.runModal() == .OK, let url = panel.url {
            newCodexHome = url.path
        }
    }

    private func addCodexHome() {
        let path = newCodexHome.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !path.isEmpty else { return }
        var paths = codexHomes.filter(\.isUserConfigured).map(\.path)
        if !paths.contains(path) { paths.append(path) }
        saveCodexHomes(paths)
    }

    private func removeCodexHome(_ path: String) {
        saveCodexHomes(codexHomes.filter { $0.isUserConfigured && $0.path != path }.map(\.path))
    }

    private func saveCodexHomes(_ paths: [String]) {
        isScanningCodexHomes = true
        Task {
            let result = await Task.detached { () -> ([CodexHomeInfo]?, String?) in
                switch CoreBridge.shared.setCodexAdditionalHomes(paths) {
                case .success(let homes): return (homes, nil)
                case .failure(let error): return (nil, error.localizedDescription)
                }
            }.value
            isScanningCodexHomes = false
            if let homes = result.0 {
                codexHomes = homes
                newCodexHome = ""
                ToastCenter.shared.success(l10n.toastSaved)
            } else {
                ToastCenter.shared.error(result.1 ?? l10n.toastSaveFailed)
            }
        }
    }

    private func sidebarItem(id: String, title: String, icon: String) -> some View {
        Label(title, systemImage: icon)
            .tag(id)
    }

    // MARK: Appearance

    private var appearanceSection: some View {
        SettingsCard(title: l10n.appearance) {
            HStack {
                Text(l10n.theme).font(.system(size: 13))
                Spacer()
                TVSegmentedPicker(
                    selection: $theme.theme,
                    options: [
                        (AppTheme.light.rawValue, l10n.themeLight),
                        (AppTheme.dark.rawValue, l10n.themeDark),
                        (AppTheme.system.rawValue, l10n.themeSystem),
                    ],
                    itemWidth: 72
                )
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
                .pickerStyle(.menu)
                .tvSelect(width: 120)
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
                .pickerStyle(.menu)
                .tvSelect(width: 120)
            }
        }
    }

    // MARK: Menu Bar

    private var menuBarSection: some View {
        let visible = LimitsVisibilityStore.visibleSet(from: limitsVisibleSources)

        return SettingsCard(title: l10n.menuBarSectionTitle) {
            // Popover panels
            Text(l10n.menuBarPanelDesc)
                .font(.system(size: 11, weight: .medium))
            HStack(spacing: 8) {
                panelChip(title: l10n.summary, isSelected: true, isLocked: true) {}
                panelChip(title: l10n.limits, isSelected: true, isLocked: true) {}
                panelChip(title: l10n.trend, isSelected: panelShowTrend) {
                    panelShowTrend.toggle()
                }
                panelChip(title: l10n.heatmap, isSelected: panelShowHeatmap) {
                    panelShowHeatmap.toggle()
                }
                panelChip(title: l10n.topModels, isSelected: panelShowModels) {
                    panelShowModels.toggle()
                }
            }

            Divider()

            // Limits card agent visibility
            Text(l10n.limitsVisibilityDesc)
                .font(.system(size: 11, weight: .medium))
            FlowLayout(itemSpacing: 6, rowSpacing: 6) {
                ForEach(agentRegistry.sortedLimitAgents) { agent in
                    agentChip(
                        source: agent.source,
                        label: AgentRegistry.shared.displayName(for: agent.source),
                        isSelected: visible.contains(agent.source),
                        isInstalled: agent.isInstalled
                    ) {
                        toggleLimitsVisibility(agent.source)
                    }
                }
            }
        }
    }

    // MARK: Sessions

    private struct YoloAgentRow: Identifiable {
        let source: String
        let displayName: String
        let isInstalled: Bool
        let supportsYolo: Bool
        var id: String { source }
    }

    /// Union of Orca's YOLO flag map and TokenViewer's own agent registry, so
    /// both agents that support YOLO params and those that don't are shown.
    private var yoloAgentRows: [YoloAgentRow] {
        var rows: [String: YoloAgentRow] = [:]
        for (source, args) in SessionCommandRegistry.yoloArgsBySource {
            rows[source] = YoloAgentRow(
                source: source,
                displayName: AgentRegistry.shared.displayName(for: source),
                isInstalled: AgentRegistry.shared.isInstalled(for: source),
                supportsYolo: !args.isEmpty
            )
        }
        for agent in AgentRegistry.shared.allAgents {
            if rows[agent.source] == nil {
                rows[agent.source] = YoloAgentRow(
                    source: agent.source,
                    displayName: agent.displayName,
                    isInstalled: agent.isInstalled,
                    supportsYolo: false
                )
            }
        }
        return rows.values.sorted {
            $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending
        }
    }

    private var installedYoloAgents: [YoloAgentRow] { yoloAgentRows.filter(\.isInstalled) }
    private var notInstalledYoloAgents: [YoloAgentRow] { yoloAgentRows.filter { !$0.isInstalled } }

    private var sessionsSection: some View {
        SettingsCard(title: l10n.sessions) {
            VStack(alignment: .leading, spacing: 4) {
                Text(l10n.sessionYoloListTitle).font(.system(size: 13, weight: .medium))
                Text(l10n.sessionYoloListDesc)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }
            Divider()
            if !installedYoloAgents.isEmpty {
                yoloGroupHeader(l10n.sessionInstalledGroup)
                VStack(spacing: 8) {
                    ForEach(installedYoloAgents) { sessionYoloRow($0) }
                }
            }
            if !installedYoloAgents.isEmpty && !notInstalledYoloAgents.isEmpty {
                Divider()
            }
            if !notInstalledYoloAgents.isEmpty {
                yoloGroupHeader(l10n.sessionNotInstalledGroup)
                VStack(spacing: 8) {
                    ForEach(notInstalledYoloAgents) { sessionYoloRow($0) }
                }
            }
        }
        .alert(l10n.sessionYoloConfirmTitle, isPresented: yoloConfirmBinding) {
            Button(l10n.cancel, role: .cancel) { pendingYoloSource = nil }
            Button(l10n.sessionYoloConfirmButton, role: .destructive) {
                sessionYoloConfirmed = true
                if let source = pendingYoloSource {
                    sessionYoloStore.setEnabled(source, true)
                }
                pendingYoloSource = nil
            }
        } message: {
            Text(l10n.sessionYoloConfirmMessage)
        }
    }

    private func yoloGroupHeader(_ title: String) -> some View {
        Text(title)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(.secondary)
    }

    private func sessionYoloRow(_ agent: YoloAgentRow) -> some View {
        HStack(spacing: 10) {
            AgentIcon(source: agent.source, size: 16)
                .opacity(agent.isInstalled ? 1 : 0.45)
            Text(agent.displayName)
                .font(.system(size: 12, weight: .medium))
                .frame(width: 120, alignment: .leading)
                .lineLimit(1)
            if agent.supportsYolo {
                TextField(l10n.sessionYoloArgsPlaceholder, text: yoloArgsBinding(agent.source))
                    .textFieldStyle(.roundedBorder)
                    .font(.system(size: 11, design: .monospaced))
                    .disabled(!(sessionYoloStore.config(for: agent.source)?.enabled ?? false))
                Toggle("", isOn: yoloEnabledBinding(agent.source))
                    .labelsHidden()
                    .toggleStyle(.switch)
                    .help(l10n.sessionYoloToggleHelp)
            } else {
                Text(l10n.sessionYoloUnsupported)
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)
                Spacer()
            }
        }
    }

    private func yoloEnabledBinding(_ source: String) -> Binding<Bool> {
        Binding(
            get: { sessionYoloStore.config(for: source)?.enabled ?? false },
            set: { newValue in
                if newValue && !sessionYoloConfirmed {
                    pendingYoloSource = source
                } else {
                    sessionYoloStore.setEnabled(source, newValue)
                }
            }
        )
    }

    private func yoloArgsBinding(_ source: String) -> Binding<String> {
        Binding(
            get: { sessionYoloStore.config(for: source)?.args ?? "" },
            set: { sessionYoloStore.setArgs(source, $0) }
        )
    }

    private var yoloConfirmBinding: Binding<Bool> {
        Binding(
            get: { pendingYoloSource != nil },
            set: { if !$0 { pendingYoloSource = nil } }
        )
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
            VStack(alignment: .leading, spacing: 2) {
                Toggle(l10n.showMenuBarIcon, isOn: $showMenuBarIcon)
                    .onChange(of: showMenuBarIcon) {
                        if !showMenuBarIcon && !showDockIcon {
                            // Never allow hiding both — the user would have no
                            // way left to open or interact with the app.
                            showDockIcon = true
                            NSApp.setActivationPolicy(.regular)
                            NSApp.activate(ignoringOtherApps: true)
                            ToastCenter.shared.error(l10n.showBothHiddenWarning)
                        }
                        StatusBarController.shared.setMenuBarIconVisible(showMenuBarIcon)
                    }
                Text(l10n.showMenuBarIconDesc)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }
            Divider()
            VStack(alignment: .leading, spacing: 2) {
                Toggle(l10n.showDockIcon, isOn: $showDockIcon)
                    .onChange(of: showDockIcon) {
                        if !showDockIcon && !showMenuBarIcon {
                            // Same safeguard in the other direction.
                            showMenuBarIcon = true
                            StatusBarController.shared.setMenuBarIconVisible(true)
                            ToastCenter.shared.error(l10n.showBothHiddenWarning)
                        }
                        NSApp.setActivationPolicy(showDockIcon ? .regular : .accessory)
                        if showDockIcon {
                            NSApp.activate(ignoringOtherApps: true)
                        }
                    }
                Text(l10n.showDockIconDesc)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }
            Divider()
            HStack {
                Text(l10n.syncFrequency).font(.system(size: 13))
                Picker("", selection: $syncFrequency) {
                    Text(l10n.sync5min).tag(5)
                    Text(l10n.sync10min).tag(10)
                    Text(l10n.sync15min).tag(15)
                    Text(l10n.sync30min).tag(30)
                    Text(l10n.sync1hour).tag(60)
                    Text(l10n.manual).tag(0)
                }
                .pickerStyle(.menu)
                .tvSelect(width: 112)
                .onChange(of: syncFrequency) { UsageViewModel.shared.startAutoSync() }
                Spacer()
                Button(action: {
                    AppSyncCoordinator.shared.syncAll()
                    ToastCenter.shared.success(l10n.toastSynced)
                }) {
                    TVSymbol(name: "arrow.triangle.2.circlepath")
                        .rotationEffect(.degrees(viewModel.isLoading ? 360 : 0))
                        .animation(viewModel.isLoading
                            ? .linear(duration: 1).repeatForever(autoreverses: false)
                            : .default, value: viewModel.isLoading)
                }
                .tvIconButton()
                .disabled(viewModel.isLoading)
                .help(l10n.syncNow)
            }
        }
    }

    // MARK: Data

    private var dataSection: some View {
        SettingsCard(title: l10n.dataManagement) {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Text(l10n.directory).font(.system(size: 13))
                    Spacer()
                    Text(dataDir).font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(.secondary).textSelection(.enabled).lineLimit(1).truncationMode(.middle)
                    Spacer(minLength: 12)
                    Button(l10n.openInFinder) {
                        NSWorkspace.shared.open(URL(fileURLWithPath: dataDir))
                    }
                    .tvActionButton(.secondary)
                }

                Divider()

                HStack {
                    Text(l10n.rebuildData).font(.system(size: 13))
                    Spacer()
                    Button(l10n.rebuildData) { showRebuildAlert = true }
                        .tvActionButton(.primary)
                        .disabled(viewModel.isLoading)
                }
                Text(l10n.rebuildDataHint)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                Text(l10n.rebuildDataDesc)
                    .font(.system(size: 10))
                    .foregroundStyle(.tertiary)

                Divider()

                HStack {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(l10n.resetSettings).font(.system(size: 13))
                        Text(l10n.resetSettingsDesc)
                            .font(.system(size: 10))
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button(l10n.resetSettings, role: .destructive) {
                        showResetSettingsAlert = true
                    }
                    .tvActionButton(.destructive)
                }
            }
            .alert(l10n.rebuildConfirm, isPresented: $showRebuildAlert) {
                Button(l10n.cancel, role: .cancel) {}
                Button(l10n.rebuildData, role: .destructive) { UsageViewModel.shared.rebuildData() }
            } message: {
                Text(l10n.rebuildDataDesc)
            }
            .alert(l10n.resetSettingsConfirm, isPresented: $showResetSettingsAlert) {
                Button(l10n.cancel, role: .cancel) {}
                Button(l10n.resetSettings, role: .destructive) {
                    resetSettings()
                }
            } message: {
                Text(l10n.resetSettingsConfirmMessage)
            }
        }
    }

    private func resetSettings() {
        AppFocus.clear()

        syncFrequency = 10
        panelShowSummary = true
        panelShowLimits = true
        panelShowHeatmap = true
        panelShowTrend = true
        panelShowModels = true
        showDockIcon = false
        showMenuBarIcon = true
        StatusBarController.shared.setMenuBarIconVisible(true)
        NSApp.setActivationPolicy(.accessory)
        limitsVisibleSources = LimitsVisibilityStore.defaultsValue
        sessionYoloConfirmed = false
        sessionYoloStore.reset()
        enabledAgentsJSON = AgentRegistry.defaultAgentSourcesJSON

        theme.theme = AppTheme.system.rawValue
        l10n.language = .system
        currency.currency = "USD"
        currency.rate = 1.0
        currency.rateFetchedAt = nil
        UserDefaults.standard.removeObject(forKey: "currencyRate")

        UsageViewModel.shared.startAutoSync()
        SkillManagerViewModel.shared.ensureValidFilter()
        SkillManagerViewModel.shared.refresh()
        ToastCenter.shared.success(l10n.toastSettingsReset)
    }

    private func toggleLimitsVisibility(_ source: String) {
        var visible = LimitsVisibilityStore.visibleSet(from: limitsVisibleSources)
        if visible.contains(source) {
            visible.remove(source)
        } else {
            visible.insert(source)
        }
        limitsVisibleSources = LimitsVisibilityStore.rawValue(from: visible)
    }

    private func panelChip(title: String, isSelected: Bool, isLocked: Bool = false, action: @escaping () -> Void) -> some View {
        let fillColor = isSelected ? TVColor.brand : Color(nsColor: .controlBackgroundColor)
        let borderColor: Color = isSelected
            ? TVColor.brand.opacity(0.22)
            : Color(nsColor: .separatorColor).opacity(0.55)
        return Button(action: action) {
            HStack(spacing: 6) {
                Image(systemName: isSelected ? "checkmark.circle.fill" : "circle")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(isSelected ? Color.white.opacity(0.95) : .secondary)
                Text(title)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(isSelected ? Color.white : .primary)
            }
            .padding(.horizontal, 12).padding(.vertical, 8)
            .fixedSize(horizontal: true, vertical: true)
            .background(Capsule().fill(fillColor).overlay(Capsule().strokeBorder(borderColor, lineWidth: 0.75)))
        }
        .buttonStyle(.plain).disabled(isLocked).opacity(isLocked ? 0.82 : 1.0)
    }

    // Shared chip style for agent/agent selection with icon
    private func agentChip(source: String? = nil, label: String, isSelected: Bool, isInstalled: Bool = true, action: @escaping () -> Void) -> some View {
        let fillColor = isSelected ? Color.green.opacity(0.16) : Color(nsColor: .controlBackgroundColor)
        let borderColor: Color = isSelected ? Color.green.opacity(0.35) : Color.secondary.opacity(0.15)
        let textColor: Color = isSelected ? .green : .secondary
        let opacity: CGFloat = isInstalled ? 1.0 : 0.45

        return Button(action: action) {
            HStack(spacing: 5) {
                if let s = source {
                    AgentIcon(source: s, size: 14)
                        .opacity(opacity)
                }
                Text(label)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(textColor)
                    .lineLimit(1)
                if !isInstalled {
                    Image(systemName: "questionmark.circle")
                        .font(.system(size: 9))
                        .foregroundStyle(.tertiary)
                }
            }
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
            .background(Capsule().fill(fillColor))
            .overlay(Capsule().strokeBorder(borderColor, lineWidth: 0.75))
            .opacity(opacity)
        }
        .buttonStyle(.plain)
        .help(isInstalled ? label : l10n.skillNotInstalled(label))
    }

    // MARK: Skills

    @State private var skillsSourceRoot: String = ""
    /// Last value loaded from/saved to the backend, used as "old path" for the move prompt on save.
    @State private var lastSavedSkillsSourceRoot: String = ""
    @AppStorage("skillsEnabledProviders") private var enabledAgentsJSON: String = AgentRegistry.defaultAgentSourcesJSON
    @State private var pendingSkillsMoveOldPath: String? = nil
    @State private var pendingSkillsMoveNewPath: String = ""
    @State private var pendingSkillsOverwriteOldPath: String? = nil
    @State private var pendingSkillsOverwriteNewPath: String = ""

    private var enabledAgents: Set<String> {
        guard let data = enabledAgentsJSON.data(using: .utf8),
              let arr = try? JSONDecoder().decode([String].self, from: data)
        else { return Set(AgentRegistry.defaultAgentSources) }
        return Set(arr)
    }

    private func toggleAgent(_ source: String) {
        var set = enabledAgents
        if set.contains(source) {
            set.remove(source)
        } else {
            set.insert(source)
        }
        guard let data = try? JSONEncoder().encode(Array(set)),
              let json = String(data: data, encoding: .utf8) else { return }
        enabledAgentsJSON = json
    }

    private var skillsSection: some View {
        SettingsCard(title: l10n.skills) {
            // Source root
            VStack(alignment: .leading, spacing: 6) {
                Text(l10n.skillsSourceRoot).font(.system(size: 11, weight: .medium))
                HStack(spacing: 6) {
                    TextField("~/.tokenviewer/skills", text: $skillsSourceRoot)
                        .textFieldStyle(.roundedBorder)
                    Button(l10n.openInFinder) {
                        openSkillsSourceRootInFinder()
                    }
                    .tvActionButton(.secondary)
                    Button(l10n.save) {
                        AppFocus.clear()
                        let trimmedPath = skillsSourceRoot.trimmingCharacters(in: .whitespacesAndNewlines)
                        let oldPath = lastSavedSkillsSourceRoot
                        if !oldPath.isEmpty,
                           !trimmedPath.isEmpty,
                           standardizedSkillsPath(oldPath) != standardizedSkillsPath(trimmedPath) {
                            pendingSkillsMoveOldPath = oldPath
                            pendingSkillsMoveNewPath = trimmedPath
                        } else {
                            saveSkillsSourceRoot(trimmedPath)
                        }
                    }
                    .tvActionButton(.primary)
                }
            }
            .padding(.bottom, 6)

            Divider()

            // Agent participation — chip style
            VStack(alignment: .leading, spacing: 6) {
                Text(l10n.skillAgentParticipation)
                    .font(.system(size: 11, weight: .medium))
                Text(l10n.skillAgentParticipationDesc)
                    .font(.system(size: 10)).foregroundStyle(.secondary)

                if agentRegistry.skillAgents.isEmpty {
                    Text(l10n.loading).font(.system(size: 11)).foregroundStyle(.secondary)
                } else {
                    FlowLayout(itemSpacing: 6, rowSpacing: 6) {
                        ForEach(agentRegistry.sortedSkillAgents) { p in
                            agentChip(
                                source: p.source,
                                label: AgentRegistry.shared.displayName(for: p.source),
                                isSelected: enabledAgents.contains(p.source),
                                isInstalled: p.isInstalled
                            ) {
                                toggleAgent(p.source)
                            }
                        }
                    }
                }
            }
        }
        .onAppear {
            loadSkillsConfig()
            agentRegistry.loadIfNeeded()
            agentRegistry.refreshInstallStatus()
        }
        .alert(
            l10n.skillsMovePromptTitle,
            isPresented: Binding(
                get: { pendingSkillsMoveOldPath != nil },
                set: { if !$0 { pendingSkillsMoveOldPath = nil } }
            )
        ) {
            Button(l10n.skillsMovePromptConfirm) {
                if let oldPath = pendingSkillsMoveOldPath {
                    let newPath = pendingSkillsMoveNewPath
                    if skillsPathsAreNested(oldPath, newPath) {
                        ToastCenter.shared.error(l10n.skillsMoveFailed)
                    } else if FileManager.default.fileExists(atPath: standardizedSkillsPath(newPath)) {
                        pendingSkillsOverwriteOldPath = oldPath
                        pendingSkillsOverwriteNewPath = newPath
                    } else if moveSkills(from: oldPath, to: newPath, overwrite: false) {
                        saveSkillsSourceRoot(
                            newPath,
                            successMessage: l10n.skillsMoveSuccess
                        )
                    }
                }
                pendingSkillsMoveOldPath = nil
            }
            Button(l10n.cancel, role: .cancel) {
                saveSkillsSourceRoot(pendingSkillsMoveNewPath)
                pendingSkillsMoveOldPath = nil
            }
        } message: {
            if let oldPath = pendingSkillsMoveOldPath {
                Text(l10n.skillsMovePromptMessage(oldPath, pendingSkillsMoveNewPath))
            }
        }
        .alert(
            l10n.skillsOverwritePromptTitle,
            isPresented: Binding(
                get: { pendingSkillsOverwriteOldPath != nil },
                set: { if !$0 { pendingSkillsOverwriteOldPath = nil } }
            )
        ) {
            Button(l10n.skillsOverwritePromptConfirm, role: .destructive) {
                if let oldPath = pendingSkillsOverwriteOldPath {
                    let newPath = pendingSkillsOverwriteNewPath
                    if moveSkills(from: oldPath, to: newPath, overwrite: true) {
                        saveSkillsSourceRoot(
                            newPath,
                            successMessage: l10n.skillsMoveSuccess
                        )
                    }
                }
                pendingSkillsOverwriteOldPath = nil
            }
            Button(l10n.cancel, role: .cancel) {
                pendingSkillsOverwriteOldPath = nil
            }
        } message: {
            Text(l10n.skillsOverwritePromptMessage(pendingSkillsOverwriteNewPath))
        }
    }

    private func loadSkillsConfig() {
        Task.detached {
            guard let data = CoreBridge.shared.skillsGetConfig() else { return }
            struct Config: Codable {
                let sourceRoot: String
            }
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            guard let config = try? decoder.decode(Config.self, from: data) else { return }
            await MainActor.run {
                skillsSourceRoot = config.sourceRoot
                lastSavedSkillsSourceRoot = config.sourceRoot
            }
        }
    }

    private func openSkillsSourceRootInFinder() {
        AppFocus.clear()
        let rawPath = skillsSourceRoot.trimmingCharacters(in: .whitespacesAndNewlines)
        let path = rawPath.isEmpty ? "~/.tokenviewer/skills" : rawPath
        let expandedPath = (NSString(string: path).expandingTildeInPath as NSString).standardizingPath
        let url = URL(fileURLWithPath: expandedPath, isDirectory: true)

        do {
            try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
            NSWorkspace.shared.open(url)
        } catch {
            ToastCenter.shared.error(l10n.toastSaveFailed)
        }
    }

    private func saveSkillsSourceRoot(
        _ path: String,
        successMessage: String? = nil
    ) {
        let payload: [String: String] = ["source_root": path]
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let resultData = CoreBridge.shared.skillsSetGitConfig(data),
              let result = try? JSONDecoder().decode(SkillOperationResult.self, from: resultData),
              result.ok else {
            ToastCenter.shared.error(l10n.toastSaveFailed)
            return
        }

        lastSavedSkillsSourceRoot = path
        agentRegistry.reload()
        SkillManagerViewModel.shared.refresh()
        ToastCenter.shared.success(successMessage ?? l10n.toastSaved)
    }

    private func standardizedSkillsPath(_ path: String) -> String {
        (NSString(string: path).expandingTildeInPath as NSString).standardizingPath
    }

    private func skillsPathsAreNested(_ firstPath: String, _ secondPath: String) -> Bool {
        let first = standardizedSkillsPath(firstPath)
        let second = standardizedSkillsPath(secondPath)
        return first.hasPrefix(second + "/") || second.hasPrefix(first + "/")
    }

    /// Moves the entire previous source root to the newly saved path. The destination
    /// is replaced only after the user confirms the destructive overwrite prompt.
    private func moveSkills(from oldRawPath: String, to newRawPath: String, overwrite: Bool) -> Bool {
        let fm = FileManager.default
        let oldPath = standardizedSkillsPath(oldRawPath)
        let newPath = standardizedSkillsPath(newRawPath)
        let oldURL = URL(fileURLWithPath: oldPath, isDirectory: true)
        let newURL = URL(fileURLWithPath: newPath, isDirectory: true)

        var isDir: ObjCBool = false
        guard !skillsPathsAreNested(oldPath, newPath),
              fm.fileExists(atPath: oldURL.path, isDirectory: &isDir),
              isDir.boolValue else {
            ToastCenter.shared.error(l10n.skillsMoveFailed)
            return false
        }

        do {
            try fm.createDirectory(at: newURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            if overwrite {
                try fm.replaceItemAt(newURL, withItemAt: oldURL)
            } else {
                guard !fm.fileExists(atPath: newURL.path) else {
                    ToastCenter.shared.error(l10n.skillsMoveFailed)
                    return false
                }
                try fm.moveItem(at: oldURL, to: newURL)
            }
            return true
        } catch {
            ToastCenter.shared.error(l10n.skillsMoveFailed)
            return false
        }
    }

    private func decodeEnabledAgents() -> Set<String> {
        guard let data = enabledAgentsJSON.data(using: .utf8),
              let arr = try? JSONDecoder().decode([String].self, from: data)
        else { return Set(AgentRegistry.defaultAgentSources) }
        return Set(arr)
    }

    private func encodeEnabledAgents(_ agents: Set<String>) {
        guard let data = try? JSONEncoder().encode(Array(agents)),
              let json = String(data: data, encoding: .utf8)
        else { return }
        enabledAgentsJSON = json
    }
}

struct FlowLayout: Layout {
    var itemSpacing: CGFloat
    var rowSpacing: CGFloat

    struct Cache {
        var sizes: [CGSize] = []
    }

    func makeCache(subviews: Subviews) -> Cache {
        Cache(sizes: subviews.map { $0.sizeThatFits(.unspecified) })
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) -> CGSize {
        let sizes = cache.sizes.count == subviews.count ? cache.sizes : subviews.map { $0.sizeThatFits(.unspecified) }
        let maxWidth = proposal.width ?? .infinity
        var x: CGFloat = 0
        var y: CGFloat = 0
        var rowHeight: CGFloat = 0
        var totalWidth: CGFloat = 0

        for size in sizes {
            if x > 0, x + size.width > maxWidth {
                totalWidth = max(totalWidth, x - itemSpacing)
                x = 0
                y += rowHeight + rowSpacing
                rowHeight = 0
            }
            x += size.width + itemSpacing
            rowHeight = max(rowHeight, size.height)
        }

        totalWidth = max(totalWidth, x > 0 ? x - itemSpacing : 0)
        return CGSize(width: min(totalWidth, maxWidth), height: y + rowHeight)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout Cache) {
        let sizes = cache.sizes.count == subviews.count ? cache.sizes : subviews.map { $0.sizeThatFits(.unspecified) }
        var x = bounds.minX
        var y = bounds.minY
        var rowHeight: CGFloat = 0

        for (index, subview) in subviews.enumerated() {
            let size = sizes[index]
            if x > bounds.minX, x + size.width > bounds.maxX {
                x = bounds.minX
                y += rowHeight + rowSpacing
                rowHeight = 0
            }

            subview.place(
                at: CGPoint(x: x, y: y),
                anchor: .topLeading,
                proposal: ProposedViewSize(width: size.width, height: size.height)
            )
            x += size.width + itemSpacing
            rowHeight = max(rowHeight, size.height)
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

    // MARK: - Agent Skills Row

    fileprivate struct AgentSkillsRow: View {
        let agent: AgentConfig
        let onSave: (String, String?, String?) -> Void
        let onReset: (String) -> Void

        @State private var skillsPath: String = ""
        @State private var linkType: String = "Directory"
        @ObservedObject private var l10n = L10n.shared

        var body: some View {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    AgentIcon(source: agent.source, size: 16)
                    Text(AgentRegistry.shared.displayName(for: agent.source))
                        .font(.system(size: 12, weight: .medium))
                        .frame(width: 100, alignment: .leading)
                    Spacer()
                }

                HStack(spacing: 8) {
                    Text(l10n.skillPathLabel).font(.caption2).foregroundStyle(.secondary).frame(width: 30, alignment: .leading)
                    TextField(agent.skillsPath, text: $skillsPath)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11))
                        .disabled(agent.hasParser == false && skillsPath.isEmpty)
                }

                HStack(spacing: 8) {
                    Text(l10n.skillLinkLabel).font(.caption2).foregroundStyle(.secondary).frame(width: 30, alignment: .leading)
                    Picker("", selection: $linkType) {
                        Text(l10n.skillLinkDirectory).tag("Directory")
                        Text(l10n.skillLinkSingleFile).tag("SingleFile")
                        Text(l10n.skillLinkOverlay).tag("Overlay")
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .controlSize(.mini)

                    Spacer()

                    Button(l10n.reset) {
                        AppFocus.clear()
                        skillsPath = agent.skillsPath
                        linkType = "Directory"
                        onReset(agent.source)
                    }
                    .tvActionButton(.secondary)
                    .disabled(skillsPath == agent.skillsPath && linkType == "Directory")

                    Button(l10n.save) {
                        AppFocus.clear()
                        let path = skillsPath.trimmingCharacters(in: .whitespaces)
                        let pathVal: String? = path.isEmpty || path == agent.skillsPath ? nil : path
                        let ltVal: String? = linkType == "Directory" ? nil : linkType
                        onSave(agent.source, pathVal, ltVal)
                    }
                    .tvActionButton(.primary)
                }
            }
            .onAppear {
                skillsPath = agent.skillsPath
                linkType = agent.linkType
            }
        }
    }
}
