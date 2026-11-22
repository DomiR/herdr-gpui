import ActivityKit
import SheprdRemoteCore
import SwiftUI
import WidgetKit

/// One running chat on the lock screen and in the Dynamic Island. Tapping
/// anywhere opens `herdr-remote://chat?...`, which the app turns into the
/// chat's page in the web app. Content a computer sealed for the push relay
/// is opened here with this phone's notify key; when it does not open, the
/// activity is the fixed `SealedCover`, with nothing from the push.
struct ChatLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: ChatActivityAttributes.self) { context in
            if let opened = context.attributes.opened(context.state) {
                LockScreenView(facts: opened.facts, state: opened.state)
                    .activityBackgroundTint(Color.black.opacity(0.55))
                    .activitySystemActionForegroundColor(.white)
                    .widgetURL(opened.facts.link.url)
            } else {
                SealedCover()
                    .activityBackgroundTint(Color.black.opacity(0.55))
                    .activitySystemActionForegroundColor(.white)
            }
        } dynamicIsland: { context in
            guard let opened = context.attributes.opened(context.state) else {
                return SealedCover.island
            }
            let state = opened.state
            let facts = opened.facts
            return DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    AgentBadge(agent: facts.agent, size: 28)
                        .padding(.leading, 4)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    if state.phase == .working {
                        Elapsed(since: state.startedAt)
                            .font(.caption.weight(.semibold))
                            .frame(maxWidth: 56, alignment: .trailing)
                    } else {
                        PhaseMark(phase: state.phase)
                    }
                }
                DynamicIslandExpandedRegion(.center) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(title(state, facts))
                            .font(.subheadline.weight(.semibold))
                            .lineLimit(1)
                        Text(facts.place)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    ExpandedBottom(facts: facts, state: state)
                }
            } compactLeading: {
                HStack(spacing: 4) {
                    Circle()
                        .fill(Palette.working)
                        .frame(width: 8, height: 8)
                    Text("\(max(state.working, 1))")
                        .font(.caption2.weight(.bold))
                        .monospacedDigit()
                }
            } compactTrailing: {
                if state.phase == .question || state.waiting > 0 {
                    HStack(spacing: 3) {
                        Circle()
                            .fill(Palette.question)
                            .frame(width: 8, height: 8)
                        Text("\(max(state.waiting, 1))")
                            .font(.caption2.weight(.bold))
                            .monospacedDigit()
                    }
                    .foregroundStyle(Palette.question)
                } else {
                    Elapsed(since: state.startedAt)
                        .font(.caption2.weight(.semibold))
                        .frame(maxWidth: 40)
                }
            } minimal: {
                PhaseMark(phase: state.phase, size: 9)
            }
            .widgetURL(facts.link.url)
            .keylineTint(Palette.phase(state.phase))
        }
    }

    private func title(_ state: ChatActivityState, _ facts: ChatActivityFacts) -> String {
        switch state.phase {
        case .working: "\(facts.agent) is working"
        case .question: "\(facts.agent) needs an answer"
        case .finished: "\(facts.agent) finished"
        case .failed: "\(facts.agent) stopped"
        }
    }
}

/// The expanded island's lower half: the question (or step, or summary) in
/// a dark block, and Open.
private struct ExpandedBottom: View {
    let facts: ChatActivityFacts
    let state: ChatActivityState

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            StepStack(state: state)
            Text(text)
                .font(.system(.footnote, design: state.phase == .question ? .monospaced : .default))
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(8)
                .background(.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            AnswerButtons(facts: facts, state: state)
            HStack {
                if state.phase == .working, let plan = state.planLabel {
                    Text(plan)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Link(destination: facts.link.url) {
                    Text("Open")
                        .font(.subheadline.weight(.semibold))
                        .padding(.horizontal, 16)
                        .padding(.vertical, 6)
                        .background(
                            state.phase == .question ? Palette.question : Color.white.opacity(0.18),
                            in: Capsule()
                        )
                        .foregroundStyle(state.phase == .question ? Color.black : Color.white)
                }
            }
        }
    }

    private var text: String {
        switch state.phase {
        case .working: state.step
        case .question: state.question ?? state.step
        case .finished, .failed: state.summary ?? state.step
        }
    }
}
