import Foundation

/// A device key this phone made for one fleet host and is linking itself
/// to it with: kept in the Keychain from before the `Link` is posted, so a
/// reply that never arrives cannot lose the key of a link that landed.
public struct FleetPendingLink: Codable, Equatable, Sendable {
    /// The host's id, hex.
    public let host: String
    public let name: String
    public let keyKind: Computer.KeyKind
    /// The device key, as a pairing record keeps it.
    public let keyData: Data
    /// Its public half, as a claim encodes it.
    public let publicKey: String
    /// Its public half, SEC1 uncompressed, as the `Link` names it.
    public let point: Data

    public init(host: String, name: String, keyKind: Computer.KeyKind, keyData: Data, publicKey: String, point: Data) {
        self.host = host
        self.name = name
        self.keyKind = keyKind
        self.keyData = keyData
        self.publicKey = publicKey
        self.point = point
    }

    public var key: HostKey { HostKey(publicKey: publicKey, kind: keyKind, data: keyData) }

    /// Splits `pending` against the links the fleet says this phone holds,
    /// by host, with the device key each names: a link is done, to keep as
    /// a computer, only when it names the very key made for it here; a link
    /// to the same host with another key is an earlier one, and this one
    /// may still land. Of the rest, only those in `refused` go: hosts whose
    /// links were never posted, or that the computer that took the post
    /// refused, with a fresh read from that same computer not showing them.
    /// A link missing from any other read may only not have arrived there
    /// yet (a computer that lags holds it waiting for its parents), so its
    /// key stays.
    public static func settle(
        _ pending: [FleetPendingLink], linked: [String: Data], refused: [String] = []
    ) -> (done: [FleetPendingLink], waiting: [FleetPendingLink]) {
        let refused = Set(refused.map { $0.lowercased() })
        let landed = { (link: FleetPendingLink) in linked[link.host.lowercased()] == link.point }
        let done = pending.filter(landed)
        let waiting = pending.filter { !landed($0) && !refused.contains($0.host.lowercased()) }
        return (done, waiting)
    }

    /// The links of `done` whose computer could not be kept: `keep` saves
    /// one and answers whether that landed. Those stay pending, private key
    /// and all, so a failed save never loses the only key a landed link
    /// names; the next settle tries again.
    public static func unsaved(_ done: [FleetPendingLink], keep: (FleetPendingLink) -> Bool) -> [FleetPendingLink] {
        done.filter { !keep($0) }
    }

    /// The pending link for `host`, when one waits: a key made for it
    /// before, whose link may yet land, is used again rather than replaced.
    public static func waiting(for host: String, in pending: [FleetPendingLink]) -> FleetPendingLink? {
        pending.first { $0.host.lowercased() == host.lowercased() }
    }

    /// `pending` with `fresh` in place of any for the same hosts: a key made
    /// again for a host replaces the one before, which never landed or
    /// whose link the new one supersedes.
    public static func merge(_ pending: [FleetPendingLink], _ fresh: [FleetPendingLink]) -> [FleetPendingLink] {
        let hosts = Set(fresh.map { $0.host.lowercased() })
        return pending.filter { !hosts.contains($0.host.lowercased()) } + fresh
    }
}
