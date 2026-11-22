import Foundation

/// The alerts the Notification Service Extension showed lately, by the
/// event id a computer seals into each (`SealedAlert.event`), so a copy
/// that arrives again shows once: a push APNs or the push relay delivered
/// twice, a retry after a failover, or one sent again another way. Kept in
/// the App Group's defaults, newest last, at most `kept` ids, none older
/// than `window`, so it never grows.
public struct SeenAlerts: Equatable, Sendable {
    public static let kept = 128
    /// Longer than any push waits for a phone: a question waits a day.
    public static let window: TimeInterval = 2 * 24 * 60 * 60
    static let key = "seenAlerts"

    struct Entry: Equatable, Sendable {
        let id: String
        let at: TimeInterval
    }

    private(set) var entries: [Entry] = []

    public init() {}

    /// Notes `id` as shown at `date`; whether it is the first time.
    public mutating func see(_ id: String, at date: Date) -> Bool {
        let now = date.timeIntervalSince1970
        entries.removeAll { now - $0.at > Self.window }
        if entries.contains(where: { $0.id == id }) { return false }
        entries.append(Entry(id: id, at: now))
        if entries.count > Self.kept { entries.removeFirst(entries.count - Self.kept) }
        return true
    }

    /// Notes `id` as shown at `date` in `defaults`; whether it is the first
    /// time. Without defaults nothing is remembered, and every alert shows.
    public static func firstSighting(_ id: String, at date: Date, in defaults: UserDefaults?) -> Bool {
        guard let defaults else { return true }
        var seen = load(from: defaults)
        let first = seen.see(id, at: date)
        defaults.set(seen.entries.flatMap { [$0.id, String($0.at)] }, forKey: key)
        return first
    }

    static func load(from defaults: UserDefaults) -> SeenAlerts {
        var seen = SeenAlerts()
        let flat = defaults.stringArray(forKey: key) ?? []
        for index in stride(from: 0, to: flat.count - 1, by: 2) {
            guard let at = TimeInterval(flat[index + 1]) else { continue }
            seen.entries.append(Entry(id: flat[index], at: at))
        }
        seen.entries = Array(seen.entries.suffix(kept))
        return seen
    }
}
