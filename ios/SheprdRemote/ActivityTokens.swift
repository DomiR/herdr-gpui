import ActivityKit
import Foundation
import SheprdRemoteCore

/// Collects the push tokens ActivityKit hands the app and sends them to the
/// paired computers (`POST /api/apns`): the push-to-start token, which lets
/// a computer start an activity while the app is closed, goes to every
/// computer; each activity's own token goes only to the computer whose CA
/// fingerprint the activity carries as `computerId`, never by name.
///
/// With the push relay on (`PushRelaySetting`), each computer gets push
/// relay handles instead of tokens (`PushRelayAccess`), and with them the
/// notify key it seals Live Activity content to. Computers push only
/// through the push relay, so with it off no token goes out at all: a
/// computer would keep it and never send to it.
///
/// A paired computer can put another computer's id in an activity it
/// starts. That only makes the app hand the other computer a token for an
/// activity the claimant started, which is harmless; the claimant can never
/// obtain a token for an activity another computer started.
@MainActor
@Observable
final class ActivityTokens {
    struct Running: Identifiable, Equatable {
        let id: String
        let attributes: ChatActivityFacts
        var token: String?
    }

    /// How a computer said it delivers pushes when it last took this
    /// phone's registration, for Settings.
    enum Delivery: Equatable {
        case pushRelay
        /// It answered that push is not set up there.
        case off
        /// An older computer, which does not say or names a mode this
        /// phone does not know.
        case unknown
    }

    /// The setting that keeps this phone's activities generic: every
    /// computer leaves the chat's name and steps out of them.
    static let genericSetting = "liveActivitiesGeneric"

    private(set) var pushToStart: String?
    /// The app's own APNs token, for sealed alerts.
    private(set) var alert: String?
    private(set) var running: [String: Running] = [:]
    private(set) var lastError: String?
    private(set) var delivery: [Fingerprint: Delivery] = [:]

    /// The paired computers, kept current by the app model.
    var computers: [Computer] = []
    /// The relay proxy address for a computer's device id, while one
    /// listens: a fleet member or linked host may answer only through it.
    @ObservationIgnored var relayOrigin: (String) -> String? = { _ in nil }
    /// Computers the last registration did not reach. Each is registered
    /// again once it comes online (`online`), so a computer away when a
    /// token or the push relay setting changed still gets them.
    private(set) var unreached: Set<Fingerprint> = []
    let pushRelay = PushRelayAccess()
    private let sessions: Sessions
    private var started = false
    /// Each computer's sync running or last queued; the next for that
    /// computer waits for it.
    private var syncing: [Fingerprint: Task<String?, Never>] = [:]

    init(sessions: Sessions) {
        self.sessions = sessions
    }

    /// The push relay setting changed. Off, the handles this phone made are
    /// revoked first; either way every computer then gets what it should
    /// use, handles or nothing, which replace what it held.
    func pushRelayChanged(to on: Bool) async {
        do {
            try NotifyKey.mirror()
        } catch {
            lastError = "Live Activity keys: \(error.localizedDescription)"
        }
        if !on { await pushRelay.turnedOff() }
        await sync()
    }

    /// The push relay refused a handle `computer` held: attests again and
    /// sends it new handles. Answers why the computer did not get them, or
    /// `nil` when it did.
    func pushRelayRejected(by computer: Computer) async -> String? {
        pushRelay.rejected()
        return await sync(only: computer.fingerprint)[computer.fingerprint]
    }

    /// `computer` is online again: registers with it when the last try
    /// did not reach it. A try still running may have started before the
    /// link found its relay proxy, so it is waited for, and registered
    /// again when it too misses: the link sends no second online event.
    func online(_ computer: Computer) {
        let fingerprint = computer.fingerprint
        if unreached.contains(fingerprint) {
            Task { await sync(only: fingerprint) }
            return
        }
        guard let running = syncing[fingerprint] else { return }
        Task {
            _ = await running.value
            if unreached.contains(fingerprint) { await sync(only: fingerprint) }
        }
    }

