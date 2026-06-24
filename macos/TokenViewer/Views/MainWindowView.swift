import SwiftUI

struct MainWindowView: View {
    @ObservedObject private var viewModel = UsageViewModel.shared
    @ObservedObject private var l10n = L10n.shared
    @AppStorage("mainWindowTab") private var mainWindowTab = "usage"

    var body: some View {
        TabView(selection: $mainWindowTab) {
            UsageView(viewModel: viewModel)
                .tag("usage")
                .tabItem { Label(l10n.usage, systemImage: "chart.bar.fill") }

            SkillManagerView()
                .tag("skills")
                .tabItem { Label(l10n.skills, systemImage: "puzzlepiece.extension.fill") }

            LimitsView(viewModel: LimitsViewModel.shared)
                .tag("limits")
                .tabItem { Label(l10n.limits, systemImage: "gauge.with.dots.needle.50percent") }

            SettingsView()
                .tag("settings")
                .tabItem { Label(l10n.settings, systemImage: "gear") }

            AboutView()
                .tag("about")
                .tabItem { Label(l10n.about, systemImage: "info.circle") }
        }
        .frame(minWidth: 600, minHeight: 480)
    }
}
