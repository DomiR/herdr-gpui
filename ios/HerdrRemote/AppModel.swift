import ActivityKit
import Foundation
import os
import HerdrRemoteCore
import Network
import UIKit

/// Where the app should go next: a chat or terminal opened from a Live
/// Activity, a notification, or a list.
private let log = Logger(subsystem: "so.pen.herdr-remote", category: "app")

enum Route: Hashable {
    case chat(PaneKey, title: String)
    case term(PaneKey, title: String)
}

/// The app's state: paired computers, their live links, and the tokens
/// Live Activities need.
@MainActor
@Observable
final class AppModel {
    private(set) var computers: [Computer] = [] {
        didSet {
            tokens.computers = computers
            fleet.sync(computers)
        }
    }
    var message: String?
    /// Set by a deep link; the root view shows it and clears it.
    var pendingRoute: Route?
    /// A new workspace or chat being set up, as a sheet over every tab.
    var creating: Creating?
    let sessions: Sessions
    let tokens: ActivityTokens
    let fleet: Fleet
    let nearby = NearbyBrowser()
    let bluetooth = NearbyBluetooth()
    /// Nearby computers the person closed the card for, this launch.
    var dismissedNearby: Set<String> = []
    private let paths = NWPathMonitor()

    init() {
        let sessions = Sessions()
        self.sessions = sessions
        tokens = ActivityTokens(sessions: sessions)
        fleet = Fleet(sessions: sessions)
        do {
            computers = try ComputerStore.load()
        } catch {
            log.error("Could not read the paired computers: \(error.localizedDescription, privacy: .public)")
            message = error.localizedDescription
        }
        // Property observers do not run during `init`.
        tokens.computers = computers
        fleet.sync(computers)
        paths.pathUpdateHandler = { [weak self] _ in
            Task { @MainActor in self?.fleet.networkChanged() }
        }
        paths.start(queue: .main)
    }

    func add(_ computer: Computer) {
        // A computer is its CA: pairing the same CA (or the same device id)
        // again replaces the old record. Names are display only.
        computers.removeAll { $0.fingerprint == computer.fingerprint || $0.device == computer.device }
        computers.append(computer)
        save()
        Task { await AppDelegate.enableAlerts() }
        Task { await tokens.sync(only: computer.fingerprint) }
    }

    func forget(_ computer: Computer) {
        computers.removeAll { $0.device == computer.device }
        save()
        Task { await sessions.forget(computer) }
    }

    private func save() {
        do {
            try ComputerStore.save(computers)
        } catch {
            log.error("Could not save the paired computers: \(error.localizedDescription, privacy: .public)")
            message = error.localizedDescription
        }
    }

    /// Opens a Live Activity's or notification's link in the app.
    func open(_ url: URL) {
        guard let link = ChatLink(url: url) else { return }
        guard let fingerprint = link.fingerprint,
              let computer = computers.first(where: { $0.fingerprint == fingerprint })
        else {
            message = "\(link.computer.isEmpty ? "That computer" : link.computer) is not paired with this phone."
            return
        }
        let title = fleet.chatItems.first { $0.computer.device == computer.device && $0.chat.pane == link.pane }?.chat.title
        pendingRoute = .chat(PaneKey(device: computer.device, pane: link.pane), title: title ?? "Chat")
    }

    /// A computer nearby with its pairing dialog open that this phone has
    /// not paired, for the proximity card.
    var nearbyOffer: NearbyComputer? {
        // The network first when a computer is found both ways: it is faster.
        (nearby.computers + bluetooth.computers).first { candidate in
            candidate.pairing && !dismissedNearby.contains(candidate.id)
                && !computers.contains { candidate.matches($0.fingerprint) }
        }
    }

    /// The name to show for `computer`: its own, plus the start of its
    /// fingerprint when another paired computer has the same name.
    func displayName(_ computer: Computer) -> String {
        let twins = computers.filter { $0.name == computer.name }.count
        guard twins > 1 else { return computer.name }
        return "\(computer.name) (\(computer.fingerprint.hex.prefix(8)))"
    }

    /// The paired computer an activity names by fingerprint, if any.
    func computer(for facts: ChatActivityFacts) -> Computer? {
        guard let fingerprint = facts.fingerprint else { return nil }
        return computers.first { $0.fingerprint == fingerprint }
    }

    #if DEBUG
    /// Starts a local activity with sample content, to check the layouts on
    /// a device without an APNs key.
    func startSample() {
        let facts = ChatActivityFacts(
            computer: computers.first?.name ?? "studio",
            computerId: computers.first?.fingerprint.hex ?? String(repeating: "0", count: 64),
            workspace: "herdr-gpui",
            chatTitle: "split tests",
            agent: "Codex",
            pane: "sample"
        )
        let state = ChatActivityState(
            phase: .working,
            step: "Writing mockups/index.html",
            startedAt: Date().addingTimeInterval(-250),
            cost: "$0.34",
            planDone: 2,
            planTotal: 4,
            working: 2,
            waiting: 1
        )
        do {
            _ = try Activity.request(
                attributes: ChatActivityAttributes(facts: facts),
                content: .init(state: state, staleDate: nil)
            )
        } catch {
            message = error.localizedDescription
        }
    }
    #endif
}