    /// Starts watching ActivityKit; safe to call more than once.
    func start() {
        guard !started else { return }
        started = true
        // The widget reads the keys from the App Group file only while the
        // push relay is on; this writes or removes it to match.
        try? NotifyKey.mirror()
        Task {
            for await data in Activity<ChatActivityAttributes>.pushToStartTokenUpdates {
                pushToStart = data.hexString
                await sync()
            }
        }
        Task {
            for await activity in Activity<ChatActivityAttributes>.activityUpdates {
                watch(activity)
            }
        }
        for activity in Activity<ChatActivityAttributes>.activities {
            watch(activity)
        }
    }

    /// A new APNs token from iOS: every computer gets it, with the key
    /// alerts are sealed to.
    func setAlert(_ token: String) {
        guard token != alert else { return }
        alert = token
        Task { await sync() }
    }

    private func watch(_ activity: Activity<ChatActivityAttributes>) {
        guard running[activity.id] == nil else { return }
        // Sealed attributes open only with the key of the computer they
        // name; one no key opens shows the cover, and its token goes nowhere.
        guard let facts = activity.attributes.facts(keys: NotifyKey.activityKeys()) else { return }
        if facts.namesChat, UserDefaults.standard.bool(forKey: Self.genericSetting) {
            let id = activity.id
            Task { await Self.end(id) }
            return
        }
        running[activity.id] = Running(id: activity.id, attributes: facts, token: activity.pushToken?.hexString)
        Task {
            for await data in activity.pushTokenUpdates {
                running[activity.id]?.token = data.hexString
                if let owner = facts.fingerprint {
                    await sync(only: owner)
                }
            }
        }
        Task {
            for await state in activity.activityStateUpdates where state == .ended || state == .dismissed {
                running[activity.id] = nil
                if let owner = facts.fingerprint {
                    await sync(only: owner)
                }
                return
            }
        }
    }

    /// Tells the computers about Keep Generic. Turned on, it first ends
    /// every activity: a title and workspace are fixed when one starts, so
    /// no later push could take them off the lock screen, and what one last
    /// said may name the chat too. That needs no network, so it does not
    /// wait on a computer that is away; `watch` ends what such a computer
    /// starts before it hears.
    func genericChanged(to generic: Bool) async {
        if generic {
            for id in Activity<ChatActivityAttributes>.activities.map(\.id) {
                await Self.end(id)
            }
        }
        await sync()
    }

    /// Ends the activity with `id` at once. By id and off this actor, since
    /// an activity it holds cannot be sent to ActivityKit.
    private nonisolated static func end(_ id: String) async {
        await Activity<ChatActivityAttributes>.activities.first { $0.id == id }?
            .end(nil, dismissalPolicy: .immediate)
    }

    /// Sends each paired computer (or only the one with CA `only`) push
    /// relay handles for its tokens while the push relay is on, and answers
    /// why each computer that did not take them did not. Syncs for one
    /// computer run one at a time, in the order asked, so one started under
    /// the other setting cannot finish after it; different computers do
    /// not wait on each other, so one that is away delays no other.
    @discardableResult
    func sync(only: Fingerprint? = nil) async -> [Fingerprint: String] {
        var queued: [(Fingerprint, Task<String?, Never>)] = []
        for computer in computers where only == nil || computer.fingerprint == only {
            let previous = syncing[computer.fingerprint]
            let fingerprint = computer.fingerprint
            let task = Task {
                _ = await previous?.value
                return await self.register(fingerprint)
            }
            syncing[computer.fingerprint] = task
            queued.append((computer.fingerprint, task))
        }
        var failures: [Fingerprint: String] = [:]
        for (fingerprint, task) in queued {
            failures[fingerprint] = await task.value
        }
        return failures
    }

