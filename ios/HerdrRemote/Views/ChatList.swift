import HerdrRemoteCore
import SwiftUI

/// Every chat across computers grouped by state: what needs you, answered
/// in place; what is working; the rest. Home shows it.
struct ChatList: View {
    @Environment(AppModel.self) private var model
    /// The chat a long press asked to close, waiting for confirmation.
    @State private var closing: ChatItem?
    @State private var problem: String?

    var body: some View {
        let items = model.fleet.chatItems
        List {
            section("Needs you", items.filter(\.needsYou))
            section("Working", items.filter(\.working))
            section("Recent", items.filter { !$0.needsYou && !$0.working })
        }
        .refreshable { model.fleet.networkChanged() }
        .confirmationDialog(
            "Close \(closing?.chat.title ?? "this chat")?",
            isPresented: Binding(get: { closing != nil }, set: { if !$0 { closing = nil } }),
            titleVisibility: .visible,
            presenting: closing
        ) { item in
            Button("Close Chat", role: .destructive) { close(item) }
        } message: { _ in
            Text("Its pane closes in Herdr and the agent stops.")
        }
        .alert("The chat did not close", isPresented: Binding(get: { problem != nil }, set: { if !$0 { problem = nil } })) {
        } message: {
            Text(problem ?? "")
        }
    }

    @ViewBuilder
    private func section(_ title: String, _ items: [ChatItem]) -> some View {
        if !items.isEmpty {
            Section(title) {
                ForEach(items) { item in
                    Group {
                        if let question = item.question {
                            // The card draws its own background and border.
                            AnswerCard(item: item, question: question)
                                .listRowInsets(EdgeInsets(top: 4, leading: 0, bottom: 4, trailing: 0))
                                .listRowBackground(Color.clear)
                                .listRowSeparator(.hidden)
                        } else {
                            NavigationLink(value: Route.chat(item.key, title: item.chat.title)) { ChatRowLabel(item: item) }
                        }
                    }
                    .contextMenu {
                        Button("Close Chat", systemImage: "xmark.circle", role: .destructive) { closing = item }
                    }
                }
            }
        }
    }

    private func close(_ item: ChatItem) {
        Task {
            do {
                try await model.fleet.closeChatPane(item.key)
            } catch {
                problem = error.localizedDescription
            }
        }
    }
}
