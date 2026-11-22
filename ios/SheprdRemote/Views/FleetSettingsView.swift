import SheprdRemoteCore
import SwiftUI
import UIKit

/// Settings > Fleet: the members and hosts of the person's fleet, as this
/// phone's own copy of the log says them. Removing a member or a host
/// signs a fleet operation, which asks for Face ID.
struct FleetSettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var removingMember: API.FleetMember?
    @State private var removingHost: API.FleetHost?

    var body: some View {
        let membership = model.fleetMembership
        List {
            if let problem = membership.problem {
                Section { Label(problem, systemImage: "exclamationmark.triangle").font(.footnote) }
            }
            if membership.notMember {
                Section {
                    Label("Not in the fleet", systemImage: "person.crop.circle.badge.xmark")
                    Button("Forget This Fleet", role: .destructive) { membership.forgetRemoved() }
                } footer: {
                    Text("A member removed this \(UIDevice.current.model). Forgetting the fleet here signs nothing; you can then join again with a new key.")
                }
            }
            if membership.admitted, let view = membership.status?.view {
                members(view, me: membership.record?.member)
                hosts(view, status: membership.status)
                if !membership.offers.isEmpty {
                    Section {
                        Button("Add \(membership.offers.count) More Computers", systemImage: "plus") {
                            Task { await membership.link(membership.offers) }
                        }
                    } footer: {
                        Text("Each one lets this \(UIDevice.current.model) control it, through its API only.")
                    }
                }
            } else if let record = membership.record {
                Section {
                    Label("Waiting for approval", systemImage: "hourglass")
                    Button("Cancel", role: .destructive) { membership.cancelJoin() }
                } footer: {
                    let name = model.computers.first { $0.device == record.through }.map(model.displayName) ?? "your computer"
                    Text("Allow this \(UIDevice.current.model) in Sheprd on \(name), in Settings > Fleet.")
                }
            } else if let computer = membership.joinable {
                Section {
                    Button("Join \(model.displayName(computer))'s Fleet", systemImage: "person.3") {
                        Task { await membership.join(through: computer, name: UIDevice.current.name) }
                    }
                } footer: {
                    Text("Joining lets this \(UIDevice.current.model) control every computer in your fleet, and remove a lost one. It never gets SSH. You approve it in Sheprd on \(model.displayName(computer)).")
                }
            } else {
                Section {
                    Text("None of your computers is in a fleet. Create one in Sheprd's Settings > Fleet, then pair this \(UIDevice.current.model) with it.")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .disabled(membership.busy)
        .overlay { if membership.busy { ProgressView() } }
        .navigationTitle("Fleet")
        .navigationBarTitleDisplayMode(.inline)
        .refreshable { await refresh() }
        .task { await refresh() }
        .confirmationDialog(
            removingMember.map { "Remove \($0.name)?" } ?? "",
            isPresented: Binding(get: { removingMember != nil }, set: { if !$0 { removingMember = nil } }),
            titleVisibility: .visible,
            presenting: removingMember
        ) { member in
            if member.id == membership.record?.member {
                Button("Leave the Fleet", role: .destructive) { remove(member, .left) }
            } else {
                Button("It Was Lost", role: .destructive) { remove(member, .lost) }
                Button("It Is Retired", role: .destructive) { remove(member, .retired) }
                Button("It Was Compromised", role: .destructive) { remove(member, .compromised) }
            }
        } message: { _ in
            Text("Every host drops it as soon as the removal reaches it. It can only come back with a new key and a new approval.")
        }
        .confirmationDialog(
            removingHost.map { "Remove \($0.name) Everywhere?" } ?? "",
            isPresented: Binding(get: { removingHost != nil }, set: { if !$0 { removingHost = nil } }),
            titleVisibility: .visible,
            presenting: removingHost
        ) { host in
            Button("Remove Everywhere", role: .destructive) {
                Task { await membership.removeHost(host.id) }
            }
        } message: { _ in
            Text("No member of your fleet reaches it through the fleet any more.")
        }
    }

    private func members(_ view: API.FleetView, me: String?) -> some View {
        Section("Members") {
            ForEach(view.members) { member in
                HStack {
                    Image(systemName: member.kind == .phone ? "iphone" : "desktopcomputer")
                        .foregroundStyle(.secondary)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(member.name)
                        if member.id == me {
                            Text("This \(UIDevice.current.model)").font(.caption).foregroundStyle(.secondary)
                        } else if let by = member.admittedBy, let name = view.members.first(where: { $0.id == by })?.name {
                            Text("Admitted by \(name)").font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
                .swipeActions {
                    Button(member.id == me ? "Leave" : "Remove", systemImage: "person.badge.minus") {
                        removingMember = member
                    }
                    .tint(.red)
                }
            }
        }
    }

    private func hosts(_ view: API.FleetView, status: API.FleetStatus?) -> some View {
        Section {
            ForEach(view.hosts) { host in
                VStack(alignment: .leading, spacing: 2) {
                    HStack {
                        Text(host.name)
                        if status?.links.contains(host.id) == true {
                            Image(systemName: "checkmark.circle.fill").foregroundStyle(.tint)
                                .accessibilityLabel("Added to this \(UIDevice.current.model)")
                        }
                    }
                    Text(Self.path(host, status: status)).font(.caption).foregroundStyle(.secondary)
                }
                .swipeActions {
                    Button("Remove", systemImage: "trash") { removingHost = host }.tint(.red)
                }
            }
        } header: {
            Text("Hosts")
        } footer: {
            Text("Hosts with a check are on this \(UIDevice.current.model). Plain SSH hosts are only for your computers.")
        }
    }

    /// What runs on a host and how it is reached, in a few words.
    private static func path(_ host: API.FleetHost, status: API.FleetStatus?) -> String {
        let kind = switch host.kind {
        case .member: "Computer"
        case .sheprd: "Sheprd host"
        case .agent: "SSH host"
        }
        if host.held { return "\(kind) · Held until someone picks its keys" }
        if status?.origins(of: host.id).isEmpty == false { return "\(kind) · Direct" }
        let relays = host.relays.count
        return relays == 0 ? "\(kind) · Not yet reached" : "\(kind) · Relay (\(relays == 1 ? "1 relay" : "\(relays) relays"))"
    }

    private func remove(_ member: API.FleetMember, _ reason: FleetLog.Reason) {
        Task { await model.fleetMembership.remove(member: member.id, reason: reason) }
    }

    private func refresh() async {
        let membership = model.fleetMembership
        if let through = membership.through {
            await membership.check(through)
        } else {
            for computer in model.computers where computer.via == nil {
                await membership.check(computer)
            }
        }
    }
}
