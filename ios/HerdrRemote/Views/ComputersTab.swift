import HerdrRemoteCore
import SwiftUI

/// Computers C: an outline from computer to workspace to its chats and
/// terminal tabs, with search across all of them, and what was said in
/// the chats, at the top.
struct ComputersTab: View {
    @Environment(AppModel.self) private var model
    @Binding var pairing: Bool
    @State private var collapsed: Set<String> = []
    @State private var query = ""

    var body: some View {
        Group {
            if query.isEmpty { outline } else { SearchResults(query: query) }
        }
        .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Chats, messages, terminals")
        .navigationBarTitleDisplayMode(.inline)
    }

    private var outline: some View {
        List {
            Section {
                machines
            }
            Section {
                Button { pairing = true } label: { Label("Add a computer", systemImage: "plus") }
            }
        }
        .listStyle(.sidebar)
    }

    private var machines: some View {
        ForEach(model.fleet.machines) { machine in
            DisclosureGroup(isExpanded: expanded(machine.id)) {
                ForEach(machine.home?.workspaces ?? []) { workspace in
                    DisclosureGroup(isExpanded: expanded("\(machine.id)/\(workspace.id)")) {
                        ForEach(workspace.chats) { chat in
                            let item = ChatItem(computer: machine.computer, workspace: workspace, chat: chat)
                            NavigationLink(value: Route.chat(item.key, title: chat.title)) {
                                ChatRowLabel(item: item, showsComputer: false)
                            }
                        }
                        Button {
                            model.creating = .chat(device: machine.id, workspace: workspace.id, label: workspace.label)
                        } label: {
                            Label("New chat", systemImage: "plus.bubble")
                        }
                        ForEach(workspace.terminals) { terminal in
                            if let pane = terminal.firstPane {
                                NavigationLink(value: Route.term(PaneKey(device: machine.id, pane: pane.pane), title: terminal.label)) {
                                    Label(terminal.label, systemImage: "apple.terminal").lineLimit(1)
                                }
                            }
                        }
                    } label: {
                        Label {
                            HStack {
                                Text(workspace.label).lineLimit(1).layoutPriority(1)
                                if let branch = workspace.branch, branch != workspace.label {
                                    Text(branch).font(.caption.monospaced()).foregroundStyle(.secondary)
                                        .lineLimit(1).truncationMode(.middle)
                                }
                            }
                        } icon: {
                            Image(systemName: "folder")
                        }
                    }
                }
                Button {
                    model.creating = .workspace(device: machine.id)
                } label: {
                    Label("New workspace", systemImage: "folder.badge.plus")
                }
                HostRows(computer: machine.computer)
            } label: {
                ComputerLabel(machine: machine)
            }
            .swipeActions {
                Button("Forget", role: .destructive) { model.forget(machine.computer) }
            }
        }
    }

    private func expanded(_ id: String) -> Binding<Bool> {
        Binding(
            get: { !collapsed.contains(id) },
            set: { open in if open { collapsed.remove(id) } else { collapsed.insert(id) } }
        )
    }
}

struct ComputerLabel: View {
    @Environment(AppModel.self) private var model
    var machine: Fleet.Machine

    var body: some View {
        HStack {
            Label {
                VStack(alignment: .leading, spacing: 1) {
                    Text(model.displayName(machine.computer)).font(.headline)
                    if let vouching = model.vouching(machine.computer) {
                        Text("via \(model.displayName(vouching))").font(.caption).foregroundStyle(.secondary)
                    }
                }
            } icon: {
                Image(systemName: machine.computer.via == nil ? "laptopcomputer" : "server.rack")
            }
            Spacer()
            switch machine.status {
            case .online:
                if let waiting = machine.home?.waiting, waiting > 0 {
                    Text("\(waiting) need you").font(.caption.weight(.semibold)).foregroundStyle(.white)
                        .padding(.horizontal, 8).padding(.vertical, 2).background(.orange, in: .capsule)
                }
                Circle().fill(.green).frame(width: 8, height: 8).accessibilityLabel("Connected")
            case .connecting:
                ProgressView().controlSize(.small)
            case .offline:
                Image(systemName: "wifi.slash").foregroundStyle(.secondary).accessibilityLabel("Offline")
            case .revoked:
                Image(systemName: "exclamationmark.triangle").foregroundStyle(.red).accessibilityLabel("Unpaired")
            }
        }
    }
}

/// Chats, workspaces, and terminals across every computer, by name, and
/// the messages in chats that contain the query.
private struct SearchResults: View {
    @Environment(AppModel.self) private var model
    let query: String
    @State private var messages: [MessageHit] = []
    @State private var searching = false

    var body: some View {
        let needle = query.lowercased()
        let chats = model.fleet.chatItems.filter {
            needle.isEmpty || $0.chat.title.lowercased().contains(needle) || $0.workspace.label.lowercased().contains(needle)
        }
        let terminals = model.fleet.terminalItems.filter {
            needle.isEmpty || $0.terminal.label.lowercased().contains(needle) || $0.workspace.label.lowercased().contains(needle)
        }
        List {
            if !chats.isEmpty {
                Section("Chats") {
                    ForEach(chats) { item in
                        NavigationLink(value: Route.chat(item.key, title: item.chat.title)) { ChatRowLabel(item: item) }
                    }
                }
            }
            if !messages.isEmpty {
                Section("Messages") {
                    ForEach(messages) { hit in
                        NavigationLink(value: Route.chat(hit.item.key, title: hit.item.chat.title)) {
                            MessageHitRow(hit: hit, query: query)
                        }
                    }
                }
            }
            if !terminals.isEmpty {
                Section("Terminals") {
                    ForEach(terminals) { item in
                        if let pane = item.terminal.firstPane {
                            NavigationLink(value: Route.term(PaneKey(device: item.computer.device, pane: pane.pane), title: item.terminal.label)) {
                                Label("\(item.terminal.label) · \(item.computer.name) · \(item.workspace.label)", systemImage: "apple.terminal")
                            }
                        }
                    }
                }
            }
        }
        .overlay {
            if chats.isEmpty, terminals.isEmpty, messages.isEmpty, !searching { ContentUnavailableView.search(text: query) }
        }
        .task(id: query) {
            searching = true
            let found = await model.fleet.searchMessages(query)
            guard !Task.isCancelled else { return }
            messages = found
            searching = false
        }
    }
}
