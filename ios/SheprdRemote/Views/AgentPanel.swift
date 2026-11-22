import SheprdRemoteCore
import SwiftUI

/// The desktop chat rail on a phone: what runs beside the conversation
/// (sub-agents, background shells, monitors), the agent's plan, and the
/// files it changed, and its pull requests, opened from the chat's header. Sections stay away
/// when the agent reports nothing; nothing here guesses from the chat.
struct AgentPanel: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let key: PaneKey
    let store: ChatStore
    /// Activities a Stop was sent for, until they end or `stopWait`
    /// passes without them ending.
    @State private var stopping: Set<String> = []
    @State private var problem: String?
    @State private var folded: Set<PanelSection> = []

    var body: some View {
        NavigationStack {
            Group {
                if let transcript = store.transcript {
                    content(transcript)
                } else {
                    ProgressView()
                }
            }
            .navigationTitle("Activity")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { dismiss() } label: { Image(systemName: "xmark") }
                        .accessibilityLabel("Close")
                }
            }
        }
    }

    private func content(_ transcript: Transcript) -> some View {
        let sections = PanelSection.allCases.filter { $0.shows(in: transcript) }
        return ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    if sections.isEmpty {
                        ContentUnavailableView(
                            "Nothing running",
                            systemImage: "list.bullet.clipboard",
                            description: Text("Sub-agents, shells, the plan, changed files, and pull requests show here when the agent reports them.")
                        )
                        .padding(.top, 40)
                    } else {
                        Overview(transcript: transcript, sections: sections) { section in
                            folded.remove(section)
                            withAnimation(.snappy) { proxy.scrollTo(section, anchor: .top) }
                        }
                    }
                    ForEach(sections) { section in
                        sectionView(section, transcript: transcript).id(section)
                    }
                    if let problem {
                        Label(problem, systemImage: "exclamationmark.triangle.fill")
                            .font(.footnote).foregroundStyle(.red)
                    }
                }
                .frame(maxWidth: 680, alignment: .leading)
                .frame(maxWidth: .infinity)
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            }
            .background(Color(.systemGroupedBackground))
            #if DEBUG
            // The simulator loop's `-agentPanelScroll plan|changed` scrolls
            // to that section.
            .task {
                let name = UserDefaults.standard.string(forKey: "agentPanelScroll")
                guard let section = PanelSection.allCases.first(where: { "\($0)" == name }) else { return }
                try? await Task.sleep(for: .milliseconds(600))
                proxy.scrollTo(section, anchor: .top)
            }
            #endif
            .animation(.snappy, value: transcript.activities)
            .animation(.snappy, value: transcript.plan)
        }
        .onChange(of: transcript.activities) {
            stopping = stopping.filter { id in transcript.activities.contains { $0.callId == id && $0.isRunning } }
        }
    }

    private func sectionView(_ section: PanelSection, transcript: Transcript) -> some View {
        let open = !folded.contains(section)
        return VStack(alignment: .leading, spacing: 8) {
            Button {
                withAnimation(.snappy) {
                    if open { folded.insert(section) } else { folded.remove(section) }
                }
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: section.symbol).foregroundStyle(section.tint).frame(width: 22)
                    Text(section.title).font(.headline)
                    Spacer()
                    Text(section.summary(of: transcript)).font(.subheadline).foregroundStyle(.secondary).monospacedDigit()
                    Image(systemName: "chevron.right")
                        .font(.footnote.weight(.semibold)).foregroundStyle(.tertiary)
                        .rotationEffect(.degrees(open ? 90 : 0))
                }
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityValue(open ? "Shown" : "Folded")
            if open {
                Group {
                    switch section {
                    case .subagents, .shells, .monitors:
                        if let kind = section.kind {
                            ActivityList(
                                group: transcript.activityGroup(kind),
                                canStop: !transcript.readOnly,
                                stopping: stopping,
                                stop: stop
                            )
                        }
                    case .plan: PlanList(plan: transcript.plan)
                    case .changed:
                        VStack(spacing: 8) {
                            ForEach(transcript.changedFiles) { DiffBlock(path: $0.path, old: $0.old, new: $0.new) }
                        }
                    case .pullRequests: ChatPullRequestList(pulls: transcript.pullRequests)
                    }
                }
                .transition(.opacity.combined(with: .move(edge: .top)))
            }
        }
    }

    /// How long a Stop the chat accepted may take to end its activity.
    /// The chat accepting the command is not the agent stopping it: an
    /// agent that refuses says so only in a notice, so after this the
    /// button comes back and the person can try again.
    static let stopWait = Duration.seconds(10)

    private func stop(_ activity: AgentActivity) {
        let id = activity.callId
        stopping.insert(id)
        Task {
            problem = await model.fleet.send(.stopActivity(callId: id), to: key)
            if problem == nil { try? await Task.sleep(for: Self.stopWait) }
            stopping.remove(id)
        }
    }
}

