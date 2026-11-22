import SheprdRemoteCore
import SwiftUI

/// One workspace, opened from the Workspaces tab: its branch and pull
/// request, its tabs, and buttons to start another tab: a terminal and the
/// agent last started, with the other agents folded below them.
struct WorkspaceScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.routeOpener) private var opener
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let key: WorkspaceKey
    @State private var agents: [AgentChoice] = []
    /// What a tap is starting: an agent's id, or `terminal`.
    @State private var starting: String?
    @State private var problem: String?
    /// The agents beyond the two large buttons are unfolded.
    @State private var showsOthers = false
    /// A tab just started, shown as soon as it exists.
    @State private var opened: Route?
    /// Files changed and not committed; `nil` until Sheprd says.
    @State private var changedFiles: Int?
    @State private var renaming = false
    @State private var newName = ""
    /// Why the last rename failed, shown apart from a tab that did not start.
    @State private var renameProblem: String?
    /// The name a rename asked for, shown with a spinner until the
    /// computer's next `home` carries it, the request fails, or it times out.
    @State private var pendingName: String?
    /// What a row's swipe asked to close, waiting for confirmation.
    @State private var closing: CloseTarget?
    /// The agent asked for a name, shown with a spinner until it answers.
    @State private var askedAgent: String?
    /// The pictures in the workspace's chats, its tabs combined.
    @State private var pictures = WorkspacePictureStore()
    /// What the person is asked to confirm: closing, or deleting the worktree.
    @State private var confirming: Ending?
    /// Whether the deletion asked about discards uncommitted changes.
    @State private var discarding = false
    /// What is on its way to the computer.
    @State private var ending: Ending?
    /// Why the last close or deletion failed.
    @State private var endProblem: EndProblem?
    @Environment(\.dismiss) private var dismiss

    private static let terminal = "terminal"

    var body: some View {
        let machine = model.fleet.machines.first { $0.id == key.device }
        if let machine, let workspace = machine.home?.workspaces.first(where: { $0.id == key.workspace }) {
            content(machine, workspace)
        } else {
            ContentUnavailableView("This workspace is gone", systemImage: "folder.badge.questionmark",
                                   description: Text("It was closed in Sheprd, or its computer is not connected."))
        }
    }

    private func content(_ machine: Fleet.Machine, _ workspace: Home.Workspace) -> some View {
        let chats = model.fleet.chatItems.filter { $0.computer.device == key.device && $0.workspace.id == key.workspace }
        let terminals = model.fleet.terminalItems.filter { $0.computer.device == key.device && $0.workspace.id == key.workspace }
        let repository = machine.home?.repositories.first { $0.workspaces.contains { $0.id == key.workspace } }
        // The chat answered last knows best what the workspace is about.
        let namer = chats.filter { $0.chat.summary?.connected == true }
            .max { ($0.lastAnswer ?? .distantPast) < ($1.lastAnswer ?? .distantPast) }
        return endingDialogs(List {
            Section {
                Summary(workspace: workspace, place: repository?.label ?? workspace.cwd)
                NavigationLink {
                    ComputerScreen(device: key.device).makesRoomForConnectionToast()
                } label: {
                    ComputerRow(computer: machine.computer, name: model.displayName(machine.computer), status: machine.status)
                }
                if let pullRequest = workspace.pullRequest, let url = URL(string: pullRequest.url) {
                    // Plain text and a chevron, like the rows below, not a
                    // tinted web link.
                    Link(destination: url) { PullRequestRow(pullRequest: pullRequest) }.tint(.primary)
                }
                if workspace.branch != nil {
                    RouteLink(.changes(key)) {
                        ChangesRow(count: changedFiles, uncommitted: workspace.uncommitted ?? false)
                    }
                }
            }
            Section {
                ForEach(chats) { item in
                    RouteLink(.chat(item.key, title: item.chat.title)) { ChatRowLabel(item: item, showsComputer: false, showsWorkspace: false, showsPullRequest: false) }
                        .closeSwipe(.chat(item.key, title: item.chat.title), closing: $closing)
                        .summarizeSwipe(item.key)
                }
                ForEach(terminals.filter { $0.agent != nil }) { item in
                    if let route = item.route {
                        RouteLink(route) { AgentTerminalRowLabel(item: item, showsComputer: false, showsWorkspace: false, showsPullRequest: false) }
                            .closeSwipe(item.closeTarget, closing: $closing)
                    }
                }
                ForEach(terminals.filter { $0.agent == nil }) { item in
                    if let pane = item.terminal.firstPane {
                        RouteLink(.term(PaneKey(device: key.device, pane: pane.pane), title: item.terminal.label)) {
                            TerminalRowLabel(terminal: item.terminal)
                        }
                        .closeSwipe(item.closeTarget, closing: $closing)
                    }
                }
            } header: {
                let count = chats.count + terminals.count
                Text(count == 1 ? "1 tab" : "\(count) tabs")
                    .contentTransition(.numericText(value: Double(count)))
            }
            WorkspacePictures(pictures: pictures, chats: chats, open: show)
            launcher(workspace, online: isOnline(machine))
            endings(workspace, online: isOnline(machine))
        }
        // Tabs the computer opens or closes slide in and out, and the
        // count rolls, rather than the rows jumping into place.
        .animation(reduceMotion ? nil : .snappy, value: chats.map(\.id) + terminals.map(\.id))
        .navigationTitle(workspace.label)
        .navigationBarTitleDisplayMode(.inline)
        // The iPad's sidebar keeps its tabs below a workspace.
        .toolbar(opener == nil ? .hidden : .automatic, for: .tabBar)
        .toolbar {
            ToolbarItem(placement: .principal) {
                Button {
                    // A rename on its way ends before another starts. Not
                    // `.disabled`, which would fade the dimmed name further.
                    guard pendingName == nil, askedAgent == nil else { return }
                    newName = workspace.label
                    renaming = true
                } label: {
                    let busy = pendingName != nil || askedAgent != nil
                    HStack(spacing: 4) {
                        Text(pendingName ?? workspace.label).font(.headline).lineLimit(1)
                            .foregroundStyle(busy ? .secondary : .primary)
                        if busy {
                            ProgressView().controlSize(.mini)
                        } else {
                            Image(systemName: "pencil.circle.fill").font(.caption).foregroundStyle(.tertiary)
                        }
                    }
                    .animation(.default, value: pendingName)
                    .animation(.default, value: askedAgent)
                }
                .buttonStyle(.plain)
                .accessibilityHint("Renames the workspace")
                .accessibilityValue(askedAgent.map { "Asking \($0) for a name" } ?? (pendingName == nil ? "" : "Renaming"))
            }
        }
        // The computer's next `home` carries the new name; stop waiting then.
        .onChange(of: workspace.label) { pendingName = nil }
        #if DEBUG
        // `-askAgentName YES` asks the workspace's chat for a name, as Ask
        // does, once the chat is listed.
        .onChange(of: namer?.id, initial: true) {
            guard let namer, askedAgent == nil, UserDefaults.standard.bool(forKey: "askAgentName") else { return }
            UserDefaults.standard.removeObject(forKey: "askAgentName")
            askName(of: namer, current: workspace.label)
        }
        #endif
        .onAppear {
            model.recentDevice = key.device
            #if DEBUG
            // The simulator loop's `-otherAgents YES` unfolds the agents,
            // and `-renameWorkspace YES` opens the rename alert.
            if UserDefaults.standard.bool(forKey: "otherAgents") { showsOthers = true }
            // `-openChanges YES` opens the workspace's changes.
            if UserDefaults.standard.bool(forKey: "openChanges"), opened == nil { opened = .changes(key) }
            // `-summarizeFirst YES` asks for the summary of the first chat
            // with a reply, as its row's swipe does.
            if UserDefaults.standard.bool(forKey: "summarizeFirst"),
               let chat = model.fleet.chatItems.first(where: { $0.computer.device == key.device && $0.workspace.id == key.workspace && $0.chat.summary?.lastText != nil }) {
                Task { try? await model.fleet.summarize(chat.key) }
            }
            // `-renamePending NAME` shows a rename on its way, sending nothing.
            if let name = UserDefaults.standard.string(forKey: "renamePending") { pendingName = name }
            if UserDefaults.standard.bool(forKey: "renameWorkspace") {
                newName = workspace.label
                renaming = true
            }
            #endif
        }
        // This screen is right behind what it opens, so Back is the way here.
        .navigationDestination(item: $opened) { $0.screen.makesRoomForConnectionToast().environment(\.offersWorkspace, false) }
        // Keyed on the whole status, so a link that comes back at another
        // address loads again: a phone waking from sleep still shows the
        // address it had, and the first try there fails.
        .task(id: machine.status) {
            guard isOnline(machine) else { return }
            while agents.isEmpty, !Task.isCancelled {
                if let loaded = try? await model.fleet.call(key.device, { client, token in try await client.newOptions(bearer: token) }).agents {
                    agents = loaded
                    return
                }
                try? await Task.sleep(for: .seconds(3))
            }
        }
        // Again on each return, as the chats may hold more pictures since.
        .task(id: WorkspacePictureStore.Load(panes: chats.map(\.chat.pane), online: isOnline(machine))) {
            await pictures.load(.init(panes: chats.map(\.chat.pane), online: isOnline(machine)), device: key.device, fleet: model.fleet)
        }
        // Again on each return, as an agent may have changed more since,
        // and when the computer's pencil comes or goes, so the count follows it.
        .task(id: ChangesLoad(status: machine.status, uncommitted: workspace.uncommitted)) {
            guard isOnline(machine), workspace.branch != nil else { return }
            let (device, id) = (key.device, key.workspace)
            changedFiles = try? await model.fleet.call(device) { client, token in
                try await client.changes(workspace: id, scope: .uncommitted, bearer: token)
            }.total
        }
        .closeConfirmation($closing)
        .alert("The tab did not start", isPresented: Binding(get: { problem != nil }, set: { if !$0 { problem = nil } })) {
        } message: {
            Text(problem ?? "")
        }
        .selectsAllOnEdit(while: renaming)
        .alert("Rename Workspace", isPresented: $renaming) {
            TextField("Name", text: $newName)
                .textInputAutocapitalization(.never)
            Button("Cancel", role: .cancel) {}
            if let namer {
                Button("Ask \(agentName(namer))") { askName(of: namer, current: workspace.label) }
            }
            Button("Rename") { rename(to: newName, from: workspace.label) }
                .disabled(newName.trimmingCharacters(in: .whitespaces).isEmpty)
        } message: {
            if let namer {
                Text("The name shows in Sheprd and in your lists. \(agentName(namer)) can suggest a short one from its chat.")
            } else {
                Text("The name shows in Sheprd and in your lists.")
            }
        }
        .alert("The workspace was not renamed", isPresented: Binding(get: { renameProblem != nil }, set: { if !$0 { renameProblem = nil } })) {
        } message: {
            Text(renameProblem ?? "")
        })
    }

    /// The failure of closing the workspace or deleting its worktree; apart
    /// from the screen's other modifiers, which together are more than the
    /// type checker takes in one expression. A deletion the computer refused
    /// as a conflict, with changes the phone had not counted, offers to
    /// discard them; the computer's reason always shows.
    private func endingDialogs(_ content: some View) -> some View {
        content
            .alert(endProblem?.action.failure ?? "", isPresented: Binding(get: { endProblem != nil }, set: { if !$0 { endProblem = nil } }),
                   presenting: endProblem) { problem in
                if problem.changes > 0 {
                    Button("Discard Changes and Delete", role: .destructive) { end(.removeWorktree, discard: true) }
                    Button("Cancel", role: .cancel) {}
                }
            } message: { problem in
                Text(problem.changes > 0 ? problem.reason + "\n\n" + Ending.discardWarning(problem.changes) : problem.reason)
            }
    }

    /// Closing the workspace, and deleting its worktree when it is a linked
    /// one, as its menu in Sheprd offers.
    private func endings(_ workspace: Home.Workspace, online: Bool) -> some View {
        Section {
            endingButton(.close)
            if workspace.worktree?.linked == true { endingButton(.removeWorktree) }
        } footer: {
            if workspace.worktree?.linked == true {
                Text("Deleting asks before discarding uncommitted changes, and leaves a checkout with an archive script for Sheprd.")
            }
        }
        .disabled(!online || ending != nil)
    }

    /// A button and its confirmation, attached to the button so the popover
    /// points at the row that asked.
    private func endingButton(_ action: Ending) -> some View {
        Button(role: .destructive) {
            discarding = action == .removeWorktree && (changedFiles ?? 0) > 0
            confirming = action
        } label: {
            HStack {
                Label(action.title, systemImage: action.symbol)
                Spacer()
                if ending == action { ProgressView() }
            }
            .contentShape(.rect)
        }
        .confirmationDialog(action.question(discarding: discarding),
                            isPresented: Binding(get: { confirming == action }, set: { if !$0 { confirming = nil } }),
                            titleVisibility: .visible) {
            Button(discarding ? "Discard Changes and Delete" : action.title, role: .destructive) {
                end(action, discard: discarding)
            }
        } message: {
            Text(action.consequence(discarding: discarding ? changedFiles ?? 0 : 0))
        }
    }

    private func end(_ action: Ending, discard: Bool = false) {
        ending = action
        Task {
            defer { ending = nil }
            do {
                switch action {
                case .close: try await model.fleet.closeWorkspace(key)
                case .removeWorktree: try await model.fleet.removeWorktree(key, discard: discard)
                }
                // Gone from the computer: its lists drop it on the next home.
                if opener == nil { dismiss() }
            } catch {
                // Changes made since the phone counted them are a conflict
                // (409); an archive script or a main checkout is not, and
                // discarding would not get past it.
                let conflict = if case RemoteError.status(409, _) = error { true } else { false }
                let changes = action == .removeWorktree && !discard && conflict ? await uncommitted() : 0
                endProblem = EndProblem(action: action, reason: error.localizedDescription, changes: changes)
            }
        }
    }

    /// The checkout's uncommitted changes as the computer counts them now;
    /// 0 when it cannot say.
    private func uncommitted() async -> Int {
        let (device, id) = (key.device, key.workspace)
        let count = try? await model.fleet.call(device) { client, token in
            try await client.changes(workspace: id, scope: .uncommitted, bearer: token)
        }.total
        if let count { changedFiles = count }
        return count ?? 0
    }

    private func rename(to label: String, from current: String) {
        let label = label.trimmingCharacters(in: .whitespaces)
        // The same name changes nothing, so no `home` would end the wait.
        guard label != current else { return }
        pendingName = label
        Task {
            do {
                try await model.fleet.renameWorkspace(key, to: label)
            } catch {
                pendingName = nil
                renameProblem = error.localizedDescription
                return
            }
            // Accepted; the name usually arrives with the next `home`, but
            // a computer that does not resend it must not spin forever.
            try? await Task.sleep(for: .seconds(10))
            if pendingName == label { pendingName = nil }
        }
    }

    private func agentName(_ item: ChatItem) -> String {
        let agent = item.chat.agent ?? ""
        return agents.first { $0.id == agent }?.name ?? (agent.isEmpty ? String(localized: "Agent") : agent.capitalized)
    }

    /// Asks the workspace's chat for a short name, then renames the
    /// workspace to it. The prompt and its answer stay in that chat, where
    /// it waits its turn behind anything the agent is doing; the phone
    /// reads the answer from the transcript, as no agent can rename a
    /// workspace Sheprd keeps for Herdr 2.
    private func askName(of item: ChatItem, current: String) {
        let prompt = WorkspaceNaming.prompt(current: current)
        let (device, pane, agent) = (key.device, item.chat.pane, agentName(item))
        askedAgent = agent
        Task {
            defer { askedAgent = nil }
            // Only this prompt's answer counts: an earlier identical prompt
            // that failed would otherwise answer a retry still queued.
            let before = try? await model.fleet.call(device) { client, token in
                try await client.chat(pane, bearer: token)
            }
            let after = before?.transcript.entries.last?.id
            do {
                let refused = try await model.fleet.call(device) { client, token in
                    try await client.command(.prompt(text: prompt), pane: pane, bearer: token)
                }
                if let refused {
                    renameProblem = refused
                    return
                }
            } catch {
                renameProblem = error.localizedDescription
                return
            }
            // Long enough for a busy agent to finish first, bounded so the
            // title does not spin forever; a dropped link is tried again.
            let deadline = ContinuousClock.now + .seconds(180)
            while ContinuousClock.now < deadline {
                try? await Task.sleep(for: .seconds(2))
                guard let snapshot = try? await model.fleet.call(device, { client, token in
                    try await client.chat(pane, bearer: token)
                }) else { continue }
                switch WorkspaceNaming.answer(to: prompt, in: snapshot.transcript, after: after) {
                case let .name(name)?:
                    askedAgent = nil
                    rename(to: name, from: current)
                    return
                case .noName?:
                    renameProblem = String(localized: "\(agent) did not suggest a name.")
                    return
                case nil:
                    continue
                }
            }
            renameProblem = String(localized: "\(agent) did not answer in time.")
        }
    }

    /// One tap starts a terminal or the agent last started, as the two
    /// most likely tabs, with the other agents folded below them. A long
    /// press on an agent opens the form with a first prompt.
    private func launcher(_ workspace: Home.Workspace, online: Bool) -> some View {
        let recent = agents.first { $0.id == model.recentAgent } ?? agents.first
        let others = agents.filter { $0.id != recent?.id }
        return Section("New tab") {
            HStack(spacing: 10) {
                bigButton("Terminal", id: Self.terminal) {
                    Image(systemName: "apple.terminal").font(.system(size: 19, weight: .semibold))
                        .frame(width: 40, height: 40).background(.fill.secondary, in: .rect(cornerRadius: 40 * 0.28))
                } action: {
                    openTerminal(in: workspace)
                }
                if let recent {
                    bigButton(recent.name, id: recent.id) { AgentIcon(agent: recent.id, size: 40) } action: {
                        start(recent, in: workspace)
                    }
                    .contextMenu { firstPrompt(recent, in: workspace) }
                }
            }
            .listRowInsets(EdgeInsets(top: 12, leading: 12, bottom: 12, trailing: 12))
            .listRowSeparator(.hidden, edges: .bottom)
            if let recent { StartChoiceRow(agent: recent.name) }
            if !others.isEmpty {
                Button {
                    withAnimation { showsOthers.toggle() }
                } label: {
                    HStack(spacing: 12) {
                        Text("Other agents")
                        Spacer()
                        if !showsOthers {
                            if others.contains(where: { model.startChoice(for: $0.name).bypasses }) { Self.unasked }
                            HStack(spacing: 4) { ForEach(others) { AgentIcon(agent: $0.id, size: 22) } }
                                .transition(.opacity)
                                .accessibilityHidden(true)
                        }
                        Image(systemName: "chevron.down").font(.footnote.weight(.semibold)).foregroundStyle(.tertiary)
                            .rotationEffect(.degrees(showsOthers ? 180 : 0))
                    }
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .accessibilityValue(showsOthers ? "Shown" : "Hidden")
                if showsOthers {
                    ForEach(others) { agent in
                        Button {
                            start(agent, in: workspace)
                        } label: {
                            HStack(spacing: 12) {
                                AgentIcon(agent: agent.id, size: 28)
                                VStack(alignment: .leading, spacing: 1) {
                                    Text(agent.name)
                                    if let label = model.startChoice(for: agent.name).label {
                                        Text(label).font(.caption).foregroundStyle(.secondary)
                                    }
                                }
                                Spacer()
                                if model.startChoice(for: agent.name).bypasses { Self.unasked }
                                if starting == agent.id { ProgressView() }
                            }
                            .contentShape(.rect)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("New \(agent.name) tab")
                        .accessibilityValue(model.startChoice(for: agent.name).label ?? "")
                        .contextMenu { firstPrompt(agent, in: workspace) }
                    }
                }
            }
        }
        .disabled(!online || starting != nil)
    }

    /// Says a new tab would run tools without asking.
    private static var unasked: some View {
        Image(systemName: "bolt.shield.fill").foregroundStyle(.red).accessibilityLabel("Runs without asking")
    }

    private func firstPrompt(_ agent: AgentChoice, in workspace: Home.Workspace) -> some View {
        Button("With a First Prompt…", systemImage: "text.bubble") {
            model.creating = .chat(device: key.device, workspace: workspace.id, label: workspace.label, agent: agent.id)
        }
    }

    private func bigButton(_ name: String, id: String, @ViewBuilder icon: () -> some View, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(spacing: 8) {
                if starting == id { ProgressView().frame(width: 40, height: 40) } else { icon() }
                Text(name).font(.subheadline.weight(.semibold)).lineLimit(1)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 14)
            .background(.fill.quinary, in: .rect(cornerRadius: 16))
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("New \(name) tab")
    }

    private func isOnline(_ machine: Fleet.Machine) -> Bool {
        if case .online = machine.status { true } else { false }
    }

    /// A tab just started: beside the sidebar on an iPad, else pushed.
    private func show(_ route: Route) {
        if let opener { opener.open(route) } else { opened = route }
    }

    private func start(_ agent: AgentChoice, in workspace: Home.Workspace) {
        let (device, id) = (key.device, workspace.id)
        let start = model.startChoice(for: agent.name).start
        starting = agent.id
        Task {
            defer { starting = nil }
            do {
                let created = try await model.fleet.call(device) { client, token in
                    try await client.startChat(NewChatRequest(agent: agent.id, start: start), workspace: id, bearer: token)
                }
                model.recentAgent = agent.id
                if let problem = model.startProblem(created, start: start, device: device) { model.notice = problem }
                if let pane = created.pane { show(.chat(PaneKey(device: device, pane: pane), title: agent.name)) }
            } catch {
                problem = error.localizedDescription
            }
        }
    }

    /// Sheprd opens a tab in the workspace of the pane an action names, so
    /// any of its panes will do.
    /// A terminal tab beside the workspace's panes, or its first one, which
    /// the computer starts in the workspace's folder.
    private func openTerminal(in workspace: Home.Workspace) {
        let pane = workspace.anyPane
        let id = workspace.id
        let device = key.device
        starting = Self.terminal
        Task {
            defer { starting = nil }
            do {
                let created = try await model.fleet.call(device) { client, token -> String? in
                    if let pane {
                        return try await client.termAction(.newTab, pane: pane, bearer: token)
                    }
                    return try await client.newTerminal(workspace: id, bearer: token)
                }
                if let created { show(.term(PaneKey(device: device, pane: created), title: "Terminal")) }
            } catch {
                problem = error.localizedDescription
            }
        }
    }
}

/// How a workspace ends from the phone.
private enum Ending: Hashable, Identifiable {
    case close
    case removeWorktree

    var id: Self { self }

    var title: String {
        switch self {
        case .close: "Close Workspace"
        case .removeWorktree: "Delete Worktree"
        }
    }

    var symbol: String {
        switch self {
        case .close: "xmark.circle"
        case .removeWorktree: "trash"
        }
    }

    func question(discarding: Bool) -> String {
        switch self {
        case .close: "Close this workspace?"
        case .removeWorktree: discarding ? "Delete this worktree and its changes?" : "Delete this worktree?"
        }
    }

    /// What ending does; `changes` uncommitted ones are lost with it.
    func consequence(discarding changes: Int) -> String {
        switch self {
        case .close: "Its tabs close in Sheprd and their agents and programs stop. Its files stay on the computer."
        case .removeWorktree where changes > 0:
            Self.discardWarning(changes) + " Its checkout is removed and the workspace closes, ending its tabs. Its branch stays."
        case .removeWorktree: "Its checkout is removed from the computer and the workspace closes, ending its tabs. Its branch stays."
        }
    }

    static func discardWarning(_ changes: Int) -> String {
        changes == 1
            ? "Its uncommitted change is lost and cannot be recovered."
            : "Its \(changes) uncommitted changes are lost and cannot be recovered."
    }

    var failure: String {
        switch self {
        case .close: "The workspace was not closed"
        case .removeWorktree: "The worktree was not deleted"
        }
    }
}

/// Why ending a workspace failed, and the uncommitted changes a refused
/// deletion could discard instead.
private struct EndProblem: Identifiable {
    let id = UUID()
    let action: Ending
    let reason: String
    let changes: Int
}

/// The workspace's branch, and where it is.
private struct Summary: View {
    var workspace: Home.Workspace
    var place: String

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "arrow.triangle.branch").font(.system(size: 14, weight: .semibold))
                .foregroundStyle(.tint)
                .frame(width: 28, height: 28).background(.tint.opacity(0.15), in: .rect(cornerRadius: 28 * 0.28))
            VStack(alignment: .leading, spacing: 2) {
                Text(workspace.branch ?? workspace.label)
                    .font(.body.monospaced()).lineLimit(1).truncationMode(.middle)
                Text(place).font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
            }
        }
    }
}

