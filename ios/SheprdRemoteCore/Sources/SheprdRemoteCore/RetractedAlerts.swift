import Foundation

/// Copies of alerts to take back. A copy of an alert the person already
/// dismissed or opened can only be shown again, silently: without Apple's
/// filtering entitlement the Notification Service Extension must deliver
/// something. It notes the copy here, and the extension's next run and the
/// app (launch, activation, background push) remove it. A copy is removed
/// only while the notification under its identifier still carries its
/// event: a newer alert for the same chat reuses the identifier (the APNs
/// collapse id) and stays. At most `kept`, none older than `window`.
///
/// The app and its extension are separate processes that both change the
/// list, so it lives in a file in the App Group and every change is one
/// coordinated read, change, and write (`update`), never two.
public struct RetractedAlerts: Codable, Equatable, Sendable {
    public static let kept = 32
    public static let window: TimeInterval = 2 * 24 * 60 * 60
    /// The file's name in the App Group container.
    public static let fileName = "retracted-alerts.json"

    struct Entry: Codable, Equatable, Sendable {
        let identifier: String
        let event: String
        let at: TimeInterval
        /// Whether the extension handed the copy to iOS. Until then,
        /// another event under its identifier proves nothing: the copy may
        /// still arrive on top of it.
        var delivered = false
    }

    private(set) var entries: [Entry] = []

    public init() {}

    public var isEmpty: Bool { entries.isEmpty }

    /// Notes the copy shown as `identifier` for `event` at `date`.
    public mutating func add(_ identifier: String, event: String, at date: Date) {
        entries.removeAll { $0.identifier == identifier }
        entries.append(Entry(identifier: identifier, event: event, at: date.timeIntervalSince1970))
        if entries.count > Self.kept { entries.removeFirst(entries.count - Self.kept) }
    }

    /// Whether a copy of `event` waits to be taken back: a copy of it now
    /// on screen is then itself a late copy, and so is the next.
    public func pending(_ event: String) -> Bool {
        entries.contains { $0.event == event }
    }

    /// The copy noted under `identifier` for `event` was handed to iOS.
    public mutating func delivered(_ identifier: String, event: String) {
        for index in entries.indices where entries[index].identifier == identifier && entries[index].event == event {
            entries[index].delivered = true
        }
    }

    /// Forgets the delivered copy noted under `identifier`: another alert
    /// is about to be shown there, replacing it, which no sweep may take
    /// back. A copy not delivered yet may still land on top of that alert,
    /// so it stays noted.
    public mutating func forget(_ identifier: String) {
        entries.removeAll { $0.identifier == identifier && $0.delivered }
    }

    /// The identifiers to remove now, given what is shown (`identifier` to
    /// its event, if it has one) at `date`. A copy shown is removed. A copy
    /// not shown yet waits for a later sweep. A delivered copy whose
    /// identifier shows another event was replaced by a newer alert and is
    /// forgotten; one not delivered yet waits, as it may still land on top.
    /// Any past `window` is forgotten.
    public mutating func take(shown: [String: String?], at date: Date) -> [String] {
        let now = date.timeIntervalSince1970
        var remove: [String] = []
        entries.removeAll { entry in
            if now - entry.at > Self.window { return true }
            guard let event = shown[entry.identifier] else { return false }
            if event == entry.event {
                remove.append(entry.identifier)
                return true
            }
            return entry.delivered
        }
        return remove
    }

    /// Reads the list in `file`, lets `change` change it, and writes it
    /// back, all while holding the file against every other coordinated
    /// reader and writer, in this process or the other. A file that is
    /// missing or unreadable reads as empty. Blocking: `change` may wait,
    /// briefly, for what iOS shows.
    @discardableResult
    public static func update<Result>(file: URL, _ change: (inout RetractedAlerts) -> Result) -> Result {
        var outcome: Result?
        var failure: NSError?
        NSFileCoordinator().coordinate(writingItemAt: file, options: .forMerging, error: &failure) { url in
            var list = (try? JSONDecoder().decode(Self.self, from: Data(contentsOf: url))) ?? Self()
            let before = list
            outcome = change(&list)
            if list != before, let data = try? JSONEncoder().encode(list) {
                try? data.write(to: url, options: .atomic)
            }
        }
        // Coordination fails only for a file it cannot reach at all; the
        // change then runs on an empty list that is not kept.
        if let outcome { return outcome }
        var list = Self()
        return change(&list)
    }
}
