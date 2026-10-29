import Foundation

/// What was typed into a terminal but is not on its screen yet, shown so a
/// slow network does not leave typing blind. Each input keeps one entry,
/// in the order sent. An entry goes once the computer took it and a screen
/// arrived after, or `linger` after it was taken when the screen did not
/// change (a password, a program that draws nothing).
public struct TermEcho: Sendable {
    public typealias Instant = ContinuousClock.Instant

    /// How long a taken input stays with no new screen.
    public static let linger: Duration = .seconds(1)

    private enum State: Sendable, Equatable {
        case queued
        case sent
        case taken(Instant)
    }

    private struct Entry: Sendable {
        /// The key's character, `⏎` or `⇥`, `nil` for backspace, and empty
        /// for what shows nothing (arrows, chords, pastes).
        var glyph: String?
        var state: State
    }

    private var entries: [Entry] = []
    /// When the request out now left.
    private var sentAt: Instant?
    private var screenAt: Instant?

    public init() {}

    /// What the strip shows: the typed characters, with backspaces taking
    /// back the ones before them, and `⌫` for one that reaches further.
    public var text: String {
        var shown: [Character] = []
        for entry in entries {
            guard let glyph = entry.glyph else {
                if let last = shown.last, last != "⌫" { shown.removeLast() } else { shown.append("⌫") }
                continue
            }
            shown.append(contentsOf: glyph)
        }
        return String(shown)
    }

    /// Input waiting for its request, one entry per key or paste.
    public mutating func queue(_ send: TermInput.Send) {
        switch send {
        case let .keys(keys): entries += keys.map { Entry(glyph: Self.glyph(for: $0), state: .queued) }
        case .paste: entries.append(Entry(glyph: "", state: .queued))
        }
    }

    /// The next `count` queued inputs left in one request.
    public mutating func sending(_ count: Int, at now: Instant) {
        var left = count
        for index in entries.indices where left > 0 && entries[index].state == .queued {
            entries[index].state = .sent
            left -= 1
        }
        sentAt = now
    }

    /// The computer took the request out. A screen that arrived meanwhile
    /// was read after it, as the computer reads one at once.
    public mutating func delivered(at now: Instant) {
        for index in entries.indices where entries[index].state == .sent {
            entries[index].state = .taken(now)
        }
        if let screenAt, let sentAt, screenAt > sentAt { dropTaken(by: now) }
        sentAt = nil
    }

    /// A new screen arrived: whatever the computer took is on it.
    public mutating func screen(at now: Instant) {
        screenAt = now
        dropTaken(by: now)
    }

    /// Drops what was taken `linger` ago and never showed.
    public mutating func expire(at now: Instant) {
        dropTaken(by: now - Self.linger)
    }

    /// A request failed: what followed it is dropped too.
    public mutating func clear() {
        entries.removeAll()
        sentAt = nil
    }

    private mutating func dropTaken(by moment: Instant) {
        let kept = entries.firstIndex {
            guard case let .taken(at) = $0.state else { return true }
            return at > moment
        }
        entries.removeFirst(kept ?? entries.count)
    }

    /// What a key in Herdr's syntax shows.
    static func glyph(for key: String) -> String? {
        if key.count == 1 { return key }
        switch key {
        case "space": return " "
        case "plus": return "+"
        case "minus": return "-"
        case "slash": return "/"
        case "backtick": return "`"
        case "enter": return "⏎"
        case "tab": return "⇥"
        case "backspace": return nil
        default: return ""
        }
    }
}