/// The computer the workspace is on, and whether the phone reaches it now;
/// opens that computer's page.
private struct ComputerRow: View {
    var computer: Computer
    var name: String
    var status: LinkStatus

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: computer.via == nil ? "laptopcomputer" : "server.rack").font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.tint)
                .frame(width: 28, height: 28).background(.tint.opacity(0.15), in: .rect(cornerRadius: 28 * 0.28))
            Text(name).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
            ComputerStatusLine(status: status).font(.subheadline).labelStyle(.iconOnly).imageScale(.small)
        }
        .accessibilityElement(children: .combine)
    }
}

/// The workspace's pull request: its number, its state as an icon, and
/// a link out to it.
private struct PullRequestRow: View {
    var pullRequest: Home.PullRequest

    var body: some View {
        let tone = pullRequest.tone
        HStack(spacing: 12) {
            Image(systemName: "arrow.triangle.pull").font(.system(size: 14, weight: .semibold))
                .foregroundStyle(tone.color)
                .frame(width: 28, height: 28).background(tone.color.opacity(0.15), in: .rect(cornerRadius: 28 * 0.28))
            Text(verbatim: "Pull request #\(pullRequest.number)").foregroundStyle(.primary)
                .frame(maxWidth: .infinity, alignment: .leading)
            Image(systemName: tone.symbol).foregroundStyle(tone.color).imageScale(.large)
            Image(systemName: "chevron.forward").font(.footnote.weight(.semibold)).foregroundStyle(.tertiary)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Pull request \(pullRequest.number), \(tone.label)")
        .accessibilityAddTraits(.isLink)
    }
}

/// When the workspace's uncommitted count is read again.
private struct ChangesLoad: Equatable {
    var status: LinkStatus
    var uncommitted: Bool?
}

/// Opens the workspace's changes, saying how many files are uncommitted,
/// with the desktop's pencil while there are any.
private struct ChangesRow: View {
    var count: Int?
    var uncommitted: Bool

