import SheprdRemoteCore
import SwiftUI

/// Settings > Notifications' push relay section: the setting, on by default
/// (`PushRelaySetting`), what the push relay sees (`docs/apns-relay-plan.md`,
/// Opt-in), and how many paired computers can deliver this phone's pushes,
/// opening how each one does.
/// Turning it back on after turning it off asks first, with the same
/// explanation, as Sheprd does on the computer.
struct PushRelaySection: View {
    @Environment(AppModel.self) private var model
    @AppStorage(PushRelaySetting.key, store: PushRelaySetting.store) private var pushRelay = true
    @State private var confirming = false
    @State private var explaining = false

    /// The plan's explanation, kept true as the design changes.
    static let explanation = """
    Get Live Activities and alerts when you are away from your computers. Apple only accepts pushes for this app from Sheprd, so they go through a small server Sheprd runs.

    What the push relay sees: this phone's push token while it sends a push, the time and size of each push, and the IP addresses of your computer and phone.

    What it never sees: your prompts, chats, code, file names, or terminal output. Alerts and Live Activities are encrypted so that only this phone can read them.

    What it cannot do: connect to your computer, run anything, or answer for you. Tokens are not stored or logged.

    Apple also sees the push token, timing, and size of every push. Computers are ready for it unless it was turned off in Sheprd's Remote control settings there.
    """

    var body: some View {
        Section {
            Toggle("Use the Push Relay", isOn: Binding(
                get: { pushRelay },
                set: { on in
                    if on {
                        confirming = true
                    } else {
                        pushRelay = false
                        Task { await model.tokens.pushRelayChanged(to: false) }
                    }
                }
            ))
            if !model.computers.isEmpty {
                NavigationLink {
                    PushDeliveryView().makesRoomForConnectionToast()
                } label: {
                    LabeledContent("Delivery", value: "\(readyCount) of \(model.computers.count) ready")
                }
            }
            if pushRelay, let error = model.tokens.pushRelay.lastError {
                Text(error).font(.footnote).foregroundStyle(.secondary)
            }
            Button("What the Push Relay Sees") { explaining = true }
            if !pushRelay, model.tokens.delivery.values.contains(.pushRelay) {
                Text("A computer sends pushes through the push relay. Turn it on here to get its alerts and Live Activities.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        } header: {
            Text("Push Relay")
        } footer: {
            Text(pushRelay
                ? "Computers get sealed handles, not this phone's push tokens. Turning it off revokes them."
                : "Computers push only through the push relay, so this phone gets no alerts or Live Activities from them.")
        }
        .alert("Push through the Sheprd push relay?", isPresented: $confirming) {
            Button("Turn On") {
                pushRelay = true
                Task { await model.tokens.pushRelayChanged(to: true) }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text(Self.explanation)
        }
        .alert("What the push relay sees", isPresented: $explaining) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(Self.explanation)
        }
    }

    private var readyCount: Int {
        model.computers.count(where: { computer in
            Self.ready(model.tokens.delivery[computer.fingerprint],
                       reached: !model.tokens.unreached.contains(computer.fingerprint),
                       pushRelay: pushRelay)
        })
    }

    /// Whether a computer can push to this phone as things stand: it was
    /// reached, and delivers the way this phone's setting expects.
    static func ready(_ delivery: ActivityTokens.Delivery?, reached: Bool, pushRelay: Bool) -> Bool {
        guard reached else { return false }
        return switch delivery {
        case .pushRelay: pushRelay
        case .off, .unknown, nil: false
        }
    }

    /// How one computer delivers, as far as its last answer said, and
    /// whether the last registration reached it.
    static func label(_ delivery: ActivityTokens.Delivery?, reached: Bool, pushRelay: Bool) -> String {
        if !reached {
            return "Not reached; tried again when online"
        }
        return switch delivery {
        case .pushRelay: pushRelay ? "Push relay" : "Push relay, off here"
        case .off: "No push set up"
        case .unknown: "Not reported"
        case nil: "Not asked yet"
        }
    }
}
