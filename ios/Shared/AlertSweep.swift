import Foundation
import SheprdRemoteCore
import UserNotifications

/// Takes back the alert copies `RetractedAlerts` holds: copies of alerts
/// the person had already dismissed or opened, which the extension could
/// only show again silently. The extension runs it as each push arrives
/// and a moment after showing one; the app at launch, on every activation
/// (any window), and on a background push.
///
/// What is shown is read, checked, and removed while the list's file is
/// held, and the extension forgets an identifier under the same hold
/// before it shows another alert there (`willShow`), so a sweep never
/// removes a newer alert that took a copy's identifier.
enum AlertSweep {
    /// How long a sweep waits for iOS to say what is shown.
    private static let shownWait: DispatchTimeInterval = .seconds(2)

    static var file: URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: NotifyKey.group)?
            .appending(path: RetractedAlerts.fileName)
    }

    static func run() async {
        await Task.detached(priority: .utility) { sweep() }.value
    }

    /// Notes the copy shown as `identifier` for `event`, to take back.
    static func retract(_ identifier: String, event: String) {
        guard let file else { return }
        RetractedAlerts.update(file: file) { $0.add(identifier, event: event, at: Date()) }
    }

    /// Notes the copy shown as `identifier` for `event` when a copy of the
    /// same event already waits to be taken back, in one hold; whether it
    /// did. A copy that replaces a late copy is late too.
    static func retractIfPending(_ identifier: String, event: String) -> Bool {
        guard let file else { return false }
        return RetractedAlerts.update(file: file) { retracted in
            guard retracted.pending(event) else { return false }
            retracted.add(identifier, event: event, at: Date())
            return true
        }
    }

    /// The late copy noted as `identifier` for `event` was handed to iOS.
    static func delivered(_ identifier: String, event: String) {
        guard let file else { return }
        RetractedAlerts.update(file: file) { $0.delivered(identifier, event: event) }
    }

    /// Another alert is about to show as `identifier`: no noted copy there
    /// may be taken back now. Blocks while a sweep holds the list.
    static func willShow(_ identifier: String) {
        guard let file else { return }
        RetractedAlerts.update(file: file) { $0.forget(identifier) }
    }

    /// Blocking: waits, briefly, for what iOS shows.
    private static func sweep() {
        guard let file else { return }
        RetractedAlerts.update(file: file) { retracted in
            guard !retracted.isEmpty, let shown = shownEvents() else { return }
            let remove = retracted.take(shown: shown, at: Date())
            if !remove.isEmpty {
                UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: remove)
            }
        }
    }

    /// Each shown notification's identifier and event, or `nil` when iOS
    /// does not answer in time.
    private static func shownEvents() -> [String: String?]? {
        let answered = DispatchSemaphore(value: 0)
        let events = ShownEvents()
        UNUserNotificationCenter.current().getDeliveredNotifications { shown in
            for note in shown {
                events.set(note.request.identifier, note.request.content.userInfo[AlertKeys.event] as? String)
            }
            answered.signal()
        }
        guard answered.wait(timeout: .now() + shownWait) == .success else { return nil }
        return events.all
    }

    /// What iOS's callback fills in on its own queue.
    private final class ShownEvents: @unchecked Sendable {
        private let lock = NSLock()
        private var events: [String: String?] = [:]

        func set(_ identifier: String, _ event: String?) {
            lock.withLock { events[identifier] = event }
        }

        var all: [String: String?] { lock.withLock { events } }
    }
}
