import SwiftUI

enum AppLanguage: String, CaseIterable {
    case system = "system"
    case en = "en"
    case zh = "zh"

    var displayName: String {
        switch self {
        case .system: return "System"
        case .en: return "English"
        case .zh: return "中文"
        }
    }
}

final class L10n: ObservableObject {
    static let shared = L10n()
    @AppStorage("appLanguage") var language: AppLanguage = .system {
        didSet { objectWillChange.send() }
    }

    private var isZh: Bool {
        switch language {
        case .zh: return true
        case .en: return false
        case .system: return Locale.current.language.languageCode?.identifier.hasPrefix("zh") ?? false
        }
    }

    // MARK: - Tabs
    var usage: String { isZh ? "用量" : "Usage" }
    var limits: String { isZh ? "限额" : "Limits" }
    var settings: String { isZh ? "设置" : "Settings" }

    // MARK: - Usage View
    var usageTitle: String { isZh ? "用量" : "Usage" }
    var usageSubtitle: String { isZh ? "所有 AI 工具的 Token 消耗" : "Token consumption across all AI tools" }
    var syncNow: String { isZh ? "立即同步" : "Sync now" }
    // Time range filter
    var rangeToday: String { isZh ? "今天" : "Today" }
    var rangeWeek: String { isZh ? "近 7 天" : "Week" }
    var rangeMonth: String { isZh ? "近 30 天" : "Month" }
    var rangeAll: String { isZh ? "全部" : "All" }
    var rangeCustom: String { isZh ? "自定义" : "Custom" }
    var rangeFrom: String { isZh ? "起始" : "From" }
    var rangeTo: String { isZh ? "结束" : "To" }
    var today: String { isZh ? "今天" : "Today" }
    var sevenDays: String { isZh ? "7 天" : "7 Days" }
    var thirtyDays: String { isZh ? "30 天" : "30 Days" }
    var total: String { isZh ? "总计" : "Total" }
    var active: String { isZh ? "活跃" : "active" }
    var perDay: String { isZh ? "/天" : "/day" }

    // MARK: - Panel / Sections
    var trend: String { isZh ? "趋势" : "Trend" }
    var activity: String { isZh ? "活跃度" : "Activity" }
    var topModels: String { isZh ? "热门模型" : "Top Models" }
    var dashboard: String { isZh ? "仪表盘" : "Dashboard" }
    var quit: String { isZh ? "退出" : "Quit" }

    // MARK: - Trend Chart
    var usageTrend: String { isZh ? "用量趋势" : "Usage Trend" }
    var byDay: String { isZh ? "按天" : "by day" }
    var byHour: String { isZh ? "按小时" : "by hour" }
    var input: String { isZh ? "输入" : "Input" }
    var output: String { isZh ? "输出" : "Output" }
    var cacheRead: String { isZh ? "缓存读取" : "Cache Read" }
    var cost: String { isZh ? "费用" : "Cost" }
    var cacheHit: String { isZh ? "缓存命中" : "Cache hit" }

    // MARK: - Limits
    var limitsTitle: String { isZh ? "限额" : "Limits" }
    var limitsSubtitle: String { isZh ? "各工具配额窗口及重置倒计时" : "Per-agent quota windows with reset countdowns" }
    var limitsVisibilityDesc: String { isZh ? "选择哪些 Agent 显示在菜单栏弹窗的限额卡片里。" : "Choose which agents appear in the menu-bar limits card." }
    var noUsageData: String { isZh ? "暂无数据" : "No usage data" }
    var noLimitsData: String { isZh ? "当前没有限额数据。" : "No limits data yet." }
    var limitsNoDataDesc: String { isZh ? "使用任意支持的 Agent 后，再点击同步查看。或者前往设置勾选要显示的 Agent。" : "Use any supported agent, then sync to view data. You can also open Settings to choose which agents appear." }
    var reset: String { isZh ? "重置" : "Reset" }
    var resets: String { isZh ? "重置" : "Resets" }
    var expires: String { isZh ? "到期" : "Expires" }
    var subscriptionReset: String { isZh ? "订阅重置" : "Subscription reset" }
    var quotaReset: String { isZh ? "额度重置" : "Quota reset" }
    func expiresInDays(_ days: Int) -> String { isZh ? "\(days) 天后到期" : "Expires in \(dayCount(days))" }
    func quotaResetsInDays(_ days: Int) -> String { isZh ? "\(days) 天后重置额度" : "Quota resets in \(dayCount(days))" }
    func subscriptionResetsInDays(_ days: Int) -> String { isZh ? "\(days) 天后重置订阅" : "Subscription resets in \(dayCount(days))" }
    func resetsInDays(_ days: Int) -> String { isZh ? "\(days) 天后重置" : "Resets in \(dayCount(days))" }
    func resetsIn(_ value: String) -> String { isZh ? "\(value) 后重置" : "Resets in \(value)" }
    var refreshingLimits: String { isZh ? "正在刷新限额…" : "Refreshing limits…" }
    var refreshLimits: String { isZh ? "刷新限额" : "Refresh limits" }
    var notConfigured: String { isZh ? "未配置" : "Not configured" }
    var openSettings: String { isZh ? "打开设置" : "Open Settings" }
    var heatmapLess: String { isZh ? "少" : "Less" }
    var heatmapMore: String { isZh ? "多" : "More" }

