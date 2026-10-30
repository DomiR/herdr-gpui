import Foundation
import HerdrRemoteCore
import Observation
import os
import WidgetKit

private let log = Logger(subsystem: "so.pen.herdr.remote", category: "fleet")

/// A chat row with where it lives, for lists across computers.
struct ChatItem: Identifiable, Hashable {
    let computer: Computer
    let workspace: Home.Workspace
    let chat: Home.ChatRow

    var id: String { "\(computer.device)/\(chat.pane)" }
    var key: PaneKey { PaneKey(device: computer.device, pane: chat.pane) }

    /// The question waiting, with its options.
    var question: Permission? { chat.summary?.question }
    var needsYou: Bool { question != nil || chat.status == .blocked }
    var working: Bool { !needsYou && (chat.status == .working || chat.summary?.status == .working) }
}

enum FleetError: Error, LocalizedError {
    case offline(String)

    var errorDescription: String? {
        switch self {
        case let .offline(name): "\(name) is not connected."
        }
    }
}

/// A terminal tab with where it lives.
struct TerminalItem: Identifiable, Hashable {
    let computer: Computer
    let workspace: Home.Workspace
    let terminal: Home.TerminalRow

    var id: String { "\(computer.device)/\(terminal.tab)" }
}

/// Every paired computer's link and what it last showed. Launch draws the
/// cached homes at once, marked stale until each link brings a fresh one.
@MainActor
@Observable
final class Fleet {
    struct Machine: Identifiable {
        var computer: Computer
        var status: LinkStatus = .connecting
        var home: Home?
        /// `home` came from the cache and the link has not refreshed it.
        var stale = true
        var id: String { computer.device }
    }

    private(set) var machines: [Machine] = []
    private var links: [String: ComputerLink] = [:]
    private var readers: [String: Task<Void, Never>] = [:]
    private var chats: [PaneKey: ChatStore] = [:]
    private var terms: [PaneKey: TermStore] = [:]
    private var active = false
    let sessions: Sessions
    let cache: Cache

    init(sessions: Sessions, cache: Cache = .shared) {
        self.sessions = sessions
        self.cache = cache
    }

    /// Follows `computers`: new ones get a link, forgotten ones lose theirs.
    func sync(_ computers: [Computer]) {
        let wanted = Set(computers.map(\.device))
        for machine in machines where !wanted.contains(machine.id) {
            stopLink(machine.id)
            cache.clear(machine.id)
        }
        machines = computers.map { computer in
            if var known = machines.first(where: { $0.id == computer.device }) {
                known.computer = computer
                return known
            }
            return Machine(computer: computer, home: cache.home(computer.device))
        }
        if active { machines.forEach { startLink($0.computer) } }
        // From the cached homes at launch, and without forgotten ones.
        publishSummary()
    }

    /// The app came to the foreground: links connect and catch up.
    func activate() {
        active = true
        machines.forEach { startLink($0.computer) }
        for link in links.values { Task { await link.networkChanged() } }
    }

    /// The app left the foreground: sockets close; pushes cover the rest.
    func deactivate() {
        active = false
        for id in Array(links.keys) { stopLink(id) }
        for index in machines.indices { machines[index].stale = true }
    }

    func networkChanged() {
        for link in links.values { Task { await link.networkChanged() } }
    }

    // MARK: Lists

    var chatItems: [ChatItem] {
        machines.flatMap { machine in
            (machine.home?.workspaces ?? []).flatMap { workspace in
                workspace.chats.map { ChatItem(computer: machine.computer, workspace: workspace, chat: $0) }
            }
        }
    }

    var terminalItems: [TerminalItem] {
        machines.flatMap { machine in
            (machine.home?.workspaces ?? []).flatMap { workspace in
                workspace.terminals.map { TerminalItem(computer: machine.computer, workspace: workspace, terminal: $0) }
            }
        }
    }

    var waiting: Int { machines.reduce(0) { $0 + ($1.home?.waiting ?? 0) } }

    /// Writes what widgets, Controls, Siri, Spotlight, and the share
    /// extension show, and reloads the widgets when it changed.
    func publishSummary() {
        let chats = chatItems.map { item in
            WidgetSummary.Chat(
                device: item.computer.device,
                computerId: item.computer.fingerprint.hex,
                computer: item.computer.name,
                pane: item.chat.pane,
                title: item.chat.title,
                workspace: item.workspace.label,
                workspaceId: item.workspace.id,
                agent: item.chat.agent,
                needsYou: item.needsYou,
                working: item.working,
                question: item.question.map { question in
                    WidgetSummary.Question(
                        permission: question.id,
                        title: question.title,
                        options: question.options.prefix(4).map { .init(id: $0.id, name: $0.name, kind: $0.kind) }
                    )
                }
            )
        }
        if WidgetSummary(chats: chats, updated: .now).save() {
            WidgetCenter.shared.reloadTimelines(ofKind: WidgetSummary.kind)
        }
        onSummary?()
    }

