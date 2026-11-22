import Foundation
import LocalAuthentication
import Observation
import SheprdFleetLog
import SheprdRemoteCore
import UIKit
import os

private let log = Logger(subsystem: "so.pen.herdr.remote", category: "fleet-membership")

/// Why the phone kept nothing of a change to its copy.
enum FleetMembershipError: Error {
    /// The Keychain record could not be written, so neither was the copy.
    case notKept
}

/// This phone as a member of the person's fleet (`docs/relay-plan.md`,
/// Fleet, phase 5): joining through a paired computer, which a person then
/// admits in Sheprd there; keeping its own copy of the fleet log, synced
/// with any computer of the fleet it reaches and checked by the same rules
/// every computer runs (`SheprdFleetLog`); and signing what the phone may
/// sign, each batch with Face ID: removals of members and hosts, and one
/// `Link` per Sheprd host it adds, which those hosts admit to their API
/// only. It drafts on its own copy and refuses, before Face ID, whatever
/// that copy would refuse, so no computer's word decides what it signs.
@MainActor
@Observable
final class FleetMembership {
    /// What the phone keeps about its membership, in the Keychain.
    struct Record: Codable, Equatable {
        var keyKind: Computer.KeyKind
        /// The Secure Enclave's blob for the fleet key.
        var keyData: Data
        /// The member id, hex.
        var member: String
        var fleet: String
        /// The computer it last synced with, tried first.
        var through: String
        /// A person admitted it; until then the join waits on `through`.
        var admitted: Bool
        /// The private halves of the relay keys its bundle published (dial,
        /// then tunnel): every host it links lets these through its relays,
        /// and each linked computer's relay record carries them. Kept in the
        /// Keychain as other relay keys are.
        var relayDial: Data
        var relayTunnel: Data
        /// Device keys made for hosts whose `Link` may not have landed yet.
        var pendingLinks: [FleetPendingLink]?
        /// Operations this phone signed that its saved copy may not hold
        /// (base64url): signed before it kept a copy, or kept here with
        /// their sequence numbers because the copy could not be written.
        /// Taken into the copy at the next sync, or the next signature, and
        /// dropped once a copy holding them is on disk.
        var unconfirmed: [String]?
        /// The highest sequence number this phone ever signed, kept apart
        /// from its copy: nothing is signed at or below it, so a copy that
        /// lost some of the phone's own operations (damaged, or started
        /// again) can never make it sign one number twice.
        var signedSeq: UInt64?
    }

    private(set) var record: Record?
    /// This phone's own copy of the log, once it holds one.
    private(set) var replica: FleetReplica?
    /// What a computer last said, while this phone waits to join or has no
    /// copy yet: only ever a reason to sync, never what it signs on.
    private var joinStatus: API.FleetStatus?
    /// The fleet as this phone's own copy says it once it has one; before,
    /// what a computer said about the join.
    var status: API.FleetStatus? { replica?.status ?? (admitted ? nil : joinStatus) }
    /// The paired computers in a fleet, by device, with the fleet's id.
    private(set) var fleets: [String: String] = [:]
    private(set) var busy = false
    /// The last failure, to show until the next attempt.
    var problem: String?
    /// The offer card was closed this launch.
    var offerDismissed = false
    /// The phone's own copy of the log says another member removed it. The
    /// phone can then forget the fleet and join again.
    var notMember: Bool { replica?.removed == true }
    /// The computers on this phone.
    @ObservationIgnored var computers: () -> [Computer] = { [] }
    /// Keeps a host the phone linked itself to; whether it was saved.
    @ObservationIgnored var onLinked: (Computer) -> Bool = { _ in false }
    @ObservationIgnored private var asked: [String: Date] = [:]
    @ObservationIgnored private var waiting: Task<Void, Never>?
    /// Every change to the copy runs in turn: a sync that started before an
    /// operation was signed must never save a copy without it, as the
    /// next draft would then reuse its sequence number.
    @ObservationIgnored private var queue: Task<Void, Never>?
    /// The saved copy failed its own checks: the next sync starts again
    /// from a computer, with `salvaged`, this phone's own operations read
    /// back from the file, taken in and checked again.
    @ObservationIgnored private var damaged = false
    @ObservationIgnored private var salvaged: [String] = []
    private let sessions: Sessions
    private let store: FleetLogStore
    /// The paired computers' live links, which fleet calls go through.
    @ObservationIgnored weak var links: Fleet?

