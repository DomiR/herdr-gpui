import HerdrRemoteCore
import SwiftUI

/// Home B: what needs you, answered in place; what is working; the rest.
struct HomeTab: View {
    @Environment(AppModel.self) private var model
    @Binding var pairing: Bool

    var body: some View {
        let items = model.fleet.chatItems
        let asking = items.filter(\.needsYou)
        let working = items.filter(\.working)
        let rest = items.filter { !$0.needsYou && !$0.working }
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                ConnectionBanner()
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
                    ForEach(working) { link($0) }
                }
                if !rest.isEmpty {
                    SectionTitle(title: "Recent", count: nil, tint: .secondary)
                    VStack(spacing: 0) {
                        ForEach(rest) { item in
                            link(item, card: false)
                            if item.id != rest.last?.id { Divider().padding(.leading, 54) }
                        }
                    }
                    .background(.background.secondary, in: .rect(cornerRadius: 18))
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 16)
            .padding(.bottom, 24)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemGroupedBackground))
        .opacity(model.fleet.machines.allSatisfy(\.stale) && !items.isEmpty ? 0.6 : 1)
        .navigationTitle("Herdr")
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

    private func link(_ item: ChatItem, card: Bool = true) -> some View {
        NavigationLink(value: Route.chat(item.key, title: item.chat.title)) {
            ChatRowLabel(item: item)
                .padding(.horizontal, 14).padding(.vertical, card ? 12 : 8)
                .background(card ? AnyShapeStyle(.background.secondary) : AnyShapeStyle(.clear), in: .rect(cornerRadius: 18))
                .contentShape(.rect)
        }
        .buttonStyle(.plain)
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
        .navigationTitle("Board")
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
