import HerdrRemoteCore
import os
import UIKit
import UserNotifications

private let log = Logger(subsystem: "so.pen.herdr-remote", category: "alerts")

/// Owns the app model, so notification actions and pushes that launch the
/// app in the background reach the same state the screens use, and
/// handles the app's APNs token, sealed alerts' actions, and clears.
@MainActor
final class AppDelegate: NSObject, UIApplicationDelegate, @preconcurrency UNUserNotificationCenterDelegate {
    let model = AppModel()

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        Task { await NotificationCategories.register(.finished) }
        #if DEBUG
        if followDebugArguments() { return true }
        #endif
        if !model.computers.isEmpty { Task { await Self.enableAlerts() } }
        return true
    }

    /// Asks once to show alerts, then registers for the APNs token the
    /// paired computers seal alerts to.
    static func enableAlerts() async {
        let center = UNUserNotificationCenter.current()
        let granted = (try? await center.requestAuthorization(options: [.alert, .badge, .sound])) ?? false
        if granted { UIApplication.shared.registerForRemoteNotifications() }
    }

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        model.tokens.setAlert(deviceToken.hexString)
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        log.error("No APNs token: \(error.localizedDescription, privacy: .public)")
    }

    /// A background push: a question was answered elsewhere, so the chat's
    /// notifications go and the badge follows.
    func application(
        _ application: UIApplication,
        didReceiveRemoteNotification userInfo: [AnyHashable: Any]
    ) async -> UIBackgroundFetchResult {
        guard let sealed = userInfo["sealed"] as? String,
              let alert = try? NotifyKey.load()?.open(sealed)
        else { return .noData }
        let center = UNUserNotificationCenter.current()
        if alert.clear {
            let shown = await center.deliveredNotifications()
            let gone = shown.filter { $0.request.content.threadIdentifier == alert.pane }.map(\.request.identifier)
            center.removeDeliveredNotifications(withIdentifiers: gone)
        }
        if let badge = alert.badge { try? await center.setBadgeCount(badge) }
        return .newData
    }

    // MARK: UNUserNotificationCenterDelegate

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        [.banner, .list, .sound, .badge]
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        let info = response.notification.request.content.userInfo
        guard let computer = info[AlertKeys.computer] as? String, let pane = info[AlertKeys.pane] as? String else { return }
        if response.actionIdentifier == UNNotificationDefaultActionIdentifier {
            let name = model.computers.first { $0.fingerprint == Fingerprint(computer) }?.name ?? ""
            model.open(ChatLink(computer: name, computerId: computer, pane: pane).url)
            return
        }
        let permission = (info[AlertKeys.permission] as? NSNumber).map { PermissionId($0.uint64Value) }
        let text = (response as? UNTextInputNotificationResponse)?.userText
        guard let command = AlertPresentation.command(action: response.actionIdentifier, permission: permission, text: text) else { return }
        do {
            try await BackgroundCommands.send(command, computer: computer, pane: pane)
        } catch {
            log.error("Could not send from a notification: \(error.localizedDescription, privacy: .public)")
            let failed = UNMutableNotificationContent()
            failed.title = "Not sent"
            failed.body = error.localizedDescription
            failed.threadIdentifier = pane
            try? await center.add(UNNotificationRequest(identifier: UUID().uuidString, content: failed, trigger: nil))
        }
    }
}

#if DEBUG
extension AppDelegate {
    /// For the simulator loop, which cannot tap the permission prompt:
    /// `-provisionalAlerts YES` asks quietly instead, `-exportNotifyKey YES`
    /// writes the alert key's public half to the App Group, and
    /// `-dumpNotifications YES` writes what was delivered there, as the
    /// extension left it. Answers whether it handled alerts itself.
    func followDebugArguments() -> Bool {
        let defaults = UserDefaults.standard
        guard let folder = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: NotifyKey.group) else { return false }
        if defaults.bool(forKey: "exportNotifyKey"), let key = try? NotifyKey.loadOrCreate() {
            try? Data(key.registered.utf8).write(to: folder.appending(path: "debug-notify-key.txt"))
        }
        if defaults.bool(forKey: "dumpNotifications") {
            Task {
                let shown = await UNUserNotificationCenter.current().deliveredNotifications()
                let rows = shown.map { note -> [String: String] in
                    let content = note.request.content
                    return [
                        "title": content.title, "body": content.body,
                        "category": content.categoryIdentifier, "thread": content.threadIdentifier,
                        "pane": content.userInfo[AlertKeys.pane] as? String ?? "",
                        "permission": (content.userInfo[AlertKeys.permission] as? NSNumber)?.stringValue ?? "",
                        "sealed_left": content.userInfo["sealed"] == nil ? "no" : "yes",
                        // The simulator does not run the extension for
                        // `simctl push`; this opens it as the extension would.
                        "opened": (content.userInfo["sealed"] as? String).flatMap { sealed in
                            (try? NotifyKey.load()?.open(sealed)).map { "\($0.title) | \($0.body) | \($0.presentation.actions.map(\.title))" }
                        } ?? "",
                    ]
                }
                let categories = await UNUserNotificationCenter.current().notificationCategories()
                    .map { "\($0.identifier): \($0.actions.map(\.title).joined(separator: ", "))" }
                let dump = ["delivered": rows, "categories": categories.map { ["category": $0] }]
                if let data = try? JSONSerialization.data(withJSONObject: dump, options: [.prettyPrinted, .sortedKeys]) {
                    try? data.write(to: folder.appending(path: "debug-delivered.json"))
                }
            }
        }
        guard defaults.bool(forKey: "provisionalAlerts") else { return false }
        Task {
            _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.provisional, .alert, .badge, .sound])
            UIApplication.shared.registerForRemoteNotifications()
        }
        return true
    }
}
#endif