/// The panel's sections, in the desktop rail's order.
enum PanelSection: CaseIterable, Identifiable, Hashable {
    case subagents, shells, monitors, plan, changed, pullRequests

    var id: Self { self }

    var kind: AgentActivityKind? {
        switch self {
        case .subagents: .subagent
        case .shells: .shell
        case .monitors: .monitor
        case .plan, .changed, .pullRequests: nil
        }
    }

    var title: String {
        switch self {
        case .subagents: "Sub-agents"
        case .shells: "Shells"
        case .monitors: "Monitors"
        case .plan: "Plan"
        case .changed: "Changed"
        case .pullRequests: "Pull requests"
        }
    }

    var symbol: String {
        switch self {
        case .subagents: "person.2.fill"
        case .shells: "apple.terminal.fill"
        case .monitors: "waveform.path.ecg"
        case .plan: "checklist"
        case .changed: "doc.on.doc.fill"
        case .pullRequests: "arrow.triangle.pull"
        }
    }

    var tint: Color {
        switch self {
        case .subagents: .purple
        case .shells: .teal
        case .monitors: .pink
        case .plan: .orange
        case .changed: .blue
        case .pullRequests: .green
        }
    }

    func shows(in transcript: Transcript) -> Bool {
        switch self {
        case .subagents, .shells, .monitors: kind.map { !transcript.activityGroup($0).isEmpty } ?? false
        case .plan: !transcript.plan.isEmpty
        case .changed: !transcript.changedFiles.isEmpty
        case .pullRequests: !transcript.pullRequests.isEmpty
        }
    }

    func summary(of transcript: Transcript) -> String {
        switch self {
        case .subagents, .shells, .monitors:
            guard let kind else { return "" }
            let group = transcript.activityGroup(kind)
            return group.running.isEmpty ? "\(group.ended.count) ended" : "\(group.running.count) running"
        case .plan:
            return "\(transcript.plan.count { $0.status == .completed }) of \(transcript.plan.count)"
        case .changed:
            let count = transcript.changedFiles.count
            return "\(count) file\(count == 1 ? "" : "s")"
        case .pullRequests:
            let count = transcript.pullRequests.count
            return "\(count) pull request\(count == 1 ? "" : "s")"
        }
    }
}

