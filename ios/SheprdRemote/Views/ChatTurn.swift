import SheprdRemoteCore
import SwiftUI

/// One turn of a conversation: the prompt as its heading, the steps folded
/// to one line, the answer, then the files it changed. Every turn reads the
/// same, so the conversation scrolls back through its history as a messages
/// app does.
struct ChatTurn: View {
    let entries: ArraySlice<Entry>
    /// Set on the latest turn: whether its prompt has scrolled up past the
    /// top of the chat, so the chat pins it there.
    var promptAbove: Binding<Bool>?
    /// Set when another agent wrote the prompt: it shows as a card.
    var brief: Brief?

    /// A transcript's entries split where each prompt starts; whatever came
    /// before the first prompt is a turn of its own, without a heading.
    static func split(_ entries: [Entry]) -> [ArraySlice<Entry>] {
        var turns: [ArraySlice<Entry>] = []
        var start = entries.startIndex
        for index in entries.indices where index > start {
            if case .user = entries[index].kind {
                turns.append(entries[start ..< index])
                start = index
            }
        }
        if start < entries.endIndex { turns.append(entries[start...]) }
        return turns
    }

    var body: some View {
        let tools = entries.compactMap { if case let .tool(tool) = $0.kind { tool } else { nil } }
        let prompt: (text: String, rest: ArraySlice<Entry>)? = if case let .user(text, _) = entries.first?.kind {
            (text, entries.dropFirst())
        } else {
            nil
        }
        VStack(alignment: .leading, spacing: 16) {
            if let prompt {
                if let promptAbove {
                    heading(prompt.text)
                        .id(PinnedPrompt.anchor)
                        // Below the viewport (reading earlier turns) is not
                        // "above": only a prompt scrolled past pins.
                        .onGeometryChange(for: Bool.self) { $0.frame(in: .scrollView).maxY < 0 } action: { promptAbove.wrappedValue = $0 }
                } else {
                    heading(prompt.text)
                }
            }
            if !tools.isEmpty {
                DisclosureGroup {
                    VStack(alignment: .leading, spacing: 8) {
                        ForEach(tools, id: \.callId) { ToolRow(tool: $0) }
                    }
                    .padding(.top, 6)
                } label: {
                    Label(Self.stepsLine(tools), systemImage: "list.bullet.indent")
                        .font(.subheadline).foregroundStyle(.secondary)
                }
                Divider()
            }
            ForEach(Array(prompt?.rest ?? entries)) { entry in
                if case .tool = entry.kind {} else { EntryView(entry: entry) }
            }
            ForEach(tools.flatMap(Self.diffs), id: \.path) { diff in
                DiffBlock(path: diff.path, old: diff.old, new: diff.new)
            }
        }
    }

    @ViewBuilder
    private func heading(_ text: String) -> some View {
        if let brief {
            BriefCard(text: text, brief: brief)
        } else {
            PromptHeading(text: PastedEnvelope.unwrap(text))
        }
    }

    static func stepsLine(_ tools: [Tool]) -> String {
        let edited = Set(tools.filter { $0.kind == .edit }.flatMap(\.locations)).count
        let steps = "\(tools.count) step\(tools.count == 1 ? "" : "s")"
        return edited > 0 ? "\(steps) · \(edited) file\(edited == 1 ? "" : "s") changed" : steps
    }

    private static func diffs(_ tool: Tool) -> [(path: String, old: String?, new: String)] {
        tool.content.compactMap { if case let .diff(path, old, new) = $0 { (path, old, new) } else { nil } }
    }
}

/// A prompt as its turn's heading. A picture this app sends into a
/// terminal reaches the agent as the path its computer stored it at; that
/// path shows as a picture mark. Any other path stays as typed.
private struct PromptHeading: View {
    let text: String

    var body: some View {
        let (words, pictures) = PromptPictures.split(text)
        HStack(alignment: .top, spacing: 10) {
            Rectangle().fill(.tint).frame(width: 3)
            VStack(alignment: .leading, spacing: 6) {
                if pictures > 0 {
                    Label(pictures == 1 ? "Picture" : "\(pictures) pictures", systemImage: "photo")
                        .font(.subheadline).foregroundStyle(.secondary)
                }
                if !words.isEmpty {
                    SizedPlain(text: words, style: .title3).font(.title3.weight(.semibold)).textSelection(.enabled)
                }
            }
        }
        .fixedSize(horizontal: false, vertical: true)
    }
}
