import SwiftUI

// MARK: - Theme

enum AppTheme: String, CaseIterable {
    case light, dark, system
}

@MainActor
final class ThemeManager: ObservableObject {
    static let shared = ThemeManager()

    @AppStorage("appTheme") var theme: String = AppTheme.system.rawValue {
        didSet { apply() }
    }

    func apply() {
        switch AppTheme(rawValue: theme) ?? .system {
        case .light: NSApp.appearance = NSAppearance(named: .aqua)
        case .dark: NSApp.appearance = NSAppearance(named: .darkAqua)
        case .system: NSApp.appearance = nil
        }
    }
}

// MARK: - Currency

struct CurrencyInfo { let code: String; let symbol: String }

@MainActor
final class CurrencyStore: ObservableObject {
    static let shared = CurrencyStore()

    static let supported: [CurrencyInfo] = [
        .init(code: "USD", symbol: "$"), .init(code: "CNY", symbol: "¥"),
        .init(code: "EUR", symbol: "€"), .init(code: "GBP", symbol: "£"),
        .init(code: "JPY", symbol: "¥"), .init(code: "KRW", symbol: "₩"),
    ]

    @AppStorage("currency") var currency: String = "USD" {
        didSet { Task { await fetchRate() } }
    }
    @Published var rate: Double = 1.0          // USD -> currency
    @Published var rateFetchedAt: Date?

    private init() {
        rate = UserDefaults.standard.double(forKey: "currencyRate").nonZeroOr(1.0)
        Task { await fetchRate() }
    }

    var symbol: String {
        Self.supported.first { $0.code == currency }?.symbol ?? "$"
    }

    /// Format a USD amount in the selected currency.
    func format(_ usd: Double) -> String {
        let v = currency == "USD" ? usd : usd * rate
        if v <= 0 { return "\(symbol)0.00" }
        if v < 0.01 { return "<\(symbol)0.01" }
        if v >= 1000 { return String(format: "%@%.0f", symbol, v) }
        return String(format: "%@%.2f", symbol, v)
    }

    func fetchRate() async {
        guard currency != "USD" else { rate = 1.0; return }
        guard let url = URL(string: "https://open.er-api.com/v6/latest/USD") else { return }
        var req = URLRequest(url: url); req.timeoutInterval = 10
        guard let (data, _) = try? await URLSession.shared.data(for: req),
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let rates = json["rates"] as? [String: Any],
              let r = (rates[currency] as? Double) ?? (rates[currency] as? Int).map(Double.init) else { return }
        rate = r
        rateFetchedAt = Date()
        UserDefaults.standard.set(r, forKey: "currencyRate")
    }
}

private extension Double {
    func nonZeroOr(_ fallback: Double) -> Double { self == 0 ? fallback : self }
}