    private static let account = "fleet"
    /// How often one computer is asked when its link comes online.
    private static let interval: TimeInterval = 60
    /// How long a join is followed: the daemon keeps it ten minutes.
    private static let patience: Duration = .seconds(10 * 60)

    init(sessions: Sessions, store: FleetLogStore = .shared) {
        self.sessions = sessions
        self.store = store
        record = (try? Keychain.read(Self.account)).flatMap { $0 }
            .flatMap { try? JSONDecoder().decode(Record.self, from: $0) }
        if record != nil { Task { await serialized { await self.load() } } }
    }

    var admitted: Bool { record?.admitted == true }

    /// A paired computer in a fleet, while this phone is in none.
    var joinable: Computer? {
        guard record == nil else { return nil }
        return computers().first { $0.via == nil && fleets[$0.device] != nil }
    }

    /// The computer the phone reads the fleet from and posts to.
    var through: Computer? { candidates.first }

    /// The computers of this phone's fleet it may sync with: the one it
    /// last reached first, then every other one, paired directly or linked
    /// through the fleet, so a computer that is offline, or the one that
    /// introduced the phone and is gone, never blocks the rest.
    private var candidates: [Computer] {
        guard let record else { return [] }
        let hosts = Set(replica?.status.view?.hosts.map { $0.id.lowercased() } ?? [])
        let inFleet = computers().filter { computer in
            hosts.contains(computer.fingerprint.hex.lowercased())
                || (computer.via == nil && (fleets[computer.device] == record.fleet || computer.device == record.through))
        }
        return inFleet.filter { $0.device == record.through } + inFleet.filter { $0.device != record.through }
    }

    /// Sheprd hosts in the fleet the phone may add: not linked yet, not
    /// held, and not a computer it already holds.
    var offers: [API.FleetHost] {
        guard admitted, !notMember, let status = replica?.status, let view = status.view else { return [] }
        let held = Set(computers().map(\.fingerprint.hex))
        return view.hosts.filter { host in
            host.runsSheprd && !host.held && !status.links.contains(host.id)
                && !held.contains(host.id.uppercased())
        }
    }

    /// Syncs with `computer` when its link comes online, at most once a
    /// minute: a paired computer, or one of the fleet's linked ones.
    func online(_ computer: Computer) {
        guard computer.via == nil || candidates.contains(where: { $0.device == computer.device }) else { return }
        if let last = asked[computer.device], Date.now.timeIntervalSince(last) < Self.interval { return }
        asked[computer.device] = .now
        Task { await check(computer) }
    }

    /// Asks `computer` about its fleet and, for this phone's, syncs the
    /// phone's copy with it; whether it answered about this phone's fleet.
    /// What it says only decides whether to sync: membership, removal, and
    /// what to sign come from the copy, checked here.
    @discardableResult
    func check(_ computer: Computer) async -> Bool {
        do {
            let member = record.flatMap { FleetLog.data(hex: $0.member) }
            let status = try await client(computer).status(member: member)
            fleets[computer.device] = status.view?.fleet
            // The membership may have changed while the read was out (a join
            // cancelled and made again with a new key): an answer about the
            // key it asked for says nothing about the one held now.
            guard status.isAbout(member: self.record?.member) else { return false }
            guard let record, status.view?.fleet == record.fleet else {
                if self.record == nil { joinStatus = status }
                return false
            }
            joinStatus = status
            if record.admitted || status.join == .member || replica != nil {
                await serialized { _ = await self.syncNow(with: computer) }
            }
            return true
        } catch {
            // A daemon from before the fleet answers 404 here.
            log.info("\(computer.name, privacy: .public) gave no fleet: \(error.localizedDescription, privacy: .public)")
            return false
        }
    }

