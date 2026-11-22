import Foundation

/// When test pushes arrived, written by the Notification Service Extension
/// as it opens one and read by the app that asked for it, through the App
/// Group's defaults. Holds the newest few, so it never grows.
public enum PushTestReceipts {
    static let key = "pushTestReceipts"
    /// More than one Settings screen can wait on at once.
    static let kept = 16

    /// Records that test `id` arrived at `date`.
    public static func record(_ id: String, at date: Date, in defaults: UserDefaults?) {
        guard let defaults else { return }
        var receipts = all(in: defaults)
        receipts[id] = date.timeIntervalSince1970
        if receipts.count > kept {
            let newest = receipts.sorted { $0.value > $1.value }.prefix(kept)
            receipts = Dictionary(uniqueKeysWithValues: newest.map { ($0.key, $0.value) })
        }
        defaults.set(receipts, forKey: key)
    }

    /// When test `id` arrived, if it has.
    public static func arrival(of id: String, in defaults: UserDefaults?) -> Date? {
        guard let defaults, let seconds = all(in: defaults)[id] else { return nil }
        return Date(timeIntervalSince1970: seconds)
    }

    private static func all(in defaults: UserDefaults) -> [String: Double] {
        defaults.dictionary(forKey: key) as? [String: Double] ?? [:]
    }
}
