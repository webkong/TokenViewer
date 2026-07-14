import Foundation

struct UsageQueryRange {
    let from: String
    let to: String
}

/// Shared time semantics for usage queries.
///
/// Usage is stored as UTC instants. User-facing ranges are local calendar days,
/// converted to UTC only at the CoreBridge boundary.
enum AppTime {
    static let allUsageStart = "2020-01-01T00:00:00Z"

    static var localTimeZone: TimeZone { .autoupdatingCurrent }

    static var localCalendar: Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = localTimeZone
        return calendar
    }

    static func isSameLocalDay(_ lhs: Date, _ rhs: Date) -> Bool {
        localCalendar.isDate(lhs, inSameDayAs: rhs)
    }

    static func localStartOfDay(for date: Date) -> Date {
        localCalendar.startOfDay(for: date)
    }

    static func trailingLocalDays(_ count: Int, now: Date = Date()) -> UsageQueryRange {
        precondition(count > 0)
        let calendar = localCalendar
        let today = calendar.startOfDay(for: now)
        let start = calendar.date(byAdding: .day, value: -(count - 1), to: today) ?? today
        let end = calendar.date(byAdding: .day, value: 1, to: today) ?? now
        return utcRange(from: start, to: end)
    }

    static func inclusiveLocalDays(from: Date, through: Date) -> UsageQueryRange {
        let calendar = localCalendar
        let start = calendar.startOfDay(for: min(from, through))
        let lastDay = calendar.startOfDay(for: max(from, through))
        let end = calendar.date(byAdding: .day, value: 1, to: lastDay) ?? lastDay
        return utcRange(from: start, to: end)
    }

    static func allUsage(through now: Date = Date()) -> UsageQueryRange {
        UsageQueryRange(from: allUsageStart, to: trailingLocalDays(1, now: now).to)
    }

    static func utcString(from date: Date) -> String {
        let formatter = DateFormatter()
        formatter.calendar = Calendar(identifier: .gregorian)
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = TimeZone(secondsFromGMT: 0)
        formatter.dateFormat = "yyyy-MM-dd'T'HH:mm:ss'Z'"
        return formatter.string(from: date)
    }

    static func localDayKey(for date: Date) -> String {
        localDayFormatter().string(from: date)
    }

    static func localDate(fromDayKey value: String) -> Date? {
        localDayFormatter().date(from: value)
    }

    private static func utcRange(from: Date, to: Date) -> UsageQueryRange {
        UsageQueryRange(from: utcString(from: from), to: utcString(from: to))
    }

    private static func localDayFormatter() -> DateFormatter {
        let formatter = DateFormatter()
        formatter.calendar = localCalendar
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.timeZone = localTimeZone
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter
    }
}