    // MARK: - Settings
    var settingsTitle: String { isZh ? "设置" : "Settings" }
    var appearance: String { isZh ? "外观" : "Appearance" }
    var theme: String { isZh ? "主题" : "Theme" }
    var themeLight: String { isZh ? "浅色" : "Light" }
    var themeDark: String { isZh ? "深色" : "Dark" }
    var themeSystem: String { isZh ? "跟随系统" : "System" }
    var currency: String { isZh ? "货币" : "Currency" }
    var languageLabel: String { isZh ? "语言" : "Language" }
    var menuBarPanel: String { isZh ? "菜单栏面板" : "Menu Bar Panel" }
    var menuBarPanelDesc: String { isZh ? "选择菜单栏弹窗中显示的板块。" : "Choose which sections appear in the menu-bar popover." }
    var menuBarLimitsCards: String { isZh ? "菜单栏弹窗限额卡片" : "Menu Bar Popover Limits Cards" }
    var summary: String { isZh ? "摘要" : "Summary" }
    var models: String { isZh ? "模型" : "Models" }
    var heatmap: String { isZh ? "热力图" : "Heatmap" }
    var general: String { isZh ? "通用" : "General" }
    var launchAtLogin: String { isZh ? "开机启动" : "Launch at Login" }
    var showDockIcon: String { isZh ? "显示 Dock 图标" : "Show Dock Icon" }
    var showDockIconDesc: String { isZh ? "关闭后 TokenViewer 只在菜单栏显示，不再出现在 Dock 中。" : "When off, TokenViewer only shows in the menu bar and is hidden from the Dock." }
    var showMenuBarIcon: String { isZh ? "显示菜单栏图标" : "Show Menu Bar Icon" }
    var showMenuBarIconDesc: String { isZh ? "关闭后 TokenViewer 不再出现在菜单栏中。" : "When off, TokenViewer's icon is hidden from the menu bar." }
    var showBothHiddenWarning: String { isZh ? "Dock 图标和菜单栏图标不能同时关闭，已自动恢复另一个。" : "Dock and menu bar icons can't both be hidden — the other one was turned back on." }
    var syncFrequency: String { isZh ? "同步频率" : "Sync Frequency" }
    var manual: String { isZh ? "手动" : "Manual" }
    var updates: String { isZh ? "更新" : "Updates" }
    var softwareUpdate: String { isZh ? "软件更新" : "Software Update" }
    var upToDate: String { isZh ? "已是最新版本" : "Up to date" }
    var newVersion: String { isZh ? "有新版本" : "New version available" }
    var download: String { isZh ? "下载" : "Download" }
    var checkNow: String { isZh ? "检查更新" : "Check Now" }
    var lastChecked: String { isZh ? "上次检查" : "Last checked" }
    var checkingUpdates: String { isZh ? "正在检查更新…" : "Checking for updates…" }
    var releaseNotesTitle: String { isZh ? "更新说明" : "Release Notes" }
    var installUpdate: String { isZh ? "安装更新" : "Install Update" }
    var later: String { isZh ? "稍后" : "Later" }
    var updateAvailableMessage: String { isZh ? "现在安装新版本？" : "Install the new version now?" }
    func updateAvailableTitle(version: String) -> String { isZh ? "TokenViewer \(version) 有可用更新" : "TokenViewer \(version) is available" }
    func downloadingUpdate(version: String) -> String { isZh ? "正在下载 v\(version)…" : "Downloading v\(version)…" }
    var checkForUpdatesFailed: String { isZh ? "无法检查更新" : "Could not check for updates" }
    var couldNotOpenInstaller: String { isZh ? "无法打开安装器" : "Could not open installer" }
    var noReleaseNotesAvailable: String { isZh ? "暂无更新说明。" : "No release notes available." }
    func updateAvailableStatus(version: String) -> String { isZh ? "v\(version) 有新版本" : "v\(version) available" }
    var github: String { isZh ? "GitHub" : "GitHub" }
    var engine: String { isZh ? "引擎" : "Engine" }
    var storage: String { isZh ? "存储" : "Storage" }
    func copyrightFooter(year: Int) -> String {
        isZh ? "© \(year) webkong. 保留所有权利。" : "© \(year) webkong. All rights reserved."
    }
    var providers: String { isZh ? "数据源" : "Providers" }
    var noProviderData: String { isZh ? "尚无数据。使用任意 AI 工具后点击同步。" : "No provider data yet. Use any supported AI tool, then Sync." }
    func recordsCount(_ n: Int) -> String { isZh ? "\(n) 条记录" : "\(n) records" }
    func activeCount(_ n: Int) -> String { isZh ? "23 个支持工具中 \(n) 个活跃" : "\(n) of 23 supported tools active" }
    var data: String { isZh ? "数据" : "Data" }
    var dataManagement: String { isZh ? "数据管理" : "Data Management" }
    var directory: String { isZh ? "目录" : "Directory" }
    var openInFinder: String { isZh ? "在 Finder 中打开" : "Open in Finder" }
    var rebuildData: String { isZh ? "重建数据" : "Rebuild Data" }
    var rebuildDataDesc: String { isZh ? "清理已处理的数据和同步游标，然后从原始数据重新拉取。" : "Clears processed data and sync cursors, then rescans raw source files." }
    var rebuildDataHint: String { isZh ? "当数据看起来缺失、过旧，或同步游标异常时使用。" : "Use when data looks stale, missing, or sync cursors are out of date." }
    var rebuildConfirm: String { isZh ? "确认重建" : "Confirm Rebuild" }
    var rebuildDone: String { isZh ? "数据重建完成，请稍后刷新查看。" : "Data rebuild complete. Refresh to view the latest data." }
    var resetSettings: String { isZh ? "重置设置" : "Reset Settings" }
    var resetSettingsDesc: String { isZh ? "恢复主题、语言、货币、同步频率、菜单栏和 Agent 显示偏好。不删除数据、Git 配置或令牌。" : "Restore theme, language, currency, sync frequency, menu bar, and agent display preferences. Data, Git config, and tokens are kept." }
    var resetSettingsConfirm: String { isZh ? "确认重置设置？" : "Reset Settings?" }
    var resetSettingsConfirmMessage: String { isZh ? "这会恢复应用偏好设置，但不会删除用量数据、skills 文件、Git 配置或 Keychain 令牌。" : "This restores app preferences without deleting usage data, skills files, Git config, or Keychain tokens." }
    var cancel: String { isZh ? "取消" : "Cancel" }
    var about: String { isZh ? "关于" : "About" }
    var aboutSupportedAgents: String { isZh ? "支持的 Agent" : "Supported Agents" }
    func aboutAgentCount(_ n: Int) -> String { isZh ? "\(n) 个 AI 编程工具" : "\(n) AI coding tools" }
    func aboutLimitsCount(_ n: Int) -> String { isZh ? "\(n) 支持限额" : "\(n) with limits" }
    func aboutOtherCount(_ n: Int) -> String { isZh ? "\(n) 其他" : "\(n) other" }
    var aboutWithLimits: String { isZh ? "带限额订阅" : "With Limits" }
    var aboutWithoutLimits: String { isZh ? "不带限额" : "Without Limits" }