    /// Asks to join `computer`'s fleet: makes this phone's fleet key and
    /// signs its bundle, which Face ID confirms; a person then admits it
    /// in Sheprd on that computer.
    func join(through computer: Computer, name: String) async {
        guard record == nil, let fleet = fleets[computer.device] else { return }
        busy = true
        problem = nil
        defer { busy = false }
        let key: FleetKey
        let bundle: FleetLog.Bundle
        do {
            key = try FleetKey.create()
            let relay = RelayKeyPair()
            bundle = try FleetLog.phoneBundle(key: key.publicKey, name: name, relay: relay)
            // Kept before the join is sent: if its answer is lost, the
            // daemon may hold a join for this key, and only this key can
            // become that member.
            save(Record(
                keyKind: key.kind, keyData: key.data, member: bundle.id.hexString, fleet: fleet,
                through: computer.device, admitted: false,
                relayDial: relay.dialKey, relayTunnel: relay.tunnelKey
            ))
        } catch {
            problem = Self.describe(error)
            return
        }
        guard record != nil else { return }
        do {
            let joined = try await client(computer).join(bundle, fleet: fleet, as: key.signer)
            if joined.state == .member, var admitted = record {
                admitted.admitted = true
                save(admitted)
            }
            follow(computer)
        } catch {
            log.error("fleet: join through \(computer.name, privacy: .public): \(error.localizedDescription, privacy: .public)")
            problem = Self.describe(error)
            if fleetRequestMayHaveLanded(status: Self.status(of: error)) {
                // No answer: the join may be waiting there. Keep the key and
                // follow it; Cancel forgets it.
                follow(computer)
            } else {
                forget()
            }
        }
    }

    /// Stops waiting for a join, and forgets its key.
    func cancelJoin() {
        guard record?.admitted == false else { return }
        waiting?.cancel()
        forget()
    }

    /// Forgets the fleet on this phone alone, once a computer of the fleet
    /// says the phone is no longer a member: nothing is signed, as a
    /// removed key may sign nothing, and joining again makes a new key.
    func forgetRemoved() {
        guard notMember else { return }
        forget()
    }

    /// Removes a member, this phone included, which then leaves.
    func remove(member: String, reason: FleetLog.Reason) async {
        guard let id = FleetLog.data(hex: member) else { return }
        let leaving = member == record?.member
        let posted = await sign("Remove this member from your fleet", [
            .remove(member: id.hexString, reason: leaving ? .left : reason),
        ])
        // Left once a computer took it, whatever failed before it.
        if leaving, case .taken = posted { forget() }
    }

    /// Removes a host from the whole fleet.
    func removeHost(_ host: String) async {
        guard let id = FleetLog.data(hex: host) else { return }
        await sign("Remove this host from your fleet", [.hostRemove(host: id.hexString)])
    }

    /// Signs one `Link` per host, with a new device key for each, so those
    /// hosts admit this phone to their API; keeps each as a computer once
    /// the phone's copy holds its link. The keys are kept before signing,
    /// so a link that lands loses none of them: the next sync finishes them.
    func link(_ hosts: [API.FleetHost]) async {
        guard var record, record.admitted else { return }
        let reason = hosts.count == 1 ? "Add \(hosts[0].name)" : "Add \(hosts.count) computers"
        var made: [FleetPendingLink] = []
        // Hosts whose key was made just now: only theirs may go on a
        // refusal. A key reused from an earlier post waits for that post.
        var fresh: [String] = []
        do {
            for host in hosts where FleetLog.data(hex: host.id) != nil {
                // A key still waiting for its link is used again: its first
                // post may yet land, and a new key would orphan it.
                if let waiting = FleetPendingLink.waiting(for: host.id, in: record.pendingLinks ?? []) {
                    made.append(waiting)
                    continue
                }
                let key = try DeviceKey.create()
                fresh.append(host.id)
                made.append(FleetPendingLink(
                    host: host.id, name: host.name, keyKind: key.kind, keyData: key.data,
                    publicKey: DeviceKeyEncoding.publicKey(key.publicKey), point: key.publicKey.x963Representation
                ))
            }
        } catch {
            problem = error.localizedDescription
            return
        }
        record.pendingLinks = FleetPendingLink.merge(record.pendingLinks ?? [], made)
        save(record)
        guard self.record == record else { return }
        let bodies: [FleetDraft] = made.compactMap { pending in
            FleetLog.data(hex: pending.host).map { .link(host: $0.hexString, deviceKey: pending.point) }
        }
        let posted = await sign(reason, bodies)
        guard let status = replica?.status else { return }
        // A link's key goes only when the link surely never landed: nothing
        // was signed, or a computer refused it and the phone's own copy,
        // synced with it, does not hold it.
        switch posted {
        case .nothing, .refused:
            settleLinks(status, refused: fresh)
        case .kept, .taken, .unknown:
            settleLinks(status)
        }
    }

