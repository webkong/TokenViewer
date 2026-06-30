import SwiftUI
import ServiceManagement

struct SettingsView: View {
    @AppStorage("syncFrequencyMinutes") private var syncFrequency: Int = 30
    @AppStorage("panelShowSummary") private var panelShowSummary = true
    @AppStorage("panelShowLimits") private var panelShowLimits = true
    @AppStorage("panelShowHeatmap") private var panelShowHeatmap = true
    @AppStorage("panelShowTrend") private var panelShowTrend = true
    @AppStorage("panelShowModels") private var panelShowModels = true
    @AppStorage("limitsVisibleSources") private var limitsVisibleSources = LimitsVisibilityStore.defaultsValue
    @State private var launchAtLogin = false
    @State private var showRebuildAlert = false
    @ObservedObject private var theme = ThemeManager.shared
    @ObservedObject private var currency = CurrencyStore.shared
    @ObservedObject private var l10n = L10n.shared
    @ObservedObject private var viewModel = UsageViewModel.shared

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
                    sidebarItem(id: "skills", title: l10n.skills, icon: "puzzlepiece.extension")
                    sidebarItem(id: "data", title: l10n.dataManagement, icon: "externaldrive")
                }
            }
            .listStyle(.sidebar)
            .frame(width: 200)

            Divider()

            // MARK: Content
            ScrollViewReader { proxy in
                ScrollView(showsIndicators: false) {
                    VStack(alignment: .leading, spacing: 16) {
                        generalSection.id("general")
                        appearanceSection.id("appearance")
                        menuBarSection.id("menuBar")
                        skillsSection.id("skills")
                        dataSection.id("data")
                    }
                    .padding(20)
                }
                .background(Color(nsColor: .windowBackgroundColor))
                .onChange(of: selectedSection) { _, new in
                    withAnimation(.easeInOut(duration: 0.25)) {
                        proxy.scrollTo(new, anchor: .top)
                    }
                }
            }
        }
        .clearInitialFocus(trigger: selectedSection)
        .onAppear {
            if #available(macOS 13.0, *) {
                launchAtLogin = SMAppService.mainApp.status == .enabled
            }
            loadSkillProviders()
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
                ForEach(sortedMenuBarProviders) { provider in
                    let isInstalled = agentInstallStatus[provider.source] ?? provider.isInstalled
                    agentChip(
                        source: provider.source,
                        label: ProviderRegistry.shared.displayName(for: provider.source),
                        isSelected: visible.contains(provider.source),
                        isInstalled: isInstalled
                    ) {
                        toggleLimitsVisibility(provider.source)
                    }
                }
            }
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
                    Text(l10n.sync2min).tag(2)
                    Text(l10n.sync5min).tag(5)
                    Text(l10n.sync15min).tag(15)
                    Text(l10n.sync30min).tag(30)
                    Text(l10n.sync1hour).tag(60)
                    Text(l10n.manual).tag(0)
                }
                .pickerStyle(.menu).labelsHidden().frame(width: 100)
                .onChange(of: syncFrequency) { UsageViewModel.shared.startAutoSync() }
                Spacer()
                Button(action: {
                    AppSyncCoordinator.shared.syncAll()
                    ToastCenter.shared.success(l10n.toastSynced)
                }) {
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
                    .font(.system(size: 13))
                }

                Divider()

                HStack {
                    Text(l10n.rebuildData).font(.system(size: 13))
                    Spacer()
                    Button(l10n.rebuildData) { showRebuildAlert = true }
                        .font(.system(size: 13))
                        .buttonStyle(.borderedProminent)
                        .disabled(viewModel.isLoading)
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

    // Shared chip style for agent/provider selection with icon
    private func agentChip(source: String? = nil, label: String, isSelected: Bool, isInstalled: Bool = true, action: @escaping () -> Void) -> some View {
        let fillColor = isSelected ? Color.green.opacity(0.16) : Color(nsColor: .controlBackgroundColor)
        let borderColor: Color = isSelected ? Color.green.opacity(0.35) : Color.secondary.opacity(0.15)
        let textColor: Color = isSelected ? .green : .secondary
        let opacity: CGFloat = isInstalled ? 1.0 : 0.45

        return Button(action: action) {
            HStack(spacing: 5) {
                if let s = source {
                    ProviderIcon(source: s, size: 14)
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
    @State private var skillProviders: [SkillProvider] = []
    @State private var agentInstallStatus: [String: Bool] = [:]
    @AppStorage("skillsEnabledProviders") private var enabledProvidersJSON: String = "[\"claude\",\"codex\",\"opencode\"]"

    private var enabledProviders: Set<String> {
        guard let data = enabledProvidersJSON.data(using: .utf8),
              let arr = try? JSONDecoder().decode([String].self, from: data)
        else { return ["claude", "codex", "opencode"] }
        return Set(arr)
    }

    private func toggleProvider(_ source: String) {
        var set = enabledProviders
        if set.contains(source) {
            set.remove(source)
        } else {
            set.insert(source)
        }
        guard let data = try? JSONEncoder().encode(Array(set)),
              let json = String(data: data, encoding: .utf8) else { return }
        enabledProvidersJSON = json
    }

    private var skillsSection: some View {
        SettingsCard(title: l10n.skills) {
            // Source root
            VStack(alignment: .leading, spacing: 6) {
                Text(l10n.skillsSourceRoot).font(.system(size: 11, weight: .medium))
                HStack(spacing: 6) {
                    TextField("~/.agents/skills", text: $skillsSourceRoot)
                        .textFieldStyle(.roundedBorder)
                    Button(l10n.openInFinder) {
                        let panel = NSOpenPanel()
                        panel.canChooseDirectories = true
                        panel.canChooseFiles = false
                        if panel.runModal() == .OK, let url = panel.url {
                            skillsSourceRoot = url.path
                        }
                    }
                    .font(.system(size: 11))
                    Button(l10n.save) {
                        AppFocus.clear()
                        let payload: [String: String] = ["source_root": skillsSourceRoot.trimmingCharacters(in: .whitespaces)]
                        if let data = try? JSONSerialization.data(withJSONObject: payload) {
                            let resultData = CoreBridge.shared.skillsSetGitConfig(data)
                            if let resultData,
                               let result = try? JSONDecoder().decode(SkillOperationResult.self, from: resultData),
                               result.ok {
                                SkillManagerViewModel.shared.refresh()
                                ToastCenter.shared.success(l10n.toastSaved)
                            } else {
                                ToastCenter.shared.error(l10n.toastSaveFailed)
                            }
                        } else {
                            ToastCenter.shared.error(l10n.toastSaveFailed)
                        }
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
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

                if skillProviders.isEmpty {
                    Text(l10n.loading).font(.system(size: 11)).foregroundStyle(.secondary)
                } else {
                    FlowLayout(itemSpacing: 6, rowSpacing: 6) {
                        ForEach(sortedSkillProviders) { p in
                            let isInstalled = agentInstallStatus[p.source] ?? p.isInstalled
                            agentChip(
                                source: p.source,
                                label: ProviderRegistry.shared.displayName(for: p.source),
                                isSelected: enabledProviders.contains(p.source),
                                isInstalled: isInstalled
                            ) {
                                toggleProvider(p.source)
                            }
                        }
                    }
                }
            }
        }
        .onAppear {
            loadSkillsConfig()
            loadSkillProviders()
        }
    }

    private var sortedSkillProviders: [SkillProvider] {
        skillProviders.sorted { lhs, rhs in
            let lhsInstalled = agentInstallStatus[lhs.source] ?? lhs.isInstalled
            let rhsInstalled = agentInstallStatus[rhs.source] ?? rhs.isInstalled
            if lhsInstalled != rhsInstalled {
                return lhsInstalled && !rhsInstalled
            }
            return lhs.displayName.localizedCaseInsensitiveCompare(rhs.displayName) == .orderedAscending
        }
    }

    private var sortedMenuBarProviders: [SkillProvider] {
        let providers = skillProviders.filter(\.hasLimits)
        if providers.isEmpty {
            return LimitsVisibilityStore.allSources.map { source in
                SkillProvider(
                    source: source,
                    displayName: LimitsVisibilityStore.displayName(for: source),
                    skillsPath: "",
                    linkType: "Directory",
                    isLinked: false,
                    linkedSkills: [],
                    hasParser: false,
                    hasLimits: true,
                    detectCmd: nil,
                    isInstalled: false
                )
            }
        }

        return providers.sorted { lhs, rhs in
            let lhsInstalled = agentInstallStatus[lhs.source] ?? lhs.isInstalled
            let rhsInstalled = agentInstallStatus[rhs.source] ?? rhs.isInstalled
            if lhsInstalled != rhsInstalled {
                return lhsInstalled && !rhsInstalled
            }
            return lhs.displayName.localizedCaseInsensitiveCompare(rhs.displayName) == .orderedAscending
        }
    }

    private func loadSkillProviders() {
        guard skillProviders.isEmpty else { return }
        // Phase 1: load agent list immediately (fast, in-memory read)
        Task.detached {
            guard let data = CoreBridge.shared.skillsListAgents() else { return }
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            let providers = (try? decoder.decode([SkillProvider].self, from: data)) ?? []
            await MainActor.run { skillProviders = providers }
        }
        // Phase 2: detect install status in background (cached → fast; fresh → ~2s)
        Task.detached {
            var installMap: [String: Bool] = [:]
            if let detectData = CoreBridge.shared.skillsDetectInstalled(),
               let detected = try? JSONDecoder().decode([String: Bool].self, from: detectData) {
                installMap = detected
            }
            let finalInstallMap = installMap
            await MainActor.run { agentInstallStatus = finalInstallMap }
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
            }
        }
    }

    private func decodeEnabledProviders() -> Set<String> {
        guard let data = enabledProvidersJSON.data(using: .utf8),
              let arr = try? JSONDecoder().decode([String].self, from: data)
        else { return ["claude", "codex", "opencode"] }
        return Set(arr)
    }

    private func encodeEnabledProviders(_ providers: Set<String>) {
        guard let data = try? JSONEncoder().encode(Array(providers)),
              let json = String(data: data, encoding: .utf8)
        else { return }
        enabledProvidersJSON = json
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

    // MARK: - Provider Skills Row

    fileprivate struct ProviderSkillsRow: View {
        let provider: SkillProvider
        let onSave: (String, String?, String?) -> Void
        let onReset: (String) -> Void

        @State private var skillsPath: String = ""
        @State private var linkType: String = "Directory"

        var body: some View {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    ProviderIcon(source: provider.source, size: 16)
                    Text(ProviderRegistry.shared.displayName(for: provider.source))
                        .font(.system(size: 12, weight: .medium))
                        .frame(width: 100, alignment: .leading)
                    Spacer()
                }

                HStack(spacing: 8) {
                    Text("Path").font(.caption2).foregroundStyle(.secondary).frame(width: 30, alignment: .leading)
                    TextField(provider.skillsPath, text: $skillsPath)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11))
                        .disabled(provider.hasParser == false && skillsPath.isEmpty)
                }

                HStack(spacing: 8) {
                    Text("Link").font(.caption2).foregroundStyle(.secondary).frame(width: 30, alignment: .leading)
                    Picker("", selection: $linkType) {
                        Text("Directory").tag("Directory")
                        Text("Single File").tag("SingleFile")
                        Text("Overlay").tag("Overlay")
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .controlSize(.mini)

                    Spacer()

                    Button("Reset") {
                        AppFocus.clear()
                        skillsPath = provider.skillsPath
                        linkType = "Directory"
                        onReset(provider.source)
                    }
                    .controlSize(.small)
                    .font(.system(size: 10))
                    .disabled(skillsPath == provider.skillsPath && linkType == "Directory")

                    Button("Save") {
                        AppFocus.clear()
                        let path = skillsPath.trimmingCharacters(in: .whitespaces)
                        let pathVal: String? = path.isEmpty || path == provider.skillsPath ? nil : path
                        let ltVal: String? = linkType == "Directory" ? nil : linkType
                        onSave(provider.source, pathVal, ltVal)
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.small)
                    .font(.system(size: 10))
                }
            }
            .onAppear {
                skillsPath = provider.skillsPath
                linkType = provider.linkType
            }
        }
    }
}
