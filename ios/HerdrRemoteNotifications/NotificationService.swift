import Foundation
import HerdrRemoteCore
import UserNotifications

/// Opens a sealed alert (`SealedAlert`) with the key the app registered,
/// so APNs only ever carried "Herdr" and ciphertext, then shows the chat's
/// title and text, groups it with the chat's other notifications, and
/// gives it the actions its question or finished turn offers.
final class NotificationService: UNNotificationServiceExtension {
    override func didReceive(
        _ request: UNNotificationRequest,
        withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void
    ) {
        let content = (request.content.mutableCopy() as? UNMutableNotificationContent) ?? UNMutableNotificationContent()
        guard let sealed = request.content.userInfo["sealed"] as? String,
              let key = try? NotifyKey.load(),
              let alert = try? key.open(sealed)
        else {
            // Shown as sent: "Herdr", "New activity"; opening the app
            // shows what it was.
            contentHandler(content)
            return
        }
        content.title = alert.title
        content.body = alert.body
        content.threadIdentifier = alert.pane
        if let badge = alert.badge { content.badge = NSNumber(value: badge) }
        var info = content.userInfo
        info["sealed"] = nil
        info[AlertKeys.computer] = alert.computer
        info[AlertKeys.pane] = alert.pane
        if let permission = alert.permission { info[AlertKeys.permission] = NSNumber(value: permission.id.value) }
        content.userInfo = info
        let presentation = alert.presentation
        guard let category = presentation.category else {
            contentHandler(content)
            return
        }
        content.categoryIdentifier = category
        let deliver = UncheckedSendable(value: contentHandler)
        let finished = UncheckedSendable(value: content)
        Task {
            await NotificationCategories.register(presentation)
            deliver.value(finished.value)
        }
    }
}

/// Lets a value iOS hands the extension cross into the task that finishes
/// it; the handler is called once, after the category is registered.
private struct UncheckedSendable<Value>: @unchecked Sendable {
    let value: Value
}