    /// Keeps as computers the pending links the fleet now holds, and drops
    /// those of `refused` hosts it does not (`FleetPendingLink.settle`).
    private func settleLinks(_ status: API.FleetStatus, refused: [String] = []) {
        guard var record, let pending = record.pendingLinks, !pending.isEmpty,
              let through = through
        else { return }
        // Kept as reached through the computer the phone syncs with first.
        let (done, settled) = FleetPendingLink.settle(pending, linked: status.linkedKeys, refused: refused)
        // A link whose computer could not be saved keeps its key pending.
        let unsaved = FleetPendingLink.unsaved(done) { link in
            guard let device = FleetLog.linkDevice(key: link.point), let fingerprint = Fingerprint(link.host) else {
                return true
            }
            let enrolled = HostState.Enrolled(
                device: device, computer: link.name, fingerprint: fingerprint,
                origins: status.origins(of: link.host)
            )
            var computer = Computer(host: enrolled, key: link.key, via: through.fingerprint)
            computer.relay = relayKeys.flatMap { status.relayRecord(for: link.host, keys: $0) }
            return onLinked(computer)
        }
        let waiting = settled + unsaved
        record.pendingLinks = waiting.isEmpty ? nil : waiting
        if record != self.record { save(record) }
    }

    /// The relay keys of this phone's fleet bundle: the ones every host it
    /// linked lets through.
    private var relayKeys: RelayKeyPair? {
        record.map { RelayKeyPair(dialKey: $0.relayDial, tunnelKey: $0.relayTunnel) }
    }

    /// Gives each computer this phone linked the relays its host's own
    /// signed record names (`FleetRoute.verified`), when it has none or
    /// the record is newer than the one its relays came from: a host that
    /// changed relays is followed, and an older record served again is
    /// refused.
    private func route(_ status: API.FleetStatus) {
        guard let keys = relayKeys else { return }
        for computer in computers() where computer.via != nil && computer.device.hasPrefix("fleet-") {
            let host = computer.fingerprint.hex.lowercased()
            guard status.links.contains(host),
                  let relay = status.relayRecord(for: host, keys: keys, replacing: computer.relay)
            else { continue }
            var routed = computer
            routed.relay = relay
            _ = onLinked(routed)
        }
    }

    // MARK: The phone's copy

    /// Runs `work` after every change to the copy queued before it.
    private func serialized<T: Sendable>(_ work: @escaping @MainActor () async -> T) async -> T {
        let previous = queue
        let task = Task { @MainActor in
            await previous?.value
            return await work()
        }
        queue = Task { _ = await task.value }
        return await task.value
    }

    /// Opens the saved copy, checking it again, off the main thread. Only
    /// this, never a computer's answer, may find the copy damaged.
    private func load() async {
        guard let record, replica == nil, !damaged, case let .bytes(saved) = store.read() else { return }
        let fleet = record.fleet, me = record.member
        do {
            let opened = try await Self.background { try FleetReplica.open(fleet: fleet, me: me, log: saved, at: .now) }
            // It may lack what the record keeps; the next sync or signature takes that in.
            try adopt(opened, waiting: nil)
        } catch {
            damaged = true
            salvaged = (try? await Self.background { try FleetReplica.salvage(me: me, log: saved) }) ?? []
            log.error("fleet: the saved copy does not check out; \(self.salvaged.count) of this phone's operations kept")
        }
    }

