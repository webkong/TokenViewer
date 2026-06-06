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
    var cancel: String { isZh ? "取消" : "Cancel" }
    var about: String { isZh ? "关于" : "About" }

    private func dayCount(_ days: Int) -> String {
        days == 1 ? "1 day" : "\(days) days"
    }
}
