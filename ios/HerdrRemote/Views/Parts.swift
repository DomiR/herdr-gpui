import HerdrRemoteCore
import SwiftUI

/// An agent's mark: a rounded tile in its color with a symbol.
struct AgentIcon: View {
    var agent: String?
    var size: CGFloat = 28

    private var look: (symbol: String, tint: Color) {
        let name = (agent ?? "").lowercased()
        if name.contains("claude") { return ("sparkle", Color(red: 0.85, green: 0.47, blue: 0.34)) }
        if name.contains("codex") { return ("hexagon.fill", Color(white: 0.5)) }
        if name.contains("gemini") { return ("diamond.fill", Color(red: 0.36, green: 0.52, blue: 0.96)) }
        if name.contains("opencode") { return ("chevron.left.forwardslash.chevron.right", .teal) }
        if name.contains("amp") { return ("bolt.fill", .pink) }
        if name.contains("copilot") { return ("airplane", Color(red: 0.42, green: 0.36, blue: 0.9)) }
        if name == "pi" || name.hasPrefix("pi ") { return ("p.circle.fill", .green) }
        return ("sparkles", .indigo)
    }

    var body: some View {
        Image(systemName: look.symbol)
            .font(.system(size: size * 0.5, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(look.tint.gradient, in: .rect(cornerRadius: size * 0.28))
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
        HStack(spacing: 12) {
            AgentIcon(agent: item.chat.agent)
            VStack(alignment: .leading, spacing: 2) {
                Text(item.chat.title).lineLimit(1)
                Text(detail).font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer(minLength: 8)
            if item.working, let started = item.chat.summary?.turnStartedMs {
                Text(timerInterval: Date(timeIntervalSince1970: Double(started) / 1000)...Date.distantFuture, countsDown: false)
                    .font(.footnote.monospacedDigit()).foregroundStyle(.secondary).frame(maxWidth: 56, alignment: .trailing)
            }
            StatusMark(status: item.chat.status, asking: item.question != nil)
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