    // MARK: - Skill Manager
    var skills: String { isZh ? "技能" : "Skills" }
    var skillsSubtitle: String { isZh ? "统一管理已选 Agent 的技能，并同步共享目录与各 Agent 目录中的链接状态。" : "Manage skills for selected agents, including shared skills and per-agent links." }
    var skillSearchPlaceholder: String { isZh ? "搜索技能…" : "Search skills…" }
    var skillFilter: String { isZh ? "筛选" : "Filter" }
    var skillFetch: String { isZh ? "刷新" : "Fetch" }
    var skillOrganize: String { isZh ? "整理" : "Organize" }
    var skillRestore: String { isZh ? "还原" : "Restore" }
    var skillDelete: String { isZh ? "删除" : "Delete" }
    var skillInstall: String { isZh ? "安装" : "Install" }
    var skillAgents: String { isZh ? "代理" : "Agents" }
    var skillColumnSkill: String { isZh ? "技能" : "Skill" }
    var skillColumnActions: String { isZh ? "操作" : "Actions" }
    var skillColumnAgents: String { isZh ? "Agent" : "Agents" }
    var skillSync: String { isZh ? "同步" : "Sync" }
    var skillSettings: String { isZh ? "设置" : "Settings" }
    var skillAll: String { isZh ? "全部" : "All" }
    var skillGlobal: String { isZh ? "全局" : "Global" }
    var skillOperationFailed: String { isZh ? "技能操作失败" : "Skill operation failed" }
    var skillNoBatchTargets: String { isZh ? "没有可处理的技能" : "No eligible skills" }
    var skillDeleteConfirm: String { isZh ? "确认删除此技能？此操作不可撤销。" : "Are you sure you want to delete this skill? This cannot be undone." }
    var skillNoSkills: String { isZh ? "暂无技能" : "No skills" }
    var skillNoSkillsDesc: String { isZh ? "在技能根目录或已选 Agent 的 skills 目录下添加包含 SKILL.md 的目录即可显示。" : "Add directories with SKILL.md under the skills source root or any selected agent skills directory." }
    var skillPreviewTip: String { isZh ? "预览此技能的 SKILL.md" : "Preview this skill's SKILL.md" }
    var skillFilesEmpty: String { isZh ? "无法读取此技能的目录结构。" : "Unable to read this skill's file tree." }
    var skillPreviewMissingFile: String { isZh ? "未找到 SKILL.md 文件。" : "SKILL.md was not found." }
    var skillPreviewNotText: String { isZh ? "该文件不是可预览的 UTF-8 文本。" : "This file is not previewable UTF-8 text." }
    func skillPreviewReadFailed(_ reason: String) -> String { isZh ? "无法读取文件：\(reason)" : "Unable to read file: \(reason)" }
    func skillPreviewTruncated(_ byteCount: Int) -> String {
        let size = ByteCountFormatter.string(fromByteCount: Int64(byteCount), countStyle: .file)
        return isZh ? "文件过大，仅显示前 \(size)。" : "Large file: showing the first \(size)."
    }
    var skillInstallTitle: String { isZh ? "安装 Skill" : "Install Skill" }
    var skillInstallDesc: String { isZh ? "从本地目录、ZIP 包、Git 仓库或 GitHub 目录链接安装单个包含 SKILL.md 的技能到全局技能目录。" : "Install one skill containing SKILL.md from a local folder, ZIP archive, Git repository, or GitHub directory URL into the global skills directory." }
    var skillInstallTip: String { isZh ? "安装新的技能到全局技能目录" : "Install a new skill into the global skills directory" }
    var skillInstallFolder: String { isZh ? "文件夹" : "Folder" }
    var skillInstallZip: String { isZh ? "ZIP 包" : "ZIP" }
    var skillInstallGit: String { isZh ? "Git 仓库" : "Git" }
    var skillInstallFolderPlaceholder: String { isZh ? "选择包含 SKILL.md 的目录" : "Choose a directory containing SKILL.md" }
    var skillInstallZipPlaceholder: String { isZh ? "选择包含单个 Skill 的 ZIP 包" : "Choose a ZIP containing one skill" }
    var skillInstallGitPlaceholder: String { isZh ? "支持仓库地址或 GitHub 目录链接" : "Repo URL or GitHub folder URL" }
    var skillInstallChooseFolder: String { isZh ? "选择文件夹" : "Choose Folder" }
    var skillInstallChooseZip: String { isZh ? "选择 ZIP" : "Choose ZIP" }
    var skillInstallReplaceExisting: String { isZh ? "如果同名 Skill 已存在，则覆盖" : "Replace existing skill with the same name" }
    var skillInstallSelectSkills: String { isZh ? "选择要安装的 Skills" : "Select skills to install" }
    var skillInstallSelectAll: String { isZh ? "全选" : "All" }
    var skillInstallSelectNone: String { isZh ? "全不选" : "None" }
    var skillInstallSelected: String { isZh ? "安装选中项" : "Install Selected" }
    func skillInstallSuccess(_ skill: String) -> String { isZh ? "已安装 \(skill)" : "Installed \(skill)" }
    func skillInstallSuccessList(_ skills: [String]) -> String {
        let label = skills.isEmpty ? "" : skills.joined(separator: ", ")
        return isZh ? "已安装 \(label)" : "Installed \(label)"
    }
    func skillInstallFailed(_ reason: String) -> String { isZh ? "安装失败：\(reason)" : "Install failed: \(reason)" }
    var skillBuiltIn: String { isZh ? "内置" : "Built-in" }
    var skillBuiltInTip: String { isZh ? "代理自带的内置技能，默认仅兼容来源代理" : "Bundled with the agent; marked compatible with the source agent by default" }
    var skillBuiltInOrganizeTitle: String { isZh ? "整理内置技能？" : "Organize Built-in Skill?" }
    func skillBuiltInOrganizeWarning(_ skill: String, _ agent: String) -> String { isZh ? "「\(skill)」是 \(agent) 的内置技能。整理后会移动到全局技能目录并保留链接，可能影响此 Agent 的内置技能更新。仍要整理吗？" : "\"\(skill)\" is built into \(agent). Organizing it will move it into the global skills directory and keep a link, which may affect future built-in skill updates. Continue?" }
    var skillBuiltInOrganizeConfirm: String { isZh ? "仍然整理" : "Organize Anyway" }
    func skillCompatWarning(_ skill: String, _ agent: String) -> String { isZh ? "「\(skill)」标记为仅兼容部分代理。仍要为 \(agent) 创建链接吗？" : "This skill is marked compatible with specific agents only. Link it to \(agent) anyway?" }
    var skillCompatConfirm: String { isZh ? "仍然链接" : "Link Anyway" }
    var skillShowBuiltIn: String { isZh ? "显示内置技能" : "Show built-in skills" }
    var agentName: String { isZh ? "名称" : "Name" }
    var agentSkillsPath: String { isZh ? "技能路径" : "Skills Path" }
    var linkStrategy: String { isZh ? "链接策略" : "Link Strategy" }
    var addAgent: String { isZh ? "添加代理" : "Add Agent" }
    var refresh: String { isZh ? "刷新" : "Refresh" }
    var pull: String { isZh ? "拉取" : "Pull" }
    var push: String { isZh ? "推送" : "Push" }
    var skillsSourceRoot: String { isZh ? "技能根目录" : "Skills Source Root" }
    var save: String { isZh ? "保存" : "Save" }
    var toastSaved: String { isZh ? "保存成功" : "Saved" }
    var toastSaveFailed: String { isZh ? "保存失败" : "Save failed" }
    var toastRefreshed: String { isZh ? "刷新成功" : "Refreshed" }
    var toastSynced: String { isZh ? "同步成功" : "Synced" }
    var toastPulled: String { isZh ? "拉取成功" : "Pulled" }
    var toastPushed: String { isZh ? "推送成功" : "Pushed" }
    var toastDeleted: String { isZh ? "删除成功" : "Deleted" }
    var toastOrganized: String { isZh ? "整理成功" : "Organized" }
    var toastRestored: String { isZh ? "还原成功" : "Restored" }
    var toastLinked: String { isZh ? "链接成功" : "Linked" }
    var toastUnlinked: String { isZh ? "取消链接成功" : "Unlinked" }
    var toastReset: String { isZh ? "重置成功" : "Reset" }
    var toastSettingsReset: String { isZh ? "设置已重置" : "Settings reset" }

