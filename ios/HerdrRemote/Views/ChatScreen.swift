import HerdrRemoteCore
import SwiftUI
import UIKit

/// Chat B: the latest turn as a document (the prompt as a heading, steps
/// folded to one line, the answer with its diffs), the running step
/// pinned under the header, earlier turns summed up, and questions in a
/// bottom sheet.
struct ChatScreen: View {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    let title: String
    @State private var store: ChatStore?

    var body: some View {
        Group {
            if let store, let transcript = store.transcript {
                ChatDocument(key: key, store: store, transcript: transcript)
            } else {
                ProgressView("Loading the chat…")
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.hidden, for: .tabBar)
        .toolbar {
            ToolbarItem(placement: .principal) {
                VStack(spacing: 0) {
                    Text(title).font(.headline).lineLimit(1)
                    if let line = store?.transcript.map(sessionLine), !line.isEmpty {
                        Text(line).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                    }
                }
            }
            if let usage = store?.transcript?.usage {
                ToolbarItem(placement: .topBarTrailing) { ContextRing(percent: usage.percent) }
            }
        }
        .onAppear { store = model.fleet.openChat(key) }
        .onDisappear {
            if let store { model.fleet.closeChat(store) }
        }
    }

    /// "Opus 5.5 · High · Bypass permissions", from the session's settings.
    private func sessionLine(_ transcript: Transcript) -> String {
        let model = transcript.options.first { $0.category == .model }.map(currentName)
        let effort = transcript.options.first { $0.category == .thoughtLevel }.map(currentName)
        let mode = transcript.modes.flatMap { modes in modes.available.first { $0.id == modes.current }?.name }
        return [model, effort, mode].compactMap { $0 }.joined(separator: " · ")
    }

    private func currentName(_ option: ConfigOption) -> String {
        switch option.kind {
        case let .select(choices, current): choices.first { $0.id == current }?.name ?? current
        case let .toggle(current): current ? "On" : "Off"
        }
    }
}

struct ContextRing: View {
    var percent: Int

    var body: some View {
        ZStack {
            Circle().stroke(.quaternary, lineWidth: 3)
            Circle().trim(from: 0, to: Double(percent) / 100)
                .stroke(percent > 80 ? AnyShapeStyle(.orange) : AnyShapeStyle(.tint), style: StrokeStyle(lineWidth: 3, lineCap: .round))
                .rotationEffect(.degrees(-90))
        }
        .frame(width: 20, height: 20)
        .accessibilityLabel("Context \(percent) percent full")
    }
}

private struct ChatDocument: View {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    let store: ChatStore
    let transcript: Transcript
    @State private var draft = ""
    @State private var problem: String?
    @State private var dismissedQuestion: PermissionId?
    /// A link the agent wrote, shown in full before it opens.
    @State private var pendingLink: URL?