    /// Syncs the copy with `computer`: takes what it has, hands it what the
    /// phone has and it lacks, the phone's own operations among them.
    /// Nil when it could not be reached, answered badly, or the copy could
    /// not be kept; the copy then stays as it was.
    private func syncNow(with computer: Computer) async -> FleetLogSync.Outcome? {
        guard let record else { return nil }
        if replica == nil {
            switch store.read() {
            // A copy is there but locked away: one rebuilt from a computer
            // could lack this phone's own operations not yet handed over.
            case .unavailable: return nil
            case .bytes where !damaged:
                await load()
                guard replica != nil || damaged else { return nil }
            case .bytes, .none: break
            }
        }
        let client = client(computer)
        let fleet = record.fleet, me = record.member
        do {
            var outcome = try await FleetLogSync.run(fleet: fleet, me: me, replica: replica, with: client)
            // What the phone signed that its copy lost or never held (signed
            // before it kept one) joins it, checked again, and goes over.
            // Those still waiting for history this computer lacks stay in
            // the record, for a sync with one that has it.
            let carried = salvaged + (record.unconfirmed ?? [])
            var waiting: [String] = []
            if !carried.isEmpty {
                let synced = outcome.replica
                let merged = try await Self.background { try synced.carrying(carried, at: .now) }
                waiting = merged.waiting
                outcome = try await FleetLogSync.run(fleet: fleet, me: me, replica: merged.replica, with: client)
            }
            try adopt(outcome.replica, waiting: waiting, through: computer)
            damaged = false
            salvaged = []
            return outcome
        } catch {
            log.info("fleet: no sync with \(computer.name, privacy: .public): \(error.localizedDescription, privacy: .public)")
            return nil
        }
    }

    /// Keeps `new` as the copy: written before anything relies on it. The
    /// record goes first: the highest sequence number the copy says this
    /// phone signed, and the operations just `signed`, are in the Keychain
    /// before the copy holding them is on disk, let alone handed to a
    /// computer. Should the copy then fail to write, the record still holds
    /// them, and the next sync or signature takes them in again, so no
    /// number is burned on an operation nobody kept. `waiting` are the
    /// phone's own operations `new` lacks, as they wait for history it does
    /// not hold yet: they stay in the record, as do the rest once `new` is
    /// written. Nil when `new` may lack what the record keeps, which then
    /// stays. Throws when either cannot be written.
    private func adopt(
        _ new: FleetReplica, signed: [String] = [], waiting: [String]? = [], through computer: Computer? = nil
    ) throws {
        guard var record, new.fleet == record.fleet, new.me == record.member else { return }
        if new.status.join == .member { record.admitted = true }
        if let computer { record.through = computer.device }
        // The copy holds the phone's own operations up to `next - 1`.
        if let next = new.status.next, next.seq > 1 {
            record.signedSeq = max(record.signedSeq ?? 0, next.seq - 1)
        }
        // Salvaged ones among `waiting` are only in the damaged file this
        // write replaces: the record takes them first.
        let kept = record.unconfirmed ?? []
        let joined = kept + (waiting ?? []).filter { !kept.contains($0) } + signed
        record.unconfirmed = joined.isEmpty ? nil : joined
        if record != self.record, !save(record) { throw FleetMembershipError.notKept }
        try store.write(new.log)
        replica = new
        // The copy on disk holds the rest now (a sync or signature takes
        // them in first). A failed save here only means taking them in
        // again, which changes nothing.
        if let waiting, record.unconfirmed != (waiting.isEmpty ? nil : waiting) {
            record.unconfirmed = waiting.isEmpty ? nil : waiting
            _ = save(record)
        }
        route(new.status)
        settleLinks(new.status)
    }

    /// Takes into the copy what this phone signed that its saved copy may
    /// not hold, without a computer: kept in the record because the copy
    /// could not be written. Runs before drafting, so the next draft
    /// follows them.
    private func carryKept() async throws {
        guard let carried = record?.unconfirmed, !carried.isEmpty, let current = replica else { return }
        let merged = try await Self.background { try current.carrying(carried, at: .now) }
        try adopt(merged.replica, waiting: merged.waiting)
    }

