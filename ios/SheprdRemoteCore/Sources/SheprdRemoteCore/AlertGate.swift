import Foundation

/// Orders copies of one alert the Notification Service Extension handles
/// at once. The first copy of an event is shown; a copy that arrives while
/// the first is still being prepared (its category registered) waits until
/// the first was handed to iOS, so it finds the first to replace, or is
/// dropped where the app may filter notifications. Whether an event was
/// seen before comes from `firstSighting` (`SeenAlerts` in the App Group).
public actor AlertGate {
    public enum Admission: Equatable, Sendable {
        /// Show it: no copy of this event was seen.
        case first
        /// A copy of an event already shown, or handed to iOS by now.
        case repeated
    }

    private let firstSighting: @Sendable (String) -> Bool
    /// Events whose first copy is still being prepared, with the copies
    /// waiting for it.
    private var preparing: [String: [CheckedContinuation<Void, Never>]] = [:]

    public init(firstSighting: @escaping @Sendable (String) -> Bool) {
        self.firstSighting = firstSighting
    }

    /// Admits a copy of `event`. A first copy must be followed by
    /// `delivered(event)` once it was handed to iOS, whatever happened.
    public func admit(_ event: String) async -> Admission {
        if firstSighting(event) {
            preparing[event] = []
            return .first
        }
        if preparing[event] != nil {
            await withCheckedContinuation { waiting in
                preparing[event, default: []].append(waiting)
            }
        }
        return .repeated
    }

    /// The first copy of `event` was handed to iOS: copies waiting on it
    /// go on.
    public func delivered(_ event: String) {
        preparing.removeValue(forKey: event)?.forEach { $0.resume() }
    }

    /// Events whose first copy is still being prepared, for tests.
    var pending: Int { preparing.count }

    /// Copies of `event` waiting on its first, for tests.
    func waitingCount(_ event: String) -> Int { preparing[event]?.count ?? 0 }
}
