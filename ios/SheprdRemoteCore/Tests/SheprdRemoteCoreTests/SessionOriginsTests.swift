import Foundation
import SheprdRemoteCore
import Testing

@Suite struct SessionOriginsTests {
    private let own = ["https://192.168.1.5:7443", "https://100.64.0.5:7443"]
    private let relay = "https://127.0.0.1:50123"

    private func computer() throws -> Computer {
        Computer(
            device: "d_1", name: "Studio", origins: own,
            fingerprint: try #require(Fingerprint(String(repeating: "AB", count: 32))),
            keyKind: .software, keyData: Data()
        )
    }

    @Test func aComputerReachedOnlyThroughItsRelayIsStillTried() throws {
        #expect(try computer().sessionOrigins(last: nil, fallback: [relay]) == own + [relay])
        #expect(try computer().sessionOrigins(last: nil, fallback: []) == own)
    }

    @Test func theAddressThatAnsweredLastGoesFirstOnce() throws {
        #expect(try computer().sessionOrigins(last: own[1], fallback: [relay]) == [own[1], own[0], relay])
        #expect(try computer().sessionOrigins(last: relay, fallback: [relay]) == [relay] + own)
    }

    @Test func aLastAddressNoLongerListedIsNotTried() throws {
        // A relay proxy that stopped leaves its old port behind.
        #expect(try computer().sessionOrigins(last: relay, fallback: []) == own)
        #expect(try computer().sessionOrigins(last: nil, fallback: [own[0]]) == own)
    }
}
