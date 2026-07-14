import Foundation

/// Parses provider timestamps into absolute `Date` values.
/// Provider payloads may use epoch seconds/milliseconds, ISO-8601, or UTC date-only strings.
enum ProviderDateParser {
    static func parse(_ value: Any?) -> Date? {
        if let number = numeric(value), number > 0 {
            let seconds = number > 1_000_000_000_000 ? number / 1000 : number
            return Date(timeIntervalSince1970: seconds)
        }

        guard let string = value as? String else { return nil }
        let trimmed = string.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }

        let iso = ISO8601DateFormatter()
        if let date = iso.date(from: trimmed) { return date }
        return parseUTCDate(trimmed, format: "yyyy-MM-dd")
    }

    static func nextUTCMonthDay(_ value: String, now: Date = Date()) -> Date? {
        let parts = value.split(separator: "/")
        guard parts.count == 2,
              let month = Int(parts[0]),
              let day = Int(parts[1]) else { return nil }

        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 0) ?? .autoupdatingCurrent
        var components = DateComponents()
        components.calendar = calendar
        components.timeZone = calendar.timeZone
        components.year = calendar.component(.year, from: now)
        components.month = month
        components.day = day

        guard var date = calendar.date(from: components) else { return nil }
        if date < now {
            components.year = (components.year ?? 0) + 1
            date = calendar.date(from: components) ?? date
        }
        return date
    }

    static func localString(from date: Date, format: String) -> String {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = .autoupdatingCurrent
        formatter.dateFormat = format
        return formatter.string(from: date)
    }

    private static func parseUTCDate(_ value: String, format: String) -> Date? {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = format
        return formatter.date(from: value)
    }

    private static func numeric(_ value: Any?) -> Double? {
        if let double = value as? Double { return double }
        if let int = value as? Int { return Double(int) }
        if let number = value as? NSNumber { return number.doubleValue }
        if let string = value as? String {
            return Double(string.trimmingCharacters(in: .whitespacesAndNewlines))
        }
        return nil
    }
}
