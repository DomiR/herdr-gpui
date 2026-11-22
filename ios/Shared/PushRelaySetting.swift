import Foundation

/// Whether this phone uses the Sheprd push relay (`docs/apns-relay-plan.md`).
/// On by default: the App Store and TestFlight app gets no push any other
/// way, so a new install, or a reinstall (which clears the App Group), turns
/// it on at launch, and only the user turning it off in Settings keeps it
/// off. Kept in the App Group's defaults, so the widget extension knows to
/// show no Live Activity text that came unsealed while the push relay holds
/// this phone's tokens.
enum PushRelaySetting {
    static let key = "pushRelay"
    static var store: UserDefaults? { UserDefaults(suiteName: NotifyKey.group) }

    /// Unset reads as on, as `applyDefault` stores at launch.
    static var isOn: Bool {
        store?.object(forKey: key) as? Bool ?? true
    }

    /// Stores the default, on, when the user has not chosen yet, so the
    /// extensions and `@AppStorage` read what the app acts on. Called first
    /// at launch, before anything registers with the computers.
    static func applyDefault() {
        guard let store, store.object(forKey: key) == nil else { return }
        store.set(true, forKey: key)
    }
}