    /// Registers with the paired computer with CA `fingerprint`, answering
    /// why it failed, or `nil`. It looks the computer up again after each
    /// wait, so one forgotten meanwhile gets nothing and no notify key is
    /// made again for it. It stops when the push relay setting changes
    /// under it, so no token goes out after it was turned on and no handle
    /// after it was turned off; `pushRelayChanged` queues the sync for the
    /// new setting.
    private func register(_ fingerprint: Fingerprint) async -> String? {
        let forgotten = "This computer is no longer paired."
        let changed = "The push relay setting changed while registering."
        guard computers.contains(where: { $0.fingerprint == fingerprint }) else { return forgotten }
        let viaPushRelay = PushRelaySetting.isOn
        // Attests first when needed; without a credential no handle can be
        // made, and no token goes out either while the push relay is on.
        let credential = viaPushRelay ? await pushRelay.usableCredential() : nil
        guard PushRelaySetting.isOn == viaPushRelay else { return changed }
        guard let computer = computers.first(where: { $0.fingerprint == fingerprint }) else { return forgotten }
        let recipient = { (token: String?, kind: PushRelayKind) -> API.PushRecipient? in
            // Without the push relay nothing reaches a token, so none goes out.
            guard let token, viaPushRelay else { return nil }
            guard let credential, let handle = self.pushRelay.handle(for: token, kind: kind, credential: credential) else {
                return nil
            }
            return .pushRelayHandle(handle, environment: credential.environment)
        }
        let activities = running.values
            .filter { $0.attributes.fingerprint == computer.fingerprint }
            .compactMap { running in
                recipient(running.token, .activity).map { API.APNsRegistration.Activity(pane: running.attributes.pane, token: $0) }
            }
        let alert = recipient(alert, .alert)
        // Each computer seals to its own notify key. Without one, alerts
        // would arrive sealed to nobody: send neither, and the computer
        // sends this phone nothing through APNs. Through the push relay,
        // Live Activity content is sealed to it too.
        let needsKey = alert != nil || viaPushRelay
        let notifyKey = needsKey ? try? NotifyKey.loadOrCreate(for: computer.fingerprint).registered : nil
        let generic = UserDefaults.standard.bool(forKey: Self.genericSetting)
        // With the push relay off, the computer drops every handle it holds,
        // so it stops pushing even if revoking them at the push relay failed.
        let registration = viaPushRelay
            ? API.APNsRegistration(
                pushToStart: recipient(pushToStart, .pushToStart),
                activities: activities,
                alert: notifyKey == nil ? nil : alert,
                notifyKey: notifyKey,
                genericActivities: generic
            )
            : .pushRelayOff(genericActivities: generic)
        do {
            let relay = relayOrigin(computer.device).map { [$0] } ?? []
            let session = try await sessions.session(for: computer, fallback: relay)
            guard PushRelaySetting.isOn == viaPushRelay else { return changed }
            guard computers.contains(where: { $0.fingerprint == fingerprint }) else { return forgotten }
            let answer = try await session.client.registerAPNs(registration, bearer: session.token)
            unreached.remove(computer.fingerprint)
            delivery[computer.fingerprint] = switch answer.delivery {
            case .pushRelay: .pushRelay
            case nil: .unknown
            }
            // Through the push relay, no credential means the computer got
            // no handles at all.
            let failure = viaPushRelay && credential == nil
                ? pushRelay.lastError ?? "The push relay gave this phone no credential."
                : nil
            lastError = failure
            return failure
        } catch RemoteError.status(503, _) {
            // A computer with no push set up. Nothing is wrong, and the
            // session is good: it answered.
            delivery[computer.fingerprint] = .off
            unreached.remove(computer.fingerprint)
            lastError = nil
            return nil
        } catch {
            await sessions.forget(computer)
            unreached.insert(computer.fingerprint)
            let failure = "\(computer.name) (\(computer.fingerprint.hex.prefix(8))): \(error.localizedDescription)"
            lastError = failure
            return failure
        }
    }
}
