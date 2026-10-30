import HerdrRemoteCore
import SwiftUI

/// Whether everything this phone sent has come back: green when every
/// chat shows it, orange while some is on its way, red when the computer
/// refused some or it never showed. A tap lists what is outstanding.
struct SyncDot: View {
    @Environment(AppModel.self) private var model
    @State private var showing = false
    /// Whether waiting has lasted long enough to show; a change that is
    /// back within a moment would only flicker the dot.
    @State private var patient = false

    /// How long a command may be out before the dot turns orange.
    static let grace: Duration = .milliseconds(300)

    var body: some View {
        let state = model.fleet.pending.state
        let waiting = if case .waiting = state { true } else { false }
        Button { showing = true } label: {
            Circle()
                .fill(color(state))
                .frame(width: 8, height: 8)
                // Narrow, so it stays clear of a back button beside it.
                .frame(width: 16, height: 24)
                .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label(state))
        .animation(.easeInOut(duration: 0.2), value: color(state))
        .task(id: waiting) {
            patient = false
            guard waiting else { return }
            try? await Task.sleep(for: Self.grace)
            patient = !Task.isCancelled
        }
        .popover(isPresented: $showing) {
            SyncList()
                .presentationCompactAdaptation(.popover)
        }
    }

    private func color(_ state: SyncState) -> Color {
        switch state {
        case .synced: .green
        case .waiting: patient ? .orange : .green
        case .failed: .red
        }
    }

    private func label(_ state: SyncState) -> String {
        switch state {
        case .synced: "Everything you sent is in"
        case let .waiting(count): "\(count) change\(count == 1 ? "" : "s") on the way"
        case let .failed(count): "\(count) change\(count == 1 ? "" : "s") did not go through"
        }
    }
}

/// What the sync dot is waiting for, chat by chat.
private struct SyncList: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let pending = model.fleet.pending
        VStack(alignment: .leading, spacing: 10) {
            if pending.items.isEmpty {
                Label("Everything you sent is in", systemImage: "checkmark.circle.fill")
                    .foregroundStyle(.green)
            }
            ForEach(pending.items) { item in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Image(systemName: item.failed ? "exclamationmark.circle.fill" : "clock")
                        .foregroundStyle(item.failed ? .red : .orange)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("\(item.expected?.label ?? "Stop") · \(title(item.key))").font(.subheadline.weight(.medium))
                        Text(detail(item)).font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            if pending.items.contains(where: \.failed) {
                Button("Dismiss") { pending.dismissFailures() }.buttonStyle(.glass).controlSize(.small)
            }
        }
        .padding(16)
        .frame(minWidth: 260, alignment: .leading)
    }

    private func title(_ key: PaneKey) -> String {
        model.fleet.chatItems.first { $0.key == key }?.chat.title ?? "Chat"
    }

    private func detail(_ item: PendingCommand) -> String {
        switch item.state {
        case .sending: "Sending to the computer…"
        case .sent: "Waiting for the chat to show it…"
        case let .failed(reason): "Undone: \(reason)"
        }
    }
}