    /// How signing and posting ended.
    private enum Posted {
        /// Nothing was signed.
        case nothing
        /// A computer took every operation.
        case taken
        /// Signed and kept in the record, but the copy could not be
        /// written: the next sync or signature takes them in and hands
        /// them over, so they may still land.
        case kept
        /// A computer refused some.
        case refused
        /// No computer answered: the copy keeps them, and the next sync with
        /// any computer hands them over.
        case unknown
    }

    /// Signs `bodies` on the phone's own copy, after syncing it with the
    /// first computer that answers, and hands them to the first computer
    /// that takes them. Refused, before Face ID, when the copy would refuse
    /// them.
    @discardableResult
    private func sign(_ reason: String, _ bodies: [FleetDraft]) async -> Posted {
        await serialized { await self.signNow(reason, bodies) }
    }

    private func signNow(_ reason: String, _ bodies: [FleetDraft]) async -> Posted {
        guard let record, record.admitted, !bodies.isEmpty else { return .nothing }
        busy = true
        problem = nil
        defer { busy = false }
        // Drafted on the newest heads a computer has, when one answers.
        for computer in candidates where await syncNow(with: computer) != nil { break }
        do {
            try await carryKept()
        } catch {
            problem = Self.describe(error)
            return .nothing
        }
        guard let current = replica else {
            problem = "This \(UIDevice.current.model) has no copy of the fleet yet. Connect to one of its computers first."
            return .nothing
        }
        let drafts: [FleetDrafted]
        do {
            drafts = try await Self.background { try current.draft(bodies, issued: .now, at: .now) }
        } catch {
            problem = Self.describe(error)
            return .nothing
        }
        // A copy missing some of what this phone signed would reuse a
        // sequence number, which the fleet takes for a stolen key.
        guard let first = drafts.first, first.seq > (self.record?.signedSeq ?? record.signedSeq ?? 0) else {
            problem = "This \(UIDevice.current.model)'s copy of the fleet lacks something it signed. Connect to a computer of your fleet that has it, then try again."
            return .nothing
        }
        let key: FleetKey
        do {
            key = try await Self.open(record, reason: reason)
        } catch {
            problem = Self.describe(error)
            return .nothing
        }
        let sealed: (ops: [String], replica: FleetReplica)
        do {
            let signer = key.signer
            sealed = try await Self.background { try current.signing(drafts, with: signer.sign, at: .now) }
        } catch {
            problem = Self.describe(error)
            return .nothing
        }
        do {
            // Kept before it is handed over, so the next draft follows it.
            // What still waits for history stays in the record.
            try adopt(sealed.replica, signed: sealed.ops, waiting: self.record?.unconfirmed ?? [])
        } catch {
            problem = Self.describe(error)
            // The record took them, though the copy did not: they still go.
            let kept = self.record?.unconfirmed ?? []
            return sealed.ops.allSatisfy(kept.contains) ? .kept : .nothing
        }
        for computer in candidates {
            guard let outcome = await syncNow(with: computer) else { continue }
            guard outcome.refused == 0 else {
                problem = Self.describe(FleetClientError.refused(outcome.refused))
                return .refused
            }
            // Synced, but its copy still lacks what the phone just signed:
            // held waiting there, say. Another computer may take it.
            guard outcome.undelivered else { return .taken }
        }
        problem = "Signed. It goes to your fleet the next time this \(UIDevice.current.model) reaches one of its computers."
        return .unknown
    }

    private static func background<T: Sendable>(_ work: @escaping @Sendable () throws -> T) async throws -> T {
        try await Task.detached(priority: .userInitiated) { try work() }.value
    }

