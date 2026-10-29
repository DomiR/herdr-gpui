import Foundation
import HerdrRemoteCore
import Observation

/// Which computer and pane a view is about.
struct PaneKey: Codable, Hashable, Sendable {
    let device: String
    let pane: String
}

/// One open chat: its transcript as the reducer keeps it, starting from
/// the cache and following the live socket.
@MainActor
@Observable
final class ChatStore {
    let key: PaneKey
    private(set) var transcript: Transcript?
    private(set) var cursor: Cursor?
    private(set) var connected = false
    /// The transcript came from the cache and nothing newer arrived yet.
    private(set) var stale = true
    /// Set when a change did not fit; the fleet fetches a snapshot.
    private(set) var diverged = false

    init(key: PaneKey, cached: Cache.CachedChat?) {
        self.key = key
        transcript = cached?.transcript
        cursor = cached?.cursor
    }

    func apply(_ event: LiveEvent) {
        switch event {
        case let .chatSnapshot(snapshot):
            transcript = snapshot.transcript
            cursor = snapshot.cursor
            connected = snapshot.connected
            stale = false
            diverged = false
        case let .chatResumed(_, cursor, connected):
            self.cursor = cursor
            self.connected = connected
            stale = false
        case let .chatChanges(_, changes):
            // After a change that did not fit, wait for the snapshot.
            guard !diverged, var current = transcript else { return }
            for item in changes {
                do {
                    try current.apply(item.change)
                } catch {
                    diverged = true
                    return
                }
                cursor = item.cursor
            }
            transcript = current
        case let .chatConnection(_, connected):
            self.connected = connected
        default:
            break
        }
    }
}

/// One open terminal pane's screen.
@MainActor
@Observable
final class TermStore {
    let key: PaneKey
    private(set) var screen: TermScreen?

    init(key: PaneKey) { self.key = key }

    func apply(_ screen: TermScreen) { self.screen = screen }
}
