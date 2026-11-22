import Foundation
@testable import SheprdRemoteCore
import Testing

/// With the push relay off, a computer gets a registration that clears
/// every handle it holds (`crates/sheprd-remote-control/src/apns/tokens.rs`,
/// `Update`), so it stops pushing through them even when revoking them at
/// the push relay failed. A missing recipient would keep the old handle.
@Suite struct PushRelayOffTests {
    private func json(_ registration: API.APNsRegistration) throws -> [String: Any] {
        try #require(JSONSerialization.jsonObject(with: JSONEncoder().encode(registration)) as? [String: Any])
    }

    @Test func turningItOffClearsEveryRecipientWithAnExplicitNull() throws {
        let off = try json(.pushRelayOff(genericActivities: false))
        #expect(off["push_to_start"] is NSNull)
        #expect(off["alert"] is NSNull)
        #expect((off["activities"] as? [Any])?.isEmpty == true)
        // No token or handle goes out, and no key to seal to.
        #expect(off["notify_key"] == nil)
        #expect(Set(off.keys) == ["push_to_start", "alert", "activities"])
    }

    @Test func itKeepsThePhonesGenericChoice() throws {
        let off = try json(.pushRelayOff(genericActivities: true))
        #expect(off["activity_detail"] as? String == "generic")
    }

    @Test func anOrdinaryRegistrationLeavesAMissingRecipientOut() throws {
        // Left out, the computer keeps what it holds, as older phones send.
        let on = try json(API.APNsRegistration(pushToStart: nil, activities: []))
        #expect(on["push_to_start"] == nil)
        #expect(on["alert"] == nil)
    }
}
