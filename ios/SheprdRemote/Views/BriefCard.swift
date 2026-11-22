import SheprdRemoteCore
import SwiftUI
import UIKit

/// A prompt another agent wrote, as a card in place of the turn's heading:
/// who sent it, the goal, the sections it lists, and the whole brief in a
/// sheet. A brief is a document to read, not the person's words, so it does
/// not fill the screen as a heading would.
struct BriefCard: View {
    let text: String
    let brief: Brief
    /// The section the sheet opens at; `nil` while it is closed.
    @State private var opened: Opened?

    private struct Opened: Identifiable {
        let line: Int?
        var id: Int { line ?? -1 }
    }

    var body: some View {
        let outline = BriefOutline(text)
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 8) {
                BriefOrigin(brief: brief)
                SizedPlain(text: outline.goal, style: .headline).font(.headline)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(14)
            ForEach(outline.sections) { section in
                Divider().padding(.leading, section.id == outline.sections.first?.id ? 0 : 42)
                Button { opened = Opened(line: section.line) } label: { SectionRow(section: section) }
                    .buttonStyle(.plain)
            }
            Divider()
            Button { opened = Opened(line: nil) } label: {
                HStack {
                    Label("Read the brief", systemImage: "doc.text.magnifyingglass")
                    Spacer()
                    Text("\(outline.words) words").foregroundStyle(.secondary)
                }
                .font(.subheadline.weight(.medium))
                .padding(.horizontal, 14).padding(.vertical, 11)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.tint)
        }
        .background(.fill.quinary, in: .rect(cornerRadius: 16))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(.separator))
        .sheet(item: $opened) { opened in
            BriefSheet(outline: outline, brief: brief, line: opened.line)
                .presentationDetents([.medium, .large])
        }
    }
}

private struct BriefOrigin: View {
    let brief: Brief

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "text.document").foregroundStyle(.tint)
            if let name = brief.fromName {
                Text("Brief from \(name)").font(.subheadline.weight(.semibold))
            } else {
                Text("Brief").font(.subheadline.weight(.semibold))
            }
        }
        .lineLimit(1)
    }
}

private struct SectionRow: View {
    let section: BriefOutline.Section

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: symbol).frame(width: 18).foregroundStyle(.tint)
            Text(section.title).font(.subheadline.weight(.medium)).lineLimit(1)
            Spacer()
            if section.count > 0 {
                Text(section.count, format: .number).font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            }
            Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
        }
        .padding(.horizontal, 14).padding(.vertical, 11)
        .contentShape(.rect)
    }

    private var symbol: String {
        switch section.kind {
        case .reading: "doc.text"
        case .steps: "list.number"
        case .checks: "checkmark.seal"
        case .other: "list.bullet"
        }
    }
}

/// The whole brief, in parts that start at each section so the sheet can
/// open at the one tapped.
private struct BriefSheet: View {
    let outline: BriefOutline
    let brief: Brief
    let line: Int?
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollViewReader { reader in
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        BriefOrigin(brief: brief)
                        ForEach(parts, id: \.line) { part in
                            MarkdownView(text: part.text).id(part.line)
                        }
                    }
                    .padding(20)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .onAppear {
                    if let line { reader.scrollTo(line, anchor: .top) }
                }
            }
            .navigationTitle("Brief")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Copy", systemImage: "doc.on.doc") { UIPasteboard.general.string = outline.text }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Close", systemImage: "xmark") { dismiss() }
                }
            }
        }
    }

    private var parts: [(line: Int, text: String)] {
        let lines = outline.text.components(separatedBy: .newlines)
        let starts = [0] + outline.sections.map(\.line).filter { $0 > 0 && $0 < lines.count }
        return starts.enumerated().map { index, start in
            let end = index + 1 < starts.count ? starts[index + 1] : lines.count
            return (start, lines[start ..< end].joined(separator: "\n"))
        }
        .filter { !$0.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    }
}