/// One tile per section at the top: how much runs, the plan's progress,
/// and the files changed, each a tap from its section.
private struct Overview: View {
    let transcript: Transcript
    let sections: [PanelSection]
    let open: (PanelSection) -> Void

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 10) {
                ForEach(sections) { section in
                    Button { open(section) } label: { tile(section) }
                        .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 16)
        }
        .padding(.horizontal, -16)
        .scrollClipDisabled()
    }

    private func tile(_ section: PanelSection) -> some View {
        let (value, caption, live) = figures(section)
        return VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: section.symbol)
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(section.tint)
                    .symbolEffect(.pulse, isActive: live)
                Text(section.title).font(.caption.weight(.semibold)).foregroundStyle(.secondary)
            }
            Text(value).font(.title2.weight(.semibold)).monospacedDigit().contentTransition(.numericText())
            if section == .plan {
                ProgressView(value: planDone, total: Double(max(transcript.plan.count, 1)))
                    .tint(section.tint)
            } else {
                Text(caption).font(.caption).foregroundStyle(.secondary)
            }
        }
        .frame(width: 118, alignment: .leading)
        .padding(12)
        .background(section.tint.opacity(0.10), in: .rect(cornerRadius: 16))
        .background(Color(.secondarySystemGroupedBackground), in: .rect(cornerRadius: 16))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(section.tint.opacity(live ? 0.35 : 0.12)))
        .accessibilityElement(children: .combine)
        .accessibilityHint("Shows the section")
    }

    private var planDone: Double { Double(transcript.plan.count { $0.status == .completed }) }

    private func figures(_ section: PanelSection) -> (String, String, Bool) {
        switch section {
        case .subagents, .shells, .monitors:
            guard let kind = section.kind else { return ("", "", false) }
            let group = transcript.activityGroup(kind)
            return group.running.isEmpty
                ? ("\(group.ended.count)", "ended", false)
                : ("\(group.running.count)", "running", true)
        case .plan:
            return ("\(Int(planDone))/\(transcript.plan.count)", "", transcript.plan.contains { $0.status == .inProgress })
        case .changed:
            let count = transcript.changedFiles.count
            return ("\(count)", count == 1 ? "file" : "files", false)
        case .pullRequests:
            let count = transcript.pullRequests.count
            return ("\(count)", count == 1 ? "pull request" : "pull requests", false)
        }
    }
}

/// A section's activities: running ones as cards, what ended folded under
/// them.
private struct ActivityList: View {
    let group: AgentActivityGroup
    let canStop: Bool
    let stopping: Set<String>
    let stop: (AgentActivity) -> Void
    @State private var showsEnded = false

    /// Ended rows listed at most; the rest are counted.
    private static let endedRows = 24

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(group.running) { activity in
                RunningRow(
                    activity: activity,
                    canStop: canStop && activity.stoppable,
                    stopping: stopping.contains(activity.callId)
                ) { stop(activity) }
                .transition(.opacity.combined(with: .scale(scale: 0.97)))
            }
            if !group.ended.isEmpty {
                VStack(alignment: .leading, spacing: 0) {
                    Button {
                        withAnimation(.snappy) { showsEnded.toggle() }
                    } label: {
                        HStack {
                            Text("\(group.ended.count) ended").font(.subheadline.weight(.medium))
                            Spacer()
                            Image(systemName: "chevron.right")
                                .font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
                                .rotationEffect(.degrees(showsEnded ? 90 : 0))
                        }
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, 14).padding(.vertical, 11)
                        .contentShape(.rect)
                    }
                    .buttonStyle(.plain)
                    if showsEnded {
                        ForEach(group.ended.prefix(Self.endedRows)) { activity in
                            Divider().padding(.leading, 14)
                            EndedRow(activity: activity)
                        }
                        if group.ended.count > Self.endedRows {
                            Divider().padding(.leading, 14)
                            Text("\(group.ended.count - Self.endedRows) more")
                                .font(.caption).foregroundStyle(.secondary).padding(14)
                        }
                    }
                }
                .background(Color(.secondarySystemGroupedBackground), in: .rect(cornerRadius: 14))
            }
        }
    }
}

private struct RunningRow: View {
    let activity: AgentActivity
    let canStop: Bool
    let stopping: Bool
    let stop: () -> Void