    // MARK: - Skill Git Sync Sheet
    var gitSync: String { isZh ? "Git 同步" : "Git Sync" }
    var gitStatus: String { isZh ? "状态" : "Status" }
    var gitBranch: String { isZh ? "分支" : "Branch" }
    var gitAhead: String { isZh ? "领先" : "Ahead" }
    var gitBehind: String { isZh ? "落后" : "Behind" }
    var gitPlatform: String { isZh ? "平台" : "Platform" }
    var gitCustomGit: String { isZh ? "自定义 Git" : "Custom Git" }
    var gitAuthentication: String { isZh ? "认证" : "Authentication" }
    var gitToken: String { isZh ? "令牌" : "Token" }
    var gitPendingChanges: String { isZh ? "待处理变更" : "Pending Changes" }
    var gitPendingFilteredChanges: String { isZh ? "将推送的变更" : "Changes To Push" }
    var gitPendingFilteredChangesDesc: String { isZh ? "当前仅显示匹配同步过滤规则的变更。" : "Showing only changes matching the sync filter." }
    var gitNoPendingChanges: String { isZh ? "没有待处理变更" : "No pending changes"}
    var gitNoBranch: String { isZh ? "(无分支)" : "(no branch)" }
    var gitRemoteFormat: String { isZh ? "远程: %@" : "Remote: %@" }
    var gitSaveConfig: String { isZh ? "保存配置" : "Save Config" }
    var gitDone: String { isZh ? "完成" : "Done" }
    var gitCancel: String { isZh ? "取消" : "Cancel" }
    var skillCompatTitle: String { isZh ? "跨代理链接" : "Cross-Agent Link" }
    var gitTokenPlaceholder: String { "Personal access token" }
    var gitRepository: String { isZh ? "仓库" : "Repository" }
    func gitRepositoryDesc(_ provider: String) -> String { isZh ? "输入用于同步 skills 的 \(provider) 仓库地址。" : "Enter the \(provider) repository URL used to sync skills." }
    var gitAuthorize: String { isZh ? "授权" : "Authorize" }
    var gitAuthorizeTip: String { isZh ? "配置 Git 平台和访问令牌" : "Configure git provider and authorization" }
    var gitConfigRequired: String { isZh ? "请先配置仓库地址和访问令牌" : "Configure repository URL and token first" }
    var gitChecking: String { isZh ? "检查中…" : "Checking…" }
    var gitConnected: String { isZh ? "已连接" : "Connected" }
    var gitDisconnected: String { isZh ? "未连接" : "Disconnected" }
    var gitUpToDate: String { isZh ? "已是最新" : "Up to Date" }
    var gitChangesPending: String { isZh ? "有待提交变更" : "Changes Pending" }
    var gitFilteredChangesPending: String { isZh ? "有匹配过滤规则的变更" : "Filtered Changes Pending" }
    var gitUpToDateFiltered: String { isZh ? "过滤范围内已是最新" : "Filtered Scope Up to Date" }
    var gitNoFilteredChanges: String { isZh ? "当前过滤规则下没有待推送变更。" : "No changes match the current sync filter." }
    func gitFilteredChangesCount(_ count: Int) -> String { isZh ? "当前过滤规则下 \(count) 个文件将被推送。" : "\(count) file(s) match the current sync filter." }
    var gitConflicts: String { isZh ? "存在冲突" : "Merge Conflicts" }
    var gitPushing: String { isZh ? "推送中…" : "Pushing…" }
    var gitPulling: String { isZh ? "拉取中…" : "Pulling…" }
    var gitError: String { isZh ? "错误" : "Error" }
    var gitNotConfigured: String { isZh ? "未配置" : "Not Configured" }
    var gitAuthorization: String { isZh ? "授权" : "Authorization" }
    var gitProvider: String { isZh ? "Git 平台" : "Git Provider" }
    var gitSyncBranch: String { isZh ? "同步分支" : "Sync Branch" }
    var gitSyncBranchDesc: String { isZh ? "Pull 和 Push 都使用此远程分支。" : "Pull and Push both use this remote branch." }
    var gitTokenStoredLocally: String { isZh ? "令牌仅保存在本机，并只用于 git pull/push。" : "Token is stored locally and used only for git push/pull." }
    var gitTokenScopes: String { isZh ? "需要权限: repo 读写" : "Required scopes: repo read/write" }
    var gitTokenSaved: String { isZh ? "令牌已保存" : "Token saved" }
    var gitRemoveToken: String { isZh ? "移除令牌" : "Remove Token" }
    var gitUpdateToken: String { isZh ? "更新" : "Update" }
    func gitTokenHelpTitle(_ provider: String) -> String { isZh ? "如何创建 \(provider) Token" : "How to create a \(provider) token" }
    func gitTokenHelpStep1(_ provider: String) -> String { isZh ? "登录 \(provider) 并打开 Access Tokens 设置" : "Open Access Tokens settings in \(provider)" }
    var gitTokenHelpStep2: String { isZh ? "创建新的个人访问令牌" : "Create a new personal access token" }
    var gitTokenHelpStep3: String { isZh ? "选择仓库读写权限" : "Select repository read/write scopes" }
    var gitTokenHelpStep4: String { isZh ? "复制令牌并粘贴到这里" : "Copy the token and paste it here" }
    var gitCommitIdentity: String { isZh ? "提交身份" : "Commit Identity" }
    var gitUserName: String { isZh ? "用户名" : "User name" }
    var gitUserEmail: String { isZh ? "邮箱" : "Email" }
    func gitDefaultIdentity(_ identity: String) -> String { isZh ? "默认: \(identity)" : "Default: \(identity)" }
    var gitDefaultIdentityMissing: String { isZh ? "未检测到默认 Git 用户名和邮箱" : "No default git user name and email detected" }
    var gitCommitIdentityDesc: String { isZh ? "留空时使用当前 git 默认配置；填写后 sync 提交会使用这里的用户名和邮箱。" : "Leave blank to use the current git default. Sync commits use these values when provided." }
    var skillSyncFilter: String { isZh ? "同步过滤" : "Sync Filter" }
    var skillSyncFilterDesc: String { isZh ? "仅在 Push 时提交匹配的 Skill；Pull 仍会拉取整个仓库以保持 Git 历史一致。" : "Only matching skills are committed on Push. Pull still fetches the full repository to keep Git history consistent." }
    var skillSyncFilterPrefixes: String { isZh ? "包含前缀" : "Included prefixes" }
    var skillSyncFilterPrefixesPlaceholder: String { isZh ? "例如 webkong，每行或逗号分隔" : "For example: webkong. Separate with commas or new lines." }
    var skillSyncFilterPrefixesHelp: String { isZh ? "前缀匹配 Skill 目录名；webkong 会匹配 webkong-article、webkong-search 等。" : "Prefixes match skill directory names. webkong matches webkong-article, webkong-search, and similar IDs." }
    var skillSyncFilterSelectedSkills: String { isZh ? "额外选择的 Skill" : "Additional selected skills" }
    func skillSyncFilterSelectedCount(_ count: Int) -> String { isZh ? "已选 \(count) 个" : "\(count) selected" }
    var skillSyncFilterSelectPrefixMatches: String { isZh ? "选择前缀匹配" : "Select prefix matches" }
    var skillSyncFilterPrefixMatched: String { isZh ? "前缀匹配" : "prefix" }

