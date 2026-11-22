import Foundation

/// The envelope Claude Code wraps around pasted text,
/// `<pasted_content id="…">…</pasted_content>`, which its log keeps.
public enum PastedEnvelope {
    private static let open = "<pasted_content"
    private static let close = "</pasted_content>"

    /// `text` without a leading envelope; any other markup stays, as does
    /// text that only looks like one (`<pasted_contents>`). Only the exact
    /// tag Claude writes, `<pasted_content>` or `<pasted_content id="ID">`
    /// with a short id, is unwrapped: anything more in it would reach the
    /// agent unseen, so such text shows as it is.
    public static func unwrap(_ text: String) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.hasPrefix(open) else { return text }
        let rest = trimmed.dropFirst(open.count)
        let body: Substring
        if rest.first == ">" {
            body = rest.dropFirst()
        } else if rest.hasPrefix(" id=\""), let quote = rest.dropFirst(5).firstIndex(of: "\"") {
            let id = rest[rest.index(rest.startIndex, offsetBy: 5) ..< quote]
            let after = rest[rest.index(after: quote)...]
            guard (1 ... maxID).contains(id.count), id.allSatisfy(isIDCharacter), after.first == ">" else { return text }
            body = after.dropFirst()
        } else {
            return text
        }
        var shown = body.trimmingCharacters(in: .whitespacesAndNewlines)
        if shown.hasSuffix(close) { shown = String(shown.dropLast(close.count)) }
        return shown.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static let maxID = 32

    private static func isIDCharacter(_ character: Character) -> Bool {
        character.isASCII && (character.isLetter || character.isNumber || character == "-" || character == "_")
    }
}

/// A brief read for its card: the goal, the sections it lists, and the
/// whole text for the sheet. Briefs are free text, so this only finds
/// what is plainly there: Markdown headings, and `Label:` lines over a
/// list.
public struct BriefOutline: Hashable, Sendable {
    public struct Section: Hashable, Sendable, Identifiable {
        public enum Kind: Hashable, Sendable { case reading, steps, checks, other }

        public var title: String
        /// List items right under it; 0 for a heading over prose.
        public var count: Int
        public var kind: Kind
        /// The line it starts on in `text`, so the sheet can scroll there.
        public var line: Int
        public var id: Int { line }
    }

    /// The first sentence, without a `Task:` label.
    public var goal: String
    public var sections: [Section]
    /// The brief without its paste envelope.
    public var text: String
    public var words: Int

    public init(_ raw: String) {
        text = PastedEnvelope.unwrap(raw)
        words = text.split(whereSeparator: \.isWhitespace).count
        let lines = text.components(separatedBy: .newlines)
        sections = Self.sections(lines)
        goal = Self.goal(lines)
    }

    private static func sections(_ lines: [String]) -> [Section] {
        var found: [Section] = []
        // Lines inside a fenced code block are code, never sections: the
        // sheet cuts the brief at each section, so a false one would split
        // a code block.
        var fence: Substring?
        for (index, line) in lines.enumerated() {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if let open = fence {
                if Self.fence(trimmed).map({ $0.first == open.first && $0.count >= open.count }) == true {
                    fence = nil
                }
                continue
            }
            if let opened = Self.fence(trimmed) {
                fence = opened
                continue
            }
            if let title = heading(trimmed) {
                found.append(Section(title: title, count: 0, kind: kind(title), line: index))
            } else if let title = label(trimmed), index + 1 < lines.count, isItem(lines[index + 1]) {
                found.append(Section(title: title, count: 0, kind: kind(title), line: index))
            } else if isItem(line), !found.isEmpty, !line.hasPrefix("  ") {
                found[found.count - 1].count += 1
            }
        }
        return found.filter { $0.count > 0 || $0.line > 0 }
    }

    /// The first paragraph's first sentence, or the first heading when the
    /// brief opens with one.
    private static func goal(_ lines: [String]) -> String {
        var paragraph: [String] = []
        for line in lines {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.isEmpty {
                if paragraph.isEmpty { continue }
                break
            }
            if let title = heading(trimmed) {
                if paragraph.isEmpty { return title }
                break
            }
            if isItem(line) || label(trimmed) != nil || fence(trimmed) != nil { break }
            paragraph.append(trimmed)
        }
        var joined = paragraph.joined(separator: " ")
        for prefix in ["Task:", "Goal:"] where joined.lowercased().hasPrefix(prefix.lowercased()) {
            joined = joined.dropFirst(prefix.count).trimmingCharacters(in: .whitespaces)
        }
        return firstSentence(joined)
    }

    private static func firstSentence(_ text: String) -> String {
        var index = text.startIndex
        while index < text.endIndex {
            let next = text.index(after: index)
            if ".?!".contains(text[index]), next < text.endIndex, text[next] == " " {
                let after = text[next...].drop { $0 == " " }
                if after.first?.isUppercase == true { return String(text[...index]) }
            }
            index = next
        }
        return text
    }

    /// The run of backticks or tildes that opens or closes a fenced code
    /// block on `line`, if it is one.
    private static func fence(_ line: String) -> Substring? {
        guard let mark = line.first, mark == "`" || mark == "~" else { return nil }
        let run = line.prefix { $0 == mark }
        return run.count >= 3 ? run : nil
    }

    private static func heading(_ line: String) -> String? {
        let hashes = line.prefix { $0 == "#" }.count
        guard (1 ... 6).contains(hashes), line.dropFirst(hashes).first == " " else { return nil }
        let title = line.dropFirst(hashes).trimmingCharacters(in: .whitespaces)
        return title.isEmpty ? nil : title
    }

    /// `Read first:` alone on its line, short enough to be a label.
    private static func label(_ line: String) -> String? {
        guard line.hasSuffix(":"), line.count <= 60, !isItem(line) else { return nil }
        let title = String(line.dropLast()).trimmingCharacters(in: .whitespaces)
        guard !title.isEmpty, !title.contains(": ") else { return nil }
        return title.replacingOccurrences(of: "**", with: "")
    }

    private static func isItem(_ line: String) -> Bool {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        if trimmed.hasPrefix("- ") || trimmed.hasPrefix("* ") || trimmed.hasPrefix("+ ") { return true }
        let digits = trimmed.prefix { $0.isNumber }
        guard !digits.isEmpty, digits.count <= 3 else { return false }
        let rest = trimmed.dropFirst(digits.count)
        return rest.hasPrefix(". ") || rest.hasPrefix(") ")
    }

    private static func kind(_ title: String) -> Section.Kind {
        let title = title.lowercased()
        let words: [(Section.Kind, [String])] = [
            (.checks, ["accept", "done when", "verif", "test", "check", "criteria"]),
            (.steps, ["step", "plan", "todo", "to do", "task", "change"]),
            (.reading, ["read", "context", "file", "doc", "background", "reference"]),
        ]
        return words.first { $0.1.contains(where: title.contains) }?.0 ?? .other
    }
}
