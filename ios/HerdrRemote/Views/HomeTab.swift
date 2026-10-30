import HerdrRemoteCore
import SwiftUI

/// Home B: what needs you, answered in place; what is working; the rest.
struct HomeTab: View {
    @Environment(AppModel.self) private var model
    @Binding var pairing: Bool
    /// The chat a long press asked to close, waiting for confirmation.
    @State private var closing: ChatItem?
    @State private var problem: String?

    var body: some View {
        let items = model.fleet.chatItems
        let asking = items.filter(\.needsYou)
        let working = items.filter(\.working)
        let rest = items.filter { !$0.needsYou && !$0.working }
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                if !asking.isEmpty {
                    SectionTitle(title: "Needs you", count: asking.count, tint: .orange)
                    ForEach(asking) { item in
                        if let question = item.question {
                            AnswerCard(item: item, question: question)
                        } else {
                            link(item)
                        }
                    }
                }
                if !working.isEmpty {
                    SectionTitle(title: "Working", count: working.count, tint: .blue)
                    rows(working)
                }
                if !rest.isEmpty {
                    SectionTitle(title: "Recent", count: nil, tint: .secondary)
                    rows(rest)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 16)
            .padding(.bottom, 24)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemGroupedBackground))
        .safeAreaInset(edge: .bottom) { ConnectionToast(pairing: $pairing) }
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
        .navigationBarTitleDisplayMode(.inline)
        .refreshable { model.fleet.networkChanged() }
        .overlay {
            if model.computers.isEmpty {
                ContentUnavailableView {
                    Label("No computers yet", systemImage: "desktopcomputer")
                } description: {
                    Text("In Herdr, open Remote control > Pair device, then scan its code here.")
                } actions: {
                    Button("Pair a computer") { pairing = true }.buttonStyle(.glassProminent)
                }
            } else if items.isEmpty {
                ContentUnavailableView("No chats", systemImage: "bubble.left.and.text.bubble.right",
                                       description: Text("Chats running in Herdr on your computers appear here."))
            }
        }
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button("New Workspace", systemImage: "folder.badge.plus") { model.creating = .workspace(device: nil) }
                        .disabled(model.fleet.machines.isEmpty)
                    Button("Pair a Computer", systemImage: "laptopcomputer.and.iphone") { pairing = true }
                } label: {
                    Image(systemName: "plus")
                }
                .accessibilityLabel("New")
            }
        }
    }

    /// Chats as one grouped list, so a section of them stays compact.
    private func rows(_ items: [ChatItem]) -> some View {
        VStack(spacing: 0) {
            ForEach(items) { item in
                link(item, card: false)
                if item.id != items.last?.id { Divider().padding(.leading, 54) }
            }
        }
        .background(.background.secondary, in: .rect(cornerRadius: 18))
    }

    private func link(_ item: ChatItem, card: Bool = true) -> some View {
        NavigationLink(value: Route.chat(item.key, title: item.chat.title)) {
            ChatRowLabel(item: item)
                .padding(.leading, 14).padding(.trailing, 12).padding(.vertical, card ? 12 : 8)
                .background(card ? AnyShapeStyle(.background.secondary) : AnyShapeStyle(.clear), in: .rect(cornerRadius: 18))
                .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button("Close Chat", systemImage: "xmark.circle", role: .destructive) { closing = item }
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

struct SectionTitle: View {
    var title: String
    var count: Int?
    var tint: Color

    var body: some View {
        HStack {
            Text(title).font(.title3.bold())
            if let count {
                Text("\(count)").font(.subheadline.bold()).foregroundStyle(.white)
                    .padding(.horizontal, 8).padding(.vertical, 2).background(tint, in: .capsule)
            }
        }
        .padding(.top, 8)
    }
}

/// Every chat across computers grouped by state, the board the web app
/// shows behind its eye.
struct BoardTab: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let items = model.fleet.chatItems
        List {
            group("Needs you", items.filter(\.needsYou))
            group("Working", items.filter(\.working))
            group("Idle", items.filter { !$0.needsYou && !$0.working })
        }
        .navigationBarTitleDisplayMode(.inline)
        .refreshable { model.fleet.networkChanged() }
    }

    @ViewBuilder
    private func group(_ title: String, _ items: [ChatItem]) -> some View {
        if !items.isEmpty {
            Section(title) {
                ForEach(items) { item in
                    NavigationLink(value: Route.chat(item.key, title: item.chat.title)) { ChatRowLabel(item: item) }
                }
            }
        }
    }
}
