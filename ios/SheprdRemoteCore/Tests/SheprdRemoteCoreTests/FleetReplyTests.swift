import Foundation
import Testing
@testable import SheprdRemoteCore

/// A read of the fleet speaks for the key it asked about: a reply that
/// arrives after the phone's membership changed (a join cancelled and made
/// again with a new key) is never taken for the key held now.
struct FleetReplyTests {
    @Test func aReplyNamesTheMemberItWasAskedAbout() async throws {
        let next = API.FleetNext(fleet: String(repeating: "09", count: 32), seq: 1, parents: [])
        let computer = FleetClientTests.Computer { _, _, _ in FleetClientTests.status(next: next) }
        let first = Data(repeating: 1, count: 32)
        let status = try await computer.client.status(member: first)
        #expect(status.isAbout(member: first.hexString))
        #expect(status.isAbout(member: first.hexString.uppercased()))
        // The join was cancelled and made again while the read was out.
        #expect(!status.isAbout(member: Data(repeating: 2, count: 32).hexString))
        #expect(!status.isAbout(member: nil))
        // A read asked about no member says nothing about one held since.
        let anonymous = try await computer.client.status(member: nil)
        #expect(anonymous.isAbout(member: nil))
        #expect(!anonymous.isAbout(member: first.hexString))
    }

    @Test func aDecodedStatusIsAboutNoMemberUntilAsked() throws {
        let status = try JSONDecoder().decode(API.FleetStatus.self, from: Data(#"{"heads":[]}"#.utf8))
        #expect(status.member == nil && status.isAbout(member: nil))
    }
}
