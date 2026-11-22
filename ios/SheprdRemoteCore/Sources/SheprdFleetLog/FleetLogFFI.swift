import Foundation
import SheprdFleetFFI

/// The one way into the Rust fleet log (`crates/sheprd-fleet-ffi`): a JSON
/// request in, a JSON answer out, freed before this returns. Nothing is
/// kept between calls; the log's bytes travel with each.
enum FleetLogFFI {
    /// Answers `request`'s `ok` value, or throws its `error`.
    static func call(_ request: some Encodable) throws -> Data {
        let body = try JSONEncoder().encode(request)
        var response: UnsafeMutablePointer<UInt8>?
        var length = 0
        let code = body.withUnsafeBytes { bytes in
            sheprd_fleet_call(bytes.bindMemory(to: UInt8.self).baseAddress, bytes.count, &response, &length)
        }
        guard code == 0, let response else { throw FleetLogError.call }
        defer { sheprd_fleet_free(response, length) }
        let answer = Data(bytes: response, count: length)
        let envelope = try JSONDecoder().decode(Envelope.self, from: answer)
        if let error = envelope.error { throw FleetLogError(error) }
        guard let object = try JSONSerialization.jsonObject(with: answer) as? [String: Any],
              let ok = object["ok"]
        else { throw FleetLogError.call }
        return try JSONSerialization.data(withJSONObject: ok)
    }

    private struct Envelope: Decodable {
        let error: Refusal?
    }

    struct Refusal: Decodable {
        let kind: String
        let rule: String?
    }
}

/// Why the phone's own copy of the log answered nothing.
public enum FleetLogError: Error, Equatable, Sendable {
    /// The log refused it, naming the rule that failed (`signer`,
    /// `unknown`, `signature`, …).
    case refused(rule: String)
    /// Nothing given founds the fleet this phone joined.
    case unfounded
    /// The saved log is another fleet's.
    case otherFleet
    /// The saved log holds operations that no longer check out.
    case damaged
    /// An operation to seal is not this phone's.
    case notMine
    /// Over a bound.
    case bounds
    /// The call itself failed: a bad request, or the library.
    case call

    init(_ refusal: FleetLogFFI.Refusal) {
        self = switch refusal.kind {
        case "log": .refused(rule: refusal.rule ?? "")
        case "unfounded": .unfounded
        case "other_fleet": .otherFleet
        case "damaged": .damaged
        case "not_mine": .notMine
        case "bounds": .bounds
        default: .call
        }
    }

    /// The saved log cannot be read back: changed on disk, damaged, or
    /// another fleet's. The phone starts its copy again from a computer,
    /// checking everything again.
    public var unreadable: Bool {
        switch self {
        case .otherFleet, .damaged, .refused: true
        default: false
        }
    }
}