    var body: some View {
        let section = PanelSection.allCases.first { $0.kind == activity.kind.section } ?? .shells
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: section.symbol)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(section.tint)
                .symbolEffect(.pulse)
                .frame(width: 34, height: 34)
                .background(section.tint.opacity(0.15), in: .rect(cornerRadius: 10))
            VStack(alignment: .leading, spacing: 4) {
                Text(activity.label)
                    .font(activity.kind.section == .shell ? .subheadline.monospaced() : .subheadline.weight(.semibold))
                    .lineLimit(3)
                if let detail = activity.detail, !detail.isEmpty {
                    Text(detail).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                }
                Text(timerInterval: Date(milliseconds: activity.startedMs)...Date.distantFuture, countsDown: false)
                    .font(.caption.monospacedDigit()).foregroundStyle(.tertiary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if canStop {
                Button(action: stop) {
                    if stopping {
                        ProgressView().controlSize(.small).frame(width: 30, height: 30)
                    } else {
                        Image(systemName: "stop.fill")
                            .font(.caption.weight(.bold))
                            .foregroundStyle(.red)
                            .frame(width: 30, height: 30)
                            .background(.red.opacity(0.14), in: .circle)
                    }
                }
                .buttonStyle(.plain)
                .disabled(stopping)
                .accessibilityLabel("Stop")
            }
        }
        .padding(12)
        .background(Color(.secondarySystemGroupedBackground), in: .rect(cornerRadius: 14))
        .contextMenu {
            Button { UIPasteboard.general.string = activity.label } label: { Label("Copy", systemImage: "doc.on.doc") }
            if canStop {
                Button(role: .destructive, action: stop) { Label("Stop", systemImage: "stop.fill") }
            }
        }
        .accessibilityElement(children: .combine)
    }
}

private struct EndedRow: View {
    let activity: AgentActivity

    var body: some View {
        let (symbol, tint, word): (String, Color, String) = switch activity.state {
        case .failed: ("xmark.circle.fill", .red, "Failed")
        case .killed: ("stop.circle.fill", .secondary, "Stopped")
        case .running, .finished: ("checkmark.circle.fill", .green, "Done")
        }
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: symbol).foregroundStyle(tint).accessibilityLabel(word)
            Text(activity.label)
                .font(activity.kind.section == .shell ? .footnote.monospaced() : .footnote)
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
            if let finished = activity.finishedMs {
                Text(Duration.milliseconds(Int64(finished.saturatingSubtract(activity.startedMs))).formatted(.units(allowed: [.hours, .minutes, .seconds], width: .narrow)))
                    .font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 14).padding(.vertical, 10)
    }
}

private struct PlanList: View {
    let plan: [PlanEntry]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(plan.enumerated()), id: \.offset) { index, step in
                if index > 0 { Divider().padding(.leading, 42) }
                HStack(alignment: .firstTextBaseline, spacing: 10) {
                    icon(step.status).frame(width: 20)
                    Text(step.content)
                        .font(step.status == .inProgress ? .subheadline.weight(.semibold) : .subheadline)
                        .foregroundStyle(step.status == .completed ? .secondary : .primary)
                        .strikethrough(step.status == .completed, color: .secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .padding(.horizontal, 12).padding(.vertical, 10)
            }
        }
        .background(Color(.secondarySystemGroupedBackground), in: .rect(cornerRadius: 14))
    }

    @ViewBuilder
    private func icon(_ status: PlanStatus) -> some View {
        switch status {
        case .completed: Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
        case .inProgress:
            Image(systemName: "circle.dotted.circle").foregroundStyle(.orange)
                .symbolEffect(.rotate, options: .repeat(.continuous))
        case .pending: Image(systemName: "circle").foregroundStyle(.tertiary)
        }
    }
}

/// The chat header's button for the panel: hidden until the agent reports
/// something, and counting what runs.
struct AgentPanelButton: View {
    let transcript: Transcript
    let open: () -> Void

    var body: some View {
        let running = transcript.runningActivities
        Button(action: open) {
            HStack(spacing: 3) {
                Image(systemName: "list.bullet.clipboard")
                    .symbolEffect(.pulse, isActive: running > 0)
                if running > 0 {
                    Text("\(running)").font(.subheadline.weight(.semibold)).monospacedDigit()
                        .contentTransition(.numericText())
                }
            }
        }
        .accessibilityLabel("Activity")
        .accessibilityValue(running > 0 ? "\(running) running" : "")
        .accessibilityHint("Shows sub-agents, shells, the plan, changed files, and pull requests")
    }
}

private extension Date {
    init(milliseconds: UInt64) { self.init(timeIntervalSince1970: Double(milliseconds) / 1000) }
}

private extension UInt64 {
    func saturatingSubtract(_ other: UInt64) -> UInt64 { self > other ? self - other : 0 }
}
