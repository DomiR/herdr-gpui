import HerdrRemoteCore
import SwiftUI

/// An agent's mark: its logo, the one Herdr UI and the web app draw
/// (`assets/icons/agent-*.svg`), on its brand's color, or on a plain tile
/// for marks without one.
struct AgentIcon: View {
    var agent: String?
    var size: CGFloat = 28

    private var look: (mark: String, tile: AnyShapeStyle, ink: Color) {
        let name = (agent ?? "").lowercased()
        let plain = AnyShapeStyle(.fill.secondary)
        // Copilot before Pi: "copilot" holds "pi".
        if name.contains("claude") { return ("claude", AnyShapeStyle(Color(red: 0xD9 / 255, green: 0x64 / 255, blue: 0x3A / 255)), .white) }
        if name.contains("codex") { return ("codex", AnyShapeStyle(Color(red: 0x11 / 255, green: 0x11 / 255, blue: 0x11 / 255)), .white) }
        if name.contains("gemini") { return ("gemini", AnyShapeStyle(Color(red: 0x3B / 255, green: 0x7C / 255, blue: 0xF0 / 255)), .white) }
        if name.contains("copilot") { return ("copilot", AnyShapeStyle(Color(red: 0x6E / 255, green: 0x40 / 255, blue: 0xC9 / 255)), .white) }
        if name.contains("opencode") { return ("opencode", plain, .primary) }
        if name.contains("amp") { return ("amp", plain, .primary) }
        if name.contains("pi") { return ("pi", plain, .primary) }
        return ("generic", plain, .primary)
    }

    var body: some View {
        let look = look
        Image("Agents/\(look.mark)")
            .resizable()
            .renderingMode(.template)
            .scaledToFit()
            .foregroundStyle(look.ink)
            .frame(width: size * 0.62, height: size * 0.62)
            .frame(width: size, height: size)
            .background(look.tile, in: .rect(cornerRadius: size * 0.28))
            .accessibilityLabel(agent ?? "Agent")
    }
}

/// The status a row shows after its title.
struct StatusMark: View {
    var status: Home.AgentStatus
    var asking = false

    var body: some View {
        Group {
            if asking || status == .blocked {
                Circle().fill(.orange).frame(width: 9, height: 9).accessibilityLabel("Needs you")
            } else {
                switch status {
                case .working:
                    Image(systemName: "ellipsis").foregroundStyle(.secondary).symbolEffect(.variableColor.iterative)
                        .accessibilityLabel("Working")
                case .done:
                    Image(systemName: "checkmark").font(.caption2.bold()).foregroundStyle(.green).accessibilityLabel("Done")
                default:
                    EmptyView()
                }
            }
        }
    }
}

struct ChatRowLabel: View {
    var item: ChatItem
    var showsComputer = true

    var body: some View {
        // The timer and status share the title's line, so the detail
        // line below runs the full width.
        HStack(spacing: 12) {
            AgentIcon(agent: item.chat.agent)
            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(item.chat.title).lineLimit(1)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if item.working, let started = item.chat.summary?.turnStartedMs {
                        Text(timerInterval: Date(timeIntervalSince1970: Double(started) / 1000)...Date.distantFuture, countsDown: false)
                            .font(.footnote.monospacedDigit()).foregroundStyle(.secondary)
                            .fixedSize()
                    }
                    StatusMark(status: item.chat.status, asking: item.question != nil)
                }
                Text(detail).font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }

    private var detail: String {
        let place = showsComputer ? "\(item.computer.name) · \(item.workspace.label)" : item.workspace.label
        if let question = item.question { return "\(place) · \(question.title)" }
        if let text = item.chat.summary?.lastText { return "\(place) · \(text)" }
        return place
    }
}

/// A question with its options as buttons, answered in place.
struct AnswerCard: View {
    @Environment(AppModel.self) private var model
    var item: ChatItem
    var question: Permission
    @State private var sending = false
    @State private var problem: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                AgentIcon(agent: item.chat.agent, size: 24)
                VStack(alignment: .leading, spacing: 0) {
                    Text(item.chat.title).font(.subheadline.weight(.semibold)).lineLimit(1)
                    Text("\(item.computer.name) · \(item.workspace.label)").font(.caption).foregroundStyle(.secondary)
                }
            }
            Text(question.title)
            // Permissions fit on one row; a question's answers get a row each.
            let compact = question.options.allSatisfy { $0.kind != .other } && question.options.count <= 4
            if compact {
                HStack {
                    ForEach(question.options) { option in
                        if option.kind == .allowOnce {
                            Button(option.name) { answer(option) }.buttonStyle(.glassProminent).tint(.orange).lineLimit(1)
                        } else {
                            Button(option.name) { answer(option) }.buttonStyle(.glass).lineLimit(1)
                        }
                    }
                }
            } else {
                ForEach(question.options) { option in
                    Button { answer(option) } label: { Text(option.name).frame(maxWidth: .infinity) }
                        .buttonStyle(.glass)
                }
            }
            if let problem { Text(problem).font(.footnote).foregroundStyle(.red) }
        }
        .disabled(sending)
        .padding(14)
        .background(.background.secondary, in: .rect(cornerRadius: 22))
        .overlay(RoundedRectangle(cornerRadius: 22).strokeBorder(.orange.opacity(0.35)))
    }

    private func answer(_ option: PermissionOption) {
        sending = true
        Task {
            guard await OwnerCheck.allows(option) else {
                sending = false
                return
            }
            problem = await model.fleet.send(.answer(permission: question.id, option: option.id), to: item.key)
            sending = false
        }
    }
}

extension Home.AgentStatus {
    var label: String {
        switch self {
        case .unknown: "Unknown"
        case .idle: "Idle"
        case .working: "Working"
        case .blocked: "Needs you"
        case .done: "Done"
        }
    }
}
