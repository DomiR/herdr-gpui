import Foundation

/// What a membership record saved before the phone kept its own copy of
/// the log says about the operations it signed, read so an update loses
/// none of them: its `lastSigned` operation and its `chain`, the line of
/// operations it posted through one computer at a time. `unconfirmed` is
/// read as the record reads it now.
///
/// Every one of them this member signed is carried into the copy at the
/// next sync or signature, where the fleet's own rules decide it. The
/// highest number among those that may stand (all but a refused post) is
/// never signed again, even while the copy cannot take them yet; a refused
/// post is carried, so the copy takes whatever of it stood, but does not
/// hold the phone back.
public struct FleetEarlierLine: Decodable, Equatable, Sendable {
    var lastSigned: String?
    var chain: Chain?

    /// A record with no earlier line.
    public init() {}

    /// `FleetChain` as it was saved: canonical bytes of each operation.
    struct Chain: Decodable, Equatable, Sendable {
        var last: Data?
        var unconfirmed: [Data]?
        var base: Data?
        var floor: Data?
        var refusal: Refusal?

        struct Refusal: Decodable, Equatable, Sendable {
            var ops: [Data]?
            var base: Data?
            var floor: Data?
        }
    }

    /// The most a record carries over: far more than one line ever held.
    static let maxCarried = 1024

    /// The operations `member` (hex) signed, base64url, in sequence order,
    /// with `unconfirmed` (the record's own, base64url) among them, and the
    /// highest sequence number that may stand.
    public func carried(member: String, unconfirmed: [String]?) -> (ops: [String], signedSeq: UInt64?) {
        let standing = [lastSigned.flatMap(Base64URL.decode), chain?.last, chain?.base, chain?.floor, chain?.refusal?.base, chain?.refusal?.floor]
            .compactMap { $0 } + (chain?.unconfirmed ?? []) + (unconfirmed ?? []).compactMap(Base64URL.decode)
        let refused = chain?.refusal?.ops ?? []
        let mine = { (data: Data) -> Fleet.Op? in
            guard let op = try? Fleet.Op.decode(data), op.signer.hexString == member.lowercased() else { return nil }
            return op
        }
        let signedSeq = standing.prefix(Self.maxCarried).compactMap(mine).map(\.seq).max()
        var seen = Set<Data>()
        let ops = (standing + refused).prefix(Self.maxCarried)
            .compactMap { data in mine(data).map { ($0.seq, data) } }
            .filter { seen.insert($0.1).inserted }
            .sorted { $0.0 < $1.0 }
            .map { Base64URL.encode($0.1) }
        return (ops, signedSeq)
    }
}
