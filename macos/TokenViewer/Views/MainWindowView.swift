import SwiftUI

struct MainWindowView: View {
    @ObservedObject private var viewModel = UsageViewModel.shared

    var body: some View {
        TabView {
            UsageView(viewModel: viewModel)
                .tabItem { Label("Usage", systemImage: "chart.bar.fill") }

            LimitsView(viewModel: LimitsViewModel.shared)
                .tabItem { Label("Limits", systemImage: "gauge.with.dots.needle.50percent") }

            SettingsView()
                .tabItem { Label("Settings", systemImage: "gear") }
        }
        .frame(minWidth: 600, minHeight: 480)
    }
}