    var body: some View {
        let entries = transcript.entries
        let start = entries.lastIndex { if case .user = $0.kind { true } else { false } } ?? 0
        let earlier = entries[..<start]
        let latest = entries[start...]
        let tools = latest.compactMap { if case let .tool(tool) = $0.kind { tool } else { nil } }
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                if !earlier.isEmpty || transcript.truncated {
                    DisclosureGroup {
                        LazyVStack(alignment: .leading, spacing: 12) {
                            ForEach(Array(earlier)) { EntryView(entry: $0) }
                        }
                    } label: {
                        let turns = earlier.filter { if case .user = $0.kind { true } else { false } }.count
                        Text("Earlier in this chat · \(turns) turn\(turns == 1 ? "" : "s")")
                            .font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
                    }
                }
                if case let .user(text, _) = latest.first?.kind {
                    HStack(alignment: .top, spacing: 10) {
                        Rectangle().fill(.tint).frame(width: 3)
                        Text(text).font(.title3.weight(.semibold)).textSelection(.enabled)
                    }
                    .fixedSize(horizontal: false, vertical: true)
                }
                if !tools.isEmpty {
                    DisclosureGroup {
                        VStack(alignment: .leading, spacing: 8) {
                            ForEach(tools, id: \.callId) { ToolRow(tool: $0) }
                        }
                        .padding(.top, 6)
                    } label: {
                        Label(stepsLine(tools), systemImage: "list.bullet.indent")
                            .font(.subheadline).foregroundStyle(.secondary)
                    }
                    Divider()
                }
                ForEach(Array(latest.dropFirst())) { entry in
                    if case .tool = entry.kind {} else { EntryView(entry: entry) }
                }
                ForEach(tools.flatMap(diffs), id: \.path) { diff in
                    DiffBlock(path: diff.path, old: diff.old, new: diff.new)
                }
                ForEach(transcript.queued, id: \.id) { queued in
                    Label("Queued: \(queued.text)", systemImage: "clock").font(.footnote).foregroundStyle(.secondary)
                }
                if let problem { Text(problem).font(.footnote).foregroundStyle(.red) }
            }
            .padding(.horizontal, 20)
            .padding(.top, transcript.status == .working ? 56 : 16)
            .padding(.bottom, 24)
        }
        .defaultScrollAnchor(.bottom)
        .opacity(store.stale ? 0.6 : 1)
        .overlay(alignment: .top) {
            if transcript.status == .working { RunningCapsule(transcript: transcript, tools: tools) }
        }
        .safeAreaInset(edge: .bottom) { composer }
        .environment(\.openURL, OpenURLAction { url in
            guard EntryView.webSchemes.contains(url.scheme?.lowercased() ?? "") else { return .discarded }
            pendingLink = url
            return .handled
        })
        .confirmationDialog(
            "Open this link?",
            isPresented: Binding(get: { pendingLink != nil }, set: { if !$0 { pendingLink = nil } }),
            titleVisibility: .visible,
            presenting: pendingLink
        ) { url in
            Button("Open in Safari") { UIApplication.shared.open(url) }
        } message: { url in
            Text(url.absoluteString)
        }
        .sheet(item: question) { permission in
            QuestionSheet(key: key, permission: permission)
                .presentationDetents([.medium, .large])
        }
    }

    /// The oldest question, shown until answered or swiped away.
    private var question: Binding<Permission?> {
        Binding(
            get: { transcript.permissions.first { $0.id != dismissedQuestion } },
            set: { if $0 == nil { dismissedQuestion = transcript.permissions.first?.id } }
        )
    }

    private func stepsLine(_ tools: [Tool]) -> String {
        let edited = Set(tools.filter { $0.kind == .edit }.flatMap(\.locations)).count
        let steps = "\(tools.count) step\(tools.count == 1 ? "" : "s")"
        return edited > 0 ? "\(steps) · \(edited) file\(edited == 1 ? "" : "s") changed" : steps
    }

    private func diffs(_ tool: Tool) -> [(path: String, old: String?, new: String)] {
        tool.content.compactMap { if case let .diff(path, old, new) = $0 { (path, old, new) } else { nil } }
    }

    private var composer: some View {
        HStack(alignment: .bottom, spacing: 10) {
            TextField("Message \(transcript.agent)", text: $draft, axis: .vertical)
                .lineLimit(1...6)
                .padding(.horizontal, 16).padding(.vertical, 11)
                .glassEffect(.regular, in: .rect(cornerRadius: 22))
            let stopping = draft.isEmpty && transcript.status == .working
            Button {
                send(stopping ? .cancel : .prompt(text: draft))
            } label: {
                Image(systemName: stopping ? "stop.fill" : "arrow.up").font(.body.bold()).frame(width: 36, height: 36)
            }
            .buttonStyle(.glassProminent)
            .buttonBorderShape(.circle)
            .disabled(transcript.readOnly || (draft.isEmpty && !stopping))
            .accessibilityLabel(stopping ? "Stop" : "Send")
        }
        .padding(.horizontal, 16).padding(.bottom, 6)
    }

    private func send(_ command: ChatCommand) {
        let sent = draft
        if case .prompt = command { draft = "" }
        Task {
            problem = await model.fleet.send(command, to: key)
            if problem != nil, case .prompt = command { draft = sent }
        }
    }
}

/// What the agent is doing now, pinned under the header.
private struct RunningCapsule: View {
    let transcript: Transcript
    let tools: [Tool]

    var body: some View {
        let current = tools.last { $0.status == .inProgress } ?? tools.last { $0.status == .pending }
        HStack(spacing: 8) {
            ProgressView().controlSize(.small)
            Text(current?.title ?? "Thinking").font(.subheadline.weight(.medium)).lineLimit(1)
            Spacer()
            if let started = transcript.turn?.startedMs {
                Text(timerInterval: Date(timeIntervalSince1970: Double(started) / 1000)...Date.distantFuture, countsDown: false)
                    .font(.subheadline.monospacedDigit()).foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 9)
        .glassEffect(.regular, in: .capsule)
        .padding(.horizontal, 16)
    }
}

/// Questions A: a bottom sheet in thumb reach, full-width options, the
/// "always" ones set apart.
private struct QuestionSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let key: PaneKey
    let permission: Permission
    @State private var problem: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(permission.title).font(.title3.weight(.semibold))
            let always = permission.options.filter { $0.kind == .allowAlways || $0.kind == .rejectAlways }
            ForEach(permission.options.filter { !always.contains($0) }) { option in
                if option.kind == .allowOnce {
                    Button { answer(option) } label: { Text(option.name).frame(maxWidth: .infinity) }
                        .buttonStyle(.glassProminent).controlSize(.large).tint(.orange)
                } else {
                    Button { answer(option) } label: { Text(option.name).frame(maxWidth: .infinity) }
                        .buttonStyle(.glass).controlSize(.large)
                }
            }
            if !always.isEmpty {
                Divider()
                ForEach(always) { option in
                    Button(option.name) { answer(option) }.font(.subheadline)
                }
            }
            if let problem { Text(problem).font(.footnote).foregroundStyle(.red) }
        }
        .padding(24)
    }

    private func answer(_ option: PermissionOption) {
        Task {
            problem = await model.fleet.send(.answer(permission: permission.id, option: option.id), to: key)
            if problem == nil { dismiss() }
        }
    }
}
