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
    var summary: String { isZh ? "摘要" : "Summary" }
    var models: String { isZh ? "模型" : "Models" }
    var heatmap: String { isZh ? "热力图" : "Heatmap" }
    var general: String { isZh ? "通用" : "General" }
    var launchAtLogin: String { isZh ? "开机启动" : "Launch at Login" }
    var syncFrequency: String { isZh ? "同步频率" : "Sync Frequency" }
    var manual: String { isZh ? "手动" : "Manual" }
    var updates: String { isZh ? "更新" : "Updates" }
    var upToDate: String { isZh ? "已是最新版本" : "Up to date" }
    var newVersion: String { isZh ? "有新版本" : "New version available" }
    var download: String { isZh ? "下载" : "Download" }
    var checkNow: String { isZh ? "检查更新" : "Check Now" }
    var providers: String { isZh ? "数据源" : "Providers" }
    var noProviderData: String { isZh ? "尚无数据。使用任意 AI 工具后点击同步。" : "No provider data yet. Use any supported AI tool, then Sync." }
    func recordsCount(_ n: Int) -> String { isZh ? "\(n) 条记录" : "\(n) records" }
    func activeCount(_ n: Int) -> String { isZh ? "22 个支持工具中 \(n) 个活跃" : "\(n) of 22 supported tools active" }
    var data: String { isZh ? "数据" : "Data" }
    var resetData: String { isZh ? "重置数据" : "Reset Data" }
    var resetDataDesc: String { isZh ? "删除所有本地 Token 数据，此操作不可撤销。" : "Deletes all local token data. This cannot be undone." }
    var resetConfirm: String { isZh ? "确认重置" : "Confirm Reset" }
    var resetDone: String { isZh ? "请重启 TokenViewer 完成重置。" : "Please relaunch TokenViewer to finish resetting." }
    var about: String { isZh ? "关于" : "About" }
}