    var body: some View {
        HStack(spacing: 12) {
            if uncommitted || (count ?? 0) > 0 {
                UncommittedMark(size: 28)
            } else {
                Image(systemName: "plusminus").font(.system(size: 14, weight: .semibold))
                    .frame(width: 28, height: 28).background(.fill.secondary, in: .rect(cornerRadius: 28 * 0.28))
            }
            Text("Changes").frame(maxWidth: .infinity, alignment: .leading)
            if let count {
                Text(count, format: .number).foregroundStyle(.secondary).monospacedDigit()
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
    }

    private var label: String {
        switch count {
        case nil: "Changes"
        case 1?: "Changes, 1 file"
        case let count?: "Changes, \(count) files"
        }
    }
}

/// A terminal tab without an agent: its name, and what runs in it. A
/// computer that does not say (Herdr 1) gets its folder's name instead,
/// so the second line is never blank.
private struct TerminalRowLabel: View {
    var terminal: Home.TerminalRow

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "apple.terminal").font(.system(size: 14, weight: .semibold))
                .frame(width: 28, height: 28).background(.fill.secondary, in: .rect(cornerRadius: 28 * 0.28))
            VStack(alignment: .leading, spacing: 2) {
                Text(terminal.label).lineLimit(1)
                switch terminal.firstPane?.job {
                case let .running(command)?:
                    Text(verbatim: command).font(.subheadline.monospaced()).foregroundStyle(.secondary)
                        .lineLimit(1).truncationMode(.tail)
                case .idle?:
                    Text("Idle").font(.subheadline).foregroundStyle(.tertiary)
                case .other?, nil:
                    Text(folder).font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
                }
            }
        }
    }

    /// The last part of the tab's folder, or a plain word when it has none.
    private var folder: String {
        let path = terminal.firstPane?.cwd ?? terminal.cwd ?? ""
        let name = URL(fileURLWithPath: path).lastPathComponent
        return path.isEmpty || name.isEmpty || name == "/" ? String(localized: "Terminal") : name
    }
}
