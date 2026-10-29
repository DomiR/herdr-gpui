import HerdrRemoteCore
import SwiftUI

enum RootTab: Hashable { case home, board, computers, search }

/// Home B: attention first, with Board, Computers, and Search as tabs.
struct RootView: View {
    @Environment(AppModel.self) private var model
    @State private var tab = RootTab.home
    @State private var homePath = NavigationPath()
    @State private var pairing = false

    var body: some View {
        TabView(selection: $tab) {
            Tab("Home", systemImage: "house", value: RootTab.home) {
                NavigationStack(path: $homePath) {
                    HomeTab(pairing: $pairing).withRoutes()
                }
            }
            .badge(model.fleet.waiting)
            Tab("Board", systemImage: "square.grid.2x2", value: RootTab.board) {
                NavigationStack { BoardTab().withRoutes() }
            }
            Tab("Computers", systemImage: "desktopcomputer", value: RootTab.computers) {
                NavigationStack { ComputersTab(pairing: $pairing).withRoutes() }
            }
            Tab(value: RootTab.search, role: .search) {
                NavigationStack { SearchTab().withRoutes() }
            }
        }
        .sheet(isPresented: $pairing) { PairView() }
        .sheet(item: Bindable(model).creating) { creating in
            switch creating {
            case let .workspace(device): NewWorkspaceSheet(device: device)
            case let .chat(device, workspace, label): NewChatSheet(device: device, workspace: workspace, label: label)
            }
        }
        .overlay(alignment: .bottom) {
            if let offer = model.nearbyOffer {
                ZStack(alignment: .bottom) {
                    Color.black.opacity(0.25).ignoresSafeArea()
                    ProximityCard(computer: offer) { model.dismissedNearby.insert(offer.id) }
                        .id(offer.id)
                }
            }
        }
        .animation(.spring(duration: 0.4), value: model.nearbyOffer?.id)
        // Bluetooth looks for computers only while pairing is likely, so
        // its permission prompt comes when the person is pairing.
        .onChange(of: pairing || model.computers.isEmpty, initial: true) { _, looking in
            if looking { model.bluetooth.start() } else { model.bluetooth.stop() }
        }
        #if DEBUG
        .task { await followLaunchArguments() }
        #endif
        .onChange(of: model.pendingRoute) { _, route in
            guard let route else { return }
            tab = .home
            homePath.append(route)
            model.pendingRoute = nil
        }
    }
}

#if DEBUG
extension RootView {
    /// For the simulator loop (`ios/README.md`), which cannot tap:
    /// `-pairLink URL` pairs without the camera (the desktop still
    /// approves), `-openLink URL` opens a Live Activity's link as a tap on
    /// the activity would, `-openTerm PANE` opens the terminal pane of
    /// whichever computer lists it, `-tab board|computers|search` picks a
    /// tab, and `-newWorkspace YES` opens the new workspace form. In a
    /// chat, `-session YES` opens its session sheet and `-draft TEXT`
    /// types into the composer.
    func followLaunchArguments() async {
        let defaults = UserDefaults.standard
        if let link = defaults.string(forKey: "pairLink"),
           !model.computers.contains(where: { (try? PairingLink(link))?.fingerprint == $0.fingerprint }) {
            let pairing = PairingModel()
            pairing.pair(link, deviceName: "Simulator") { model.add($0) }
        }
        if let text = defaults.string(forKey: "openLink"), let url = URL(string: text) {
            // Lets the first home arrive, so the chat has its title.
            try? await Task.sleep(for: .seconds(2))
            model.open(url)
        }
        switch defaults.string(forKey: "tab") {
        case "board": tab = .board
        case "computers": tab = .computers
        case "search": tab = .search
        default: break
        }
        if defaults.bool(forKey: "newWorkspace") {
            try? await Task.sleep(for: .seconds(2))
            model.creating = .workspace(device: nil)
        }
        if let pane = defaults.string(forKey: "openTerm") {
            try? await Task.sleep(for: .seconds(2))
            if let item = model.fleet.terminalItems.first(where: { $0.terminal.panes.contains { $0.pane == pane } }) {
                model.pendingRoute = .term(PaneKey(device: item.computer.device, pane: pane), title: item.terminal.label)
            }
        }
    }
}
#endif

extension View {
    func withRoutes() -> some View {
        navigationDestination(for: Route.self) { route in
            switch route {
            case let .chat(key, title): ChatScreen(key: key, title: title)
            case let .term(key, title): TerminalScreen(key: key, title: title)
            }
        }
    }
}

/// Says which computers are not reachable, and why.
struct ConnectionBanner: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let down = model.fleet.machines.compactMap { machine -> String? in
            switch machine.status {
            case let .offline(reason): "\(machine.computer.name): \(reason)"
            case .revoked: "\(machine.computer.name) unpaired this phone. Pair it again."
            case .connecting where machine.home != nil: "Reconnecting to \(machine.computer.name)…"
            default: nil
            }
        }
        if !down.isEmpty {
            VStack(alignment: .leading, spacing: 2) {
                ForEach(down, id: \.self) { Text($0) }
            }
            .font(.footnote)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 14).padding(.vertical, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glassEffect(.regular, in: .rect(cornerRadius: 16))
        }
    }
}
