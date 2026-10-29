import SwiftUI
import UserNotifications

/// Herdr Remote: the chats and terminals running in Herdr on your
/// computers, with Live Activities on the lock screen. Each paired
/// computer keeps one live socket while the app is in front; launch draws
/// the cached state at once.
@main
struct HerdrRemoteApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    private var model: AppModel { delegate.model }
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .onOpenURL { model.open($0) }
                .task { model.tokens.start() }
                .onChange(of: model.fleet.waiting, initial: true) { _, waiting in
                    // While the app is in front, the badge follows the live
                    // count; pushes set it otherwise.
                    if phase == .active { UNUserNotificationCenter.current().setBadgeCount(waiting) }
                }
                .onChange(of: phase, initial: true) { _, phase in
                    switch phase {
                    case .active:
                        model.fleet.activate()
                        model.nearby.start()
                    case .background:
                        model.fleet.deactivate()
                        model.nearby.stop()
                    default: break
                    }
                }
        }
    }
}
