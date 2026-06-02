import SwiftUI

struct MainWindowView: View {
    @ObservedObject private var viewModel = UsageViewModel.shared
    @ObservedObject private var l10n = L10n.shared

    var body: some View {
        TabView {
            UsageView(viewModel: viewModel)
                .tabItem { Label(l10n.usage, systemImage: "chart.bar.fill") }

            LimitsView(viewModel: LimitsViewModel.shared)
                .tabItem { Label(l10n.limits, systemImage: "gauge.with.dots.needle.50percent") }

            SettingsView()
                .tabItem { Label(l10n.settings, systemImage: "gear") }
        }
        .frame(minWidth: 600, minHeight: 480)
    }
}