    // MARK: - Skill Manager additional
    var skillGlobalBadge: String { isZh ? "全局" : "Global" }
    var skillNoAgentsEnabled: String { isZh ? "未启用任何 Agent" : "No agents enabled" }
    var skillRefreshTip: String { isZh ? "重新扫描技能根目录和已启用 Agent 的 skills 目录" : "Rescan the skills source root and enabled agent skills folders" }
    var skillGitSyncTip: String { isZh ? "打开 Git 同步面板，配置远程仓库并同步技能目录" : "Open Git Sync to configure a remote repository and sync skills" }
    var skillOrganizeAllTip: String { isZh ? "把当前筛选结果中尚未整理的技能移动到共享技能根目录" : "Organize all eligible skills in the current filtered list into the shared source root" }
    var skillRestoreAllTip: String { isZh ? "把当前筛选结果中已整理的技能还原到原始 Agent 目录" : "Restore all organized skills in the current filtered list to their original agent folders" }
    var skillOrganizeAllConfirmTitle: String { isZh ? "确认批量整理？" : "Confirm Batch Organize?" }
    var skillOrganizeAllConfirmMessage: String { isZh ? "将把当前筛选结果中可整理的技能移动到共享技能根目录，并保留对应 Agent 的链接。" : "Eligible skills in the current filtered list will be moved into the shared source root while keeping their agent links." }
    var skillRestoreAllConfirmTitle: String { isZh ? "确认批量还原？" : "Confirm Batch Restore?" }
    var skillRestoreAllConfirmMessage: String { isZh ? "将把当前筛选结果中可还原的技能放回原始 Agent 目录。" : "Eligible skills in the current filtered list will be restored to their original agent folders." }
    var skillAllFilterTip: String { isZh ? "显示所有已启用 Agent 可见的技能" : "Show skills visible to all enabled agents" }
    var skillGlobalFilterTip: String { isZh ? "只显示全局技能目录中的技能" : "Show only skills in the global skills directory" }
    func skillAgentFilterTip(_ agent: String) -> String { isZh ? "只显示 \(agent) 相关的技能" : "Show skills related to \(agent)" }
    func skillOrganizeTip(_ agent: String) -> String { isZh ? "把此技能整理到共享技能根目录，并为 \(agent) 保持链接" : "Move this skill into the shared source root and keep \(agent) linked" }
    func skillRestoreTip(_ agent: String) -> String { isZh ? "把此技能从共享技能根目录还原到 \(agent) 的 skills 目录" : "Restore this skill from the shared source root into \(agent)'s skills folder" }
    var skillDeleteTip: String { isZh ? "删除此技能目录。此操作不可撤销" : "Delete this skill directory. This cannot be undone" }
    func skillLinkTip(_ agent: String) -> String { isZh ? "为 \(agent) 创建此技能的符号链接" : "Create a symlink for \(agent)" }
    func skillUnlinkTip(_ agent: String) -> String { isZh ? "移除 \(agent) 的此技能符号链接" : "Remove the symlink for \(agent)" }
    func skillSourceLinkTip(_ agent: String) -> String { isZh ? "此技能来源于 \(agent)，点击可为它创建共享目录链接" : "This skill originated in \(agent); click to create a shared-root link" }
    var gitDoneTip: String { isZh ? "关闭 Git 同步面板" : "Close the Git Sync panel" }
    var gitPullTip: String { isZh ? "从远程仓库拉取最新技能变更" : "Pull the latest skill changes from the remote repository" }
    var gitPushTip: String { isZh ? "把本地技能变更推送到远程仓库" : "Push local skill changes to the remote repository" }
    var gitRefreshStatusTip: String { isZh ? "刷新当前 Git 分支和待处理变更状态" : "Refresh the current branch and pending Git changes" }
    var gitSaveConfigTip: String { isZh ? "保存 Git 远程地址、平台和访问令牌" : "Save the Git remote URL, platform, and access token" }
    var skillAgentParticipation: String { isZh ? "参与 Skills 管理的 Agent" : "Agents participating in Skills" }
    var skillAgentParticipationDesc: String { isZh ? "启用的 Agent 将出现在 Skills 页面的筛选器中" : "Enabled agents appear as filters on the Skills page" }
    func skillNotInstalled(_ agent: String) -> String { isZh ? "\(agent) 未检测到安装" : "\(agent) not installed" }
    var skillsSourceRootPlaceholder: String { isZh ? "~/.agents/skills" : "~/.agents/skills" }
    var loading: String { isZh ? "加载中…" : "Loading…" }
    var menuBarSectionTitle: String { isZh ? "菜单栏" : "Menu Bar" }