    /// Called after each summary, for Spotlight.
    @ObservationIgnored var onSummary: (() -> Void)?
    /// Called when a computer's link comes online, for host pairing.
    @ObservationIgnored var onOnline: ((Computer) -> Void)?

    // MARK: Views

    func openChat(_ key: PaneKey) -> ChatStore {
        if let open = chats[key] { return open }
        let store = ChatStore(key: key, cached: cache.chat(key.device, pane: key.pane))
        chats[key] = store
        let since = store.cursor
        Task { await links[key.device]?.watch(.chat(pane: key.pane), since: since) }
        return store
    }

    func closeChat(_ store: ChatStore) {
        guard chats[store.key] === store else { return }
        chats[store.key] = nil
        if let transcript = store.transcript {
            cache.save(chat: transcript, cursor: store.cursor, device: store.key.device, pane: store.key.pane)
        }
        Task { await links[store.key.device]?.unwatch(.chat(pane: store.key.pane)) }
    }

    func openTerm(_ key: PaneKey) -> TermStore {
        if let open = terms[key] { return open }
        let store = TermStore(key: key)
        terms[key] = store
        Task { await links[key.device]?.watch(.term(pane: key.pane)) }
        return store
    }

    func closeTerm(_ store: TermStore) {
        guard terms[store.key] === store else { return }
        terms[store.key] = nil
        Task { await links[store.key.device]?.unwatch(.term(pane: store.key.pane)) }
    }

    /// Sends a command to a chat; answers a reason to show when it failed
    /// or the chat refused it.
    func send(_ command: ChatCommand, to key: PaneKey) async -> String? {
        do {
            return try await call(key.device) { client, token in
                try await client.command(command, pane: key.pane, bearer: token)
            }
        } catch {
            return error.localizedDescription
        }
    }

    /// Calls the daemon of `device` at the address its link is using.
    func call<Answer: Sendable>(
        _ device: String,
        _ body: @Sendable (RemoteClient, String) async throws -> Answer
    ) async throws -> Answer {
        guard let machine = machines.first(where: { $0.id == device }),
              case let .online(origin) = machine.status
        else { throw FleetError.offline(machines.first { $0.id == device }?.computer.name ?? "The computer") }
        let token = try await sessions.token(for: machine.computer, at: origin)
        let client = RemoteClient(origin: origin, fingerprint: machine.computer.fingerprint)
        return try await body(client, token)
    }

    /// Closes the pane a chat runs in, which stops its agent in Herdr.
    func closeChatPane(_ key: PaneKey) async throws {
        let pane = key.pane
        _ = try await call(key.device) { client, token in
            try await client.termAction(.closePane, pane: pane, bearer: token)
        }
    }

    // MARK: Links

    private func startLink(_ computer: Computer) {
        guard links[computer.device] == nil else { return }
        let link = ComputerLink(
            fingerprint: computer.fingerprint.hex,
            origins: computer.origins,
            io: LiveSocket.io(for: computer, sessions: sessions)
        )
        links[computer.device] = link
        let device = computer.device
        let watching = chats.filter { $0.key.device == device }.map { ($0.key.pane, $0.value.cursor) }
        let terminals = terms.keys.filter { $0.device == device }.map(\.pane)
        readers[device] = Task {
            await link.watch(.home)
            for (pane, cursor) in watching { await link.watch(.chat(pane: pane), since: cursor) }
            for pane in terminals { await link.watch(.term(pane: pane)) }
            await link.start()
            for await update in link.updates {
                self.handle(update, from: device)
            }
        }
    }

    private func stopLink(_ device: String) {
        readers.removeValue(forKey: device)?.cancel()
        if let link = links.removeValue(forKey: device) { Task { await link.stop() } }
    }

    private func handle(_ update: LinkUpdate, from device: String) {
        guard let index = machines.firstIndex(where: { $0.id == device }) else { return }
        switch update {
        case let .status(status):
            machines[index].status = status
            if case .revoked = status { stopLink(device) }
            if case .online = status { onOnline?(machines[index].computer) }
        case let .event(.home(home)):
            machines[index].home = home
            machines[index].stale = false
            cache.save(home: home, device: device)
            publishSummary()
        case let .event(.error(message)):
            log.error("\(self.machines[index].computer.name, privacy: .public) refused: \(message, privacy: .public)")
        case let .event(.termScreen(screen)):
            terms[PaneKey(device: device, pane: screen.pane)]?.apply(screen)
        case let .event(event):
            guard let pane = event.pane, let store = chats[PaneKey(device: device, pane: pane)] else { return }
            let before = store.diverged
            store.apply(event)
            if store.diverged, !before {
                // A change that did not fit: start over from a snapshot.
                Task { await links[device]?.watch(.chat(pane: pane), since: nil) }
            }
        }
    }
}

extension LiveEvent {
    /// The chat pane a chat event is about.
    var pane: String? {
        switch self {
        case let .chatSnapshot(snapshot): snapshot.pane
        case let .chatResumed(pane, _, _), let .chatChanges(pane, _), let .chatConnection(pane, _): pane
        default: nil
        }
    }
}
