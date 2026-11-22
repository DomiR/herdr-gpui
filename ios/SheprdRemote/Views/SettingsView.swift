import SheprdRemoteCore
import SwiftUI
import UIKit

/// Settings: a short hub of pages, as the system's is. What repeats per
/// computer (alerts at the desk, push delivery, test pushes) lives on each
/// computer's page, and a problem shows once, on top.
struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @AppStorage(PushRelaySetting.key, store: PushRelaySetting.store) private var pushRelay = true
    @AppStorage(AppAppearance.key) private var appearance = AppAppearance.system
    #if DEBUG
    @State private var showingComputers = false
    @State private var showingRelays = false
    @State private var showingGitHub = false
    @State private var showingNotifications = false
    #endif

    var body: some View {
        List {
            // The relay's error comes first and opens its controls. A
            // registration copies it into `tokens.lastError`, so the last
            // problem shows only when it says something else.
            let relayError = pushRelay ? model.tokens.pushRelay.lastError : nil
            let problem = (model.tokens.lastError ?? model.message).flatMap { $0 == relayError ? nil : $0 }
            if relayError != nil || problem != nil {
                Section {
                    if let relayError {
                        NavigationLink {
                            NotificationSettingsView().makesRoomForConnectionToast()
                        } label: {
                            ProblemRow(title: "Push relay refused this \(UIDevice.current.model)", detail: relayError)
                        }
                    }
                    if let problem {
                        ProblemRow(title: "Last problem", detail: problem)
                    }
                }
            }
            Section {
                SettingsLink("Computers", symbol: "desktopcomputer", color: .blue, value: "\(model.computers.count)") {
                    ComputersSettingsView()
                }
                SettingsLink("Fleet", symbol: "point.3.connected.trianglepath.dotted", color: .indigo, value: fleetSummary) {
                    FleetSettingsView()
                }
                SettingsLink("Relays", symbol: "antenna.radiowaves.left.and.right", color: .teal,
                             value: "\(model.savedRelays.relays.count)") {
                    RelaysView()
                }
                GitHubSettingsRow()
            }
            Section {
                SettingsLink("Notifications", symbol: "bell.badge.fill", color: .red) {
                    NotificationSettingsView()
                }
                SettingsLink("Chats", symbol: "bubble.left.and.bubble.right.fill", color: .green) {
                    ChatSettingsView()
                }
                SettingsLink("Voice", symbol: "waveform", color: .pink) {
                    VoiceSettingsView()
                }
                SettingsLink("Terminal Keys", symbol: "terminal.fill", color: .gray,
                             value: "\(model.termKeys.layout.top.count + model.termKeys.layout.bottom.count)") {
                    TerminalKeysView()
                }
                SettingsLink("Sounds & Haptics", symbol: "speaker.wave.2.fill", color: .orange) {
                    SoundsSettingsView()
                }
            }
            Section {
                Picker(selection: $appearance) {
                    ForEach(AppAppearance.allCases) { Text($0.title).tag($0) }
                } label: {
                    Label { Text("Appearance") } icon: { SettingsIcon(symbol: "circle.lefthalf.filled", color: .blue) }
                }
                .pickerStyle(.menu)
                SettingsLink("Privacy & Storage", symbol: "hand.raised.fill", color: .blue) {
                    PrivacySettingsView()
                }
            } footer: {
                Text("Terminals stay dark in either appearance.")
            }
            Section {
                LabeledContent("Version", value: Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "")
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        #if DEBUG
        .navigationDestination(isPresented: $showingComputers) { ComputersSettingsView().makesRoomForConnectionToast() }
        .task { if UserDefaults.standard.bool(forKey: "editComputers") { showingComputers = true } }
        .navigationDestination(isPresented: $showingRelays) { RelaysView().makesRoomForConnectionToast() }
        .task { if UserDefaults.standard.bool(forKey: "openRelays") { showingRelays = true } }
        .navigationDestination(isPresented: $showingGitHub) { GitHubSettingsView().makesRoomForConnectionToast() }
        .task { if UserDefaults.standard.bool(forKey: "openGitHub") { showingGitHub = true } }
        .navigationDestination(isPresented: $showingNotifications) { NotificationSettingsView().makesRoomForConnectionToast() }
        .task { if UserDefaults.standard.bool(forKey: "openNotifications") { showingNotifications = true } }
        #endif
    }

    /// Where this phone stands with the fleet, beside its row.
    private var fleetSummary: String {
        let membership = model.fleetMembership
        if membership.admitted {
            let count = membership.status?.view?.members.count ?? 0
            return count == 1 ? "1 member" : "\(count) members"
        }
        return membership.record == nil ? "Not joined" : "Waiting"
    }
}

/// A row icon in a colored rounded square, as the system's Settings draws.
struct SettingsIcon: View {
    let symbol: String
    let color: Color

    var body: some View {
        Image(systemName: symbol)
            .font(.system(size: 14, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: 28, height: 28)
            .background(color, in: RoundedRectangle(cornerRadius: 7))
            .accessibilityHidden(true)
    }
}

/// One of the hub's rows: an icon, a title, a value, and the page it opens.
private struct SettingsLink<Destination: View>: View {
    let title: LocalizedStringKey
    let symbol: String
    let color: Color
    var value: String?
    @ViewBuilder let destination: () -> Destination

    init(_ title: LocalizedStringKey, symbol: String, color: Color, value: String? = nil,
         @ViewBuilder destination: @escaping () -> Destination) {
        self.title = title
        self.symbol = symbol
        self.color = color
        self.value = value
        self.destination = destination
    }

    var body: some View {
        NavigationLink {
            destination().makesRoomForConnectionToast()
        } label: {
            LabeledContent {
                if let value { Text(verbatim: value) }
            } label: {
                Label { Text(title) } icon: { SettingsIcon(symbol: symbol, color: color) }
            }
        }
    }
}

/// The one problem Settings shows, above everything else.
private struct ProblemRow: View {
    let title: String
    let detail: String

    var body: some View {
        Label {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                Text(detail).font(.footnote).foregroundStyle(.secondary)
            }
        } icon: {
            SettingsIcon(symbol: "exclamationmark.triangle.fill", color: .orange)
        }
    }
}
