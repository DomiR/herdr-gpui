import Foundation
import Synchronization

/// The tags of one call's requests, in the order they went out. Each
/// request has its own (`PathFailure.tagHeader`), so when a call makes
/// several, such as one upload per picture, a report names the one that
/// failed rather than one the computer already answered.
public final class CallTags: Sendable {
    /// A request's tag and when it went out by the phone's clock.
    public struct Sent: Equatable, Sendable {
        public let tag: String
        public let began: Date
    }

    /// The most requests a call remembers; the oldest are forgotten.
    public static let limit = 8
    /// The earlier requests a report lists besides the last.
    static let listed = 3

    private let sent = Mutex<[Sent]>([])

    public init() {}

    /// A new tag for a request going out at `began`.
    public func next(at began: Date = .now) -> String {
        let tag = PathFailure.newTag()
        sent.withLock { sent in
            sent.append(Sent(tag: tag, began: began))
            if sent.count > Self.limit { sent.removeFirst(sent.count - Self.limit) }
        }
        return tag
    }

    public var requests: [Sent] {
        sent.withLock { $0 }
    }

    /// How a report names the call at `origin`: its last request, which a
    /// call's requests go one after another, so the one that failed, and
    /// the tags of a few before it; nil when nothing went out.
    public func report(origin: String) -> String? {
        let requests = requests
        guard let last = requests.last else { return nil }
        let line = PathFailure.call(tag: last.tag, origin: origin, began: last.began)
        let earlier = requests.dropLast().suffix(Self.listed).map(\.tag)
        return earlier.isEmpty ? line : "\(line), after \(earlier.joined(separator: " "))"
    }
}
