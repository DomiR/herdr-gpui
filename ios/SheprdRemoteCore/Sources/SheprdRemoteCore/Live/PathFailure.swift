import Foundation

/// Failures waiting to reach a computer's log, oldest first. Bounded, as a
/// computer may stay away; a line stays until the computer accepts it.
public struct UnreportedFailures: Equatable, Sendable {
    /// The daemon takes ten reports at once from an address.
    public static let limit = 8

    public private(set) var lines: [String] = []

    public init() {}

    public mutating func add(_ line: String) {
        lines = Array((lines + [line]).suffix(Self.limit))
    }

    /// The computer took `line`. Newer failures may have pushed it out
    /// while it was on its way, which leaves nothing to remove.
    public mutating func accepted(_ line: String) {
        if let index = lines.firstIndex(of: line) { lines.remove(at: index) }
    }
}

/// What a computer said when it refused a call.
public enum Refusal {
    /// The most of a refusal that is kept, in characters.
    public static let limit = 300

    /// The daemon's `{"error": "..."}` as a sentence for the screen, else
    /// the status with whatever came back. Takes the whole answer: cut
    /// first, a long message would no longer parse.
    public static func text(status: Int, body: Data) -> String {
        struct Answer: Decodable { let error: String }
        guard let answer = try? JSONDecoder().decode(Answer.self, from: body), !answer.error.isEmpty else {
            guard !body.isEmpty else { return "The computer answered \(status)." }
            return "The computer answered \(status): \(String(decoding: body.prefix(limit), as: UTF8.self))"
        }
        return String(answer.error.prefix(limit))
    }
}

/// What a failed call says about the address it used, and how logs name it.
public enum PathFailure {
    /// Path segments followed by an identifier, which logs leave out: a
    /// pairing claim's is a secret, and the daemon's log keeps none either.
    private static let identified: Set<String> = ["chat", "claim", "hosts", "term", "workspaces"]

    /// Whether nothing came back from the address. An answer of any kind,
    /// a refused certificate, or a cancelled call says the path still works.
    public static func isDead(_ error: Error) -> Bool {
        switch (error as? URLError)?.code {
        case .cannotConnectToHost, .cannotFindHost, .dnsLookupFailed,
             .networkConnectionLost, .notConnectedToInternet, .timedOut:
            true
        default:
            false
        }
    }

    /// `url`'s path with identifiers replaced, and never its query, which
    /// carries what the person searched for.
    public static func route(_ url: URL?) -> String {
        guard let url else { return "(no url)" }
        var previous = ""
        let segments = url.path.split(separator: "/").map { segment in
            defer { previous = String(segment) }
            return identified.contains(previous) ? "{id}" : String(segment)
        }
        return "/" + segments.joined(separator: "/")
    }

    /// The error's text with its code, which stays the same in every
    /// language, and how long the call had been going.
    public static func summary(_ error: Error, after elapsed: Duration) -> String {
        let code = error as NSError
        return "\(code.localizedDescription) (\(code.domain) \(code.code)) after \(seconds(elapsed))"
    }

    /// The header naming one call, which the daemon's request log shows
    /// too, so a failure the phone reports can be matched with what the
    /// computer saw of it, or the absence of anything.
    public static let tagHeader = "X-Sheprd-Request"

    /// A short random name for a call: eight hex digits, nothing about
    /// the person or what the call carries.
    public static func newTag() -> String {
        String(UInt32.random(in: .min ... .max), radix: 16).leftPadded(to: 8)
    }

    /// How a report names a call: its tag, the address it went to, and
    /// when it began by the phone's clock, to the millisecond, as the
    /// daemon's log is in UTC to the microsecond.
    public static func call(tag: String, origin: String, began: Date) -> String {
        let format = ISO8601DateFormatter()
        format.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return "call \(tag) at \(origin) begun \(format.string(from: began))"
    }

    /// `elapsed` to a tenth of a second, as logs show it.
    public static func seconds(_ elapsed: Duration) -> String {
        let parts = elapsed.components
        let value = Double(parts.seconds) + Double(parts.attoseconds) / 1e18
        return value.formatted(.number.precision(.fractionLength(1)).locale(Locale(identifier: "en_US_POSIX"))) + "s"
    }
}

private extension String {
    func leftPadded(to width: Int) -> String {
        String(repeating: "0", count: max(0, width - count)) + self
    }
}