    /// Opens the fleet key with one Face ID for `reason`, so a batch of
    /// signatures asks once. The simulator's software key asks nothing.
    private static func open(_ record: Record, reason: String) async throws -> FleetKey {
        guard record.keyKind == .secureEnclave else { return try FleetKey(kind: record.keyKind, data: record.keyData) }
        let context = LAContext()
        context.localizedReason = reason
        try await context.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason)
        return try FleetKey(kind: record.keyKind, data: record.keyData, context: context)
    }

    /// Follows a join until a person admits it, or the daemon forgets it.
    private func follow(_ computer: Computer) {
        waiting?.cancel()
        waiting = Task {
            let deadline = ContinuousClock.now + Self.patience
            while !Task.isCancelled, ContinuousClock.now < deadline, record?.admitted == false {
                try? await Task.sleep(for: .seconds(3))
                await check(computer)
            }
        }
    }

    /// The fleet calls of `computer`, through its live connection when it
    /// has one (`Fleet.call`, relay included), else by signing in at its
    /// saved addresses, as a computer just paired, with no link yet, needs.
    private func client(_ computer: Computer) -> FleetClient {
        let sessions = sessions
        let links = links
        let device = computer.device
        return FleetClient.preferring(live: { @MainActor in
            guard let links, case .online = links.status(of: device) else { return nil }
            return { method, path, body in
                try await links.call(device) { client, token in
                    try await client.fleetCall(method, path, body: body, bearer: token)
                }
            }
        }, fallback: { method, path, body in
            let (client, token) = try await sessions.session(for: computer)
            return try await client.fleetCall(method, path, body: body, bearer: token)
        })
    }

    /// Saves `record` to the Keychain; whether that landed. Only a saved
    /// record replaces the one in memory.
    @discardableResult
    private func save(_ record: Record) -> Bool {
        do {
            try Keychain.write(JSONEncoder().encode(record), account: Self.account)
            self.record = record
            return true
        } catch {
            problem = error.localizedDescription
            return false
        }
    }

    private func forget() {
        try? Keychain.delete(Self.account)
        store.delete()
        record = nil
        replica = nil
        joinStatus = nil
    }

    /// The HTTP status a computer answered with, when it answered.
    private static func status(of error: Error) -> Int? {
        if case let RemoteError.status(code, _) = error { return code }
        return nil
    }

    private static func describe(_ error: Error) -> String {
        switch error {
        case FleetLogError.refused(rule: "signer"): "This \(UIDevice.current.model) is no longer a member of the fleet."
        case FleetLogError.refused(rule: "unknown"): "That member is no longer in the fleet."
        case FleetLogError.refused: "Your fleet would refuse it, so nothing was signed."
        case FleetMembershipError.notKept: "This \(UIDevice.current.model) could not save it, so nothing was sent."
        case FleetClientError.noFleet: "That computer is no longer in a fleet."
        case FleetClientError.notMember: "This \(UIDevice.current.model) is not a member of that fleet yet."
        case let FleetClientError.refused(count): "The fleet refused \(count == 1 ? "it" : "\(count) of them")."
        case RemoteError.status(429, _): "Too many devices are waiting to join. Try again in a few minutes."
        case RemoteError.status(404, _): "That computer's Sheprd does not know fleets yet. Update it first."
        default: error.localizedDescription
        }
    }
}

extension FleetMembership.Record {
    /// Reads a record of any version: one saved before the phone kept its
    /// own copy of the log carries what it signed into `unconfirmed`, and
    /// the highest of its numbers that may stand into `signedSeq`
    /// (`FleetEarlierLine`), so an update neither drops an operation the
    /// fleet has not seen nor signs another on its number.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        keyKind = try c.decode(Computer.KeyKind.self, forKey: .keyKind)
        keyData = try c.decode(Data.self, forKey: .keyData)
        member = try c.decode(String.self, forKey: .member)
        fleet = try c.decode(String.self, forKey: .fleet)
        through = try c.decode(String.self, forKey: .through)
        admitted = try c.decode(Bool.self, forKey: .admitted)
        relayDial = try c.decode(Data.self, forKey: .relayDial)
        relayTunnel = try c.decode(Data.self, forKey: .relayTunnel)
        pendingLinks = try c.decodeIfPresent([FleetPendingLink].self, forKey: .pendingLinks)
        let unconfirmed = try c.decodeIfPresent([String].self, forKey: .unconfirmed)
        let signedSeq = try c.decodeIfPresent(UInt64.self, forKey: .signedSeq)
        let earlier = (try? FleetEarlierLine(from: decoder)) ?? FleetEarlierLine()
        let (carried, seq) = earlier.carried(member: member, unconfirmed: unconfirmed)
        self.unconfirmed = carried.isEmpty ? nil : carried
        self.signedSeq = [signedSeq, seq].compactMap { $0 }.max()
    }
}
