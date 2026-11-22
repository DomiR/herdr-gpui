import Foundation
import SheprdRemoteCore

/// Settings' "Send test push": asks one paired computer for a sealed test
/// alert (`POST /api/apns/test`), then waits for the Notification Service
/// Extension to record its arrival (`PushTestReceipts`). Says where it
/// stopped: the computer, APNs, or the phone.
///
/// With the push relay on, the phone can also test itself with no computer
/// online (`sendFromThisPhone`): it seals a test to its own key
/// (`SelfTestPush`), posts it to the push relay through a handle for its
/// own alert token, and waits for the same receipt.
@MainActor
@Observable
final class PushTests {
    enum Status: Equatable {
        case sending
        /// APNs took it; the phone has not opened it yet.
        case waiting
        case arrived(seconds: Double)
        case failed(String)
    }

    /// How long an accepted push may take before it counts as lost.
    static let patience: Duration = .seconds(30)
    /// How long iOS may take to hand over a first notification token.
    static let tokenPatience: Duration = .seconds(10)

    private(set) var status: [Fingerprint: Status] = [:]
    /// The test this phone sent itself, if any.
    private(set) var ownStatus: Status?
    private let sessions: Sessions
    private let tokens: ActivityTokens
    private let group = UserDefaults(suiteName: "group.so.pen.herdr.remote")

    init(sessions: Sessions, tokens: ActivityTokens) {
        self.sessions = sessions
        self.tokens = tokens
    }

    func send(to computer: Computer) async {
        let key = computer.fingerprint
        guard status[key] != .sending, status[key] != .waiting else { return }
        status[key] = .sending
        let id = UUID().uuidString
        let started = Date()
        let answer: API.PushTestAnswer
        do {
            let relay = tokens.relayOrigin(computer.device).map { [$0] } ?? []
            let session = try await sessions.session(for: computer, fallback: relay)
            answer = try await session.client.sendTestPush(id: id, bearer: session.token)
        } catch RemoteError.status(503, _) {
            status[key] = .failed(PushRelaySetting.isOn
                ? "\(computer.name) has the push relay off. Turn it on in Sheprd's Remote control settings there."
                : "\(computer.name) cannot push to this phone while the push relay is off here. Turn it on here, and check it is not turned off in Sheprd's Remote control settings there.")
            return
        } catch let RemoteError.status(400, text) where text.contains("not registered") {
            status[key] = .failed(PushRelaySetting.isOn
                ? "\(computer.name) has no push relay handle from this phone that it can use. Allow notifications for Sheprd and reopen the app."
                : "\(computer.name) pushes only through the push relay, which is off here. Turn it on above.")
            return
        } catch {
            status[key] = .failed("Could not ask \(computer.name): \(error.localizedDescription)")
            return
        }
        switch answer.outcome {
        case .delivered:
            status[key] = .waiting
        case .gone:
            status[key] = .failed("APNs no longer knows this phone's token. Reopen the app to send a new one, then try again.")
            return
        case .limited:
            status[key] = .failed("APNs or the push relay is limiting pushes to this phone. Try again in a minute.")
            return
        case .refused where answer.reason == nil && answer.status == 403:
            // The push relay gives no reason; 403 is a handle it no longer
            // takes. The phone attests again and sends the computer new
            // handles, as reusing the cached ones would fail the same way.
            let failure = await tokens.pushRelayRejected(by: computer)
            status[key] = .failed(failure.map {
                "The push relay no longer took this phone's handle, and sending \(computer.name) a new one failed: \($0)"
            } ?? "The push relay no longer took this phone's handle. A new one was sent to \(computer.name); try again.")
            return
        case .refused:
            let reason = answer.reason ?? "HTTP \(answer.status ?? 0)"
            status[key] = .failed("APNs or the push relay refused it: \(reason).")
            return
        case .unreachable:
            status[key] = .failed("\(computer.name) could not reach APNs or the push relay.")
            return
        }
        status[key] = await arrival(of: id, sent: started)
    }

    /// Sends this phone a test push through the push relay, with no
    /// computer involved, and waits for it as `send(to:)` does.
    func sendFromThisPhone() async {
        guard ownStatus != .sending, ownStatus != .waiting else { return }
        ownStatus = .sending
        guard PushRelaySetting.isOn else {
            ownStatus = .failed("This phone has no APNs key of its own, so it can only push itself through the push relay. Turn on Use the Push Relay above.")
            return
        }
        // `alertToken` says why when there is none.
        guard let token = await alertToken() else { return }
        let relay = tokens.pushRelay
        guard let credential = await relay.usableCredential() else {
            ownStatus = .failed("The push relay has not let this phone in: \(relay.lastError ?? "turn the push relay off and on again.")")
            return
        }
        guard let handle = relay.handle(for: token, kind: .alert, credential: credential) else {
            ownStatus = .failed("Could not make a push relay handle for this phone's notification token.")
            return
        }
        let id = UUID().uuidString
        let request: SelfTestPush.Request
        do {
            request = try SelfTestPush.request(id: id, handle: handle, device: NotifyKey.own().publicKey)
        } catch {
            ownStatus = .failed("Could not seal the test to this phone's own key: \(error.localizedDescription)")
            return
        }
        let started = Date()
        let outcome: PushRelayOutcome
        do {
            outcome = try await relay.push(request)
        } catch {
            ownStatus = .failed("Could not reach the push relay: \(error.localizedDescription)")
            return
        }
        if let failure = outcome.failure {
            // The next try attests again rather than reuse what failed.
            if outcome.needsNewCredential { relay.rejected() }
            ownStatus = .failed(failure)
            return
        }
        ownStatus = .waiting
        ownStatus = await arrival(of: id, sent: started)
    }

    /// This phone's alert token. A phone that never paired was never asked
    /// to show alerts, so it has none: this asks, as pairing does, and
    /// waits briefly for iOS to hand the token over. `nil`, with the reason
    /// in `ownStatus`, when there is none.
    private func alertToken() async -> String? {
        if let token = tokens.alert { return token }
        guard await AppDelegate.enableAlerts() else {
            ownStatus = .failed("Notifications are off for Sheprd. Allow them in the iPhone's Settings, then try again.")
            return nil
        }
        let deadline = ContinuousClock.now + Self.tokenPatience
        while ContinuousClock.now < deadline {
            if let token = tokens.alert { return token }
            try? await Task.sleep(for: .milliseconds(100))
        }
        ownStatus = .failed("iOS has not given this phone a notification token yet. Check that it is online, then try again.")
        return nil
    }

    /// Waits up to `patience` for test `id`, sent at `sent`, to arrive.
    private func arrival(of id: String, sent: Date) async -> Status {
        let deadline = ContinuousClock.now + Self.patience
        while ContinuousClock.now < deadline {
            if let arrival = PushTestReceipts.arrival(of: id, in: group) {
                return .arrived(seconds: arrival.timeIntervalSince(sent))
            }
            try? await Task.sleep(for: .milliseconds(250))
        }
        return .failed("APNs took it, but it did not arrive within 30 seconds. Check that notifications are on for Sheprd and Focus allows them.")
    }

    /// The line Settings shows under a computer's button.
    static func text(_ status: Status) -> String {
        switch status {
        case .sending: "Sending…"
        case .waiting: "Sent. Waiting for it to arrive…"
        case let .arrived(seconds): "Arrived in \(seconds.formatted(.number.precision(.fractionLength(1)))) s."
        case let .failed(why): why
        }
    }
}