    // MARK: - Usage section headers
    var usageDaily: String { isZh ? "每日用量" : "Daily Usage" }
    var usageModels: String { isZh ? "模型" : "Models" }
    var usageTokenBreakdown: String { isZh ? "Token 分解" : "Token Breakdown" }
    var usageProviders: String { isZh ? "数据源" : "Providers" }
    var usageActivity: String { isZh ? "活跃度" : "Activity" }
    var usageDailyDetails: String { isZh ? "每日明细" : "Daily Details" }
    func usageActiveDays(_ n: Int) -> String { isZh ? "\(n) 天活跃" : "\(n) active days" }

    // MARK: - Sync frequency
    var sync5min: String { "5 min" }
    var sync10min: String { "10 min" }
    var sync15min: String { "15 min" }
    var sync30min: String { "30 min" }
    var sync1hour: String { "1 hour" }

    // MARK: - Provider settings
    var pathLabel: String { "Path" }
    var linkLabel: String { "Link" }
    var linkDirectory: String { "Directory" }
    var linkSingleFile: String { "Single File" }
    var linkOverlay: String { "Overlay" }

    // MARK: - App
    var appName: String { "Token Viewer" }

    private func dayCount(_ days: Int) -> String {
        days == 1 ? "1 day" : "\(days) days"
    }

    func countdownText(until date: Date) -> String {
        let seconds = date.timeIntervalSince(Date())
        if seconds <= 0 { return isZh ? "现在" : "now" }
        if seconds < 3_600 {
            let minutes = max(1, Int(ceil(seconds / 60)))
            return isZh ? "\(minutes) 分钟" : (minutes == 1 ? "1 minute" : "\(minutes) minutes")
        }
        if seconds < 172_800 {
            let hours = max(1, Int(ceil(seconds / 3_600)))
            return isZh ? "\(hours) 小时" : (hours == 1 ? "1 hour" : "\(hours) hours")
        }
        return dayCount(max(1, Int(ceil(seconds / 86_400))))
    }
}
