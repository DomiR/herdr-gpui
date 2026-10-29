import HerdrRemoteCore
import SwiftUI

/// Computers C: an outline from computer to workspace to its chats and
/// terminal tabs.
struct ComputersTab: View {
    @Environment(AppModel.self) private var model
    @Binding var pairing: Bool
    @State private var collapsed: Set<String> = []

    var body: some View {
        List {
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
                } label: {
                    ComputerLabel(machine: machine)
                }
                .swipeActions {
                    Button("Forget", role: .destructive) { model.forget(machine.computer) }
                }
            }
            Button { pairing = true } label: { Label("Add a computer", systemImage: "plus") }
        }
        .listStyle(.sidebar)
        .navigationTitle("Computers")
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                NavigationLink { SettingsView() } label: { Image(systemName: "gearshape") }
                    .accessibilityLabel("Settings")
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
            Label(model.displayName(machine.computer), systemImage: "laptopcomputer").font(.headline)
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

/// Chats, workspaces, and terminals across every computer, by name.
struct SearchTab: View {
    @Environment(AppModel.self) private var model
    @State private var query = ""

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
        .searchable(text: $query, prompt: "Chats, workspaces, terminals")
        .navigationTitle("Search")
    }
}
