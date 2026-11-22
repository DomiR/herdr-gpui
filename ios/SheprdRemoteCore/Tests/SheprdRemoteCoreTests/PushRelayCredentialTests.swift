import Foundation
import Testing
@testable import SheprdRemoteCore

/// When the app attests again, and which push relay a build uses.
@Suite struct PushRelayCredentialTests {
    static let expiresS: UInt64 = 1_800_000_000
    static var expires: Date { Date(timeIntervalSince1970: TimeInterval(expiresS)) }

    static func credential() throws -> PushRelayCredential {
        try PushRelayHandleTests.credential(try PushRelayHandleTests.fixture(), expiresS: expiresS)
    }

    @Test func isRenewedAWeekBeforeItRunsOut() throws {
        let credential = try Self.credential()
        let week: TimeInterval = 7 * 24 * 60 * 60
        #expect(!credential.needsRenewal(at: Self.expires.addingTimeInterval(-week - 1)))
        #expect(credential.needsRenewal(at: Self.expires.addingTimeInterval(-week)))
        #expect(credential.isValid(at: Self.expires.addingTimeInterval(-1)))
        #expect(!credential.isValid(at: Self.expires))
        #expect(credential.needsRenewal(at: Self.expires.addingTimeInterval(60)))
    }

    @Test func keepsWhatThePushRelayIssued() throws {
        let credential = try Self.credential()
        let fixture = try PushRelayHandleTests.fixture()
        #expect(credential.expires == Self.expires)
        #expect(credential.environment == .sandbox)
        #expect(credential.relayKeyId == fixture["relay_key_id"])
        let decoded = try JSONDecoder().decode(PushRelayCredential.self, from: JSONEncoder().encode(credential))
        #expect(decoded == credential)
    }

    static func profile(aps: String?) -> Data {
        let entitlements = aps.map { "<key>aps-environment</key><string>\($0)</string>" } ?? ""
        let plist = """
        <?xml version="1.0" encoding="UTF-8"?>
        <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
        <plist version="1.0"><dict><key>Name</key><string>p</string>
        <key>Entitlements</key><dict>\(entitlements)</dict></dict></plist>
        """
        // The CMS envelope around it, as bytes the parser must skip.
        return Data([0x30, 0x82, 0x00, 0xFF]) + Data(plist.utf8) + Data([0xA0, 0x00, 0x31])
    }

    @Test func aDevelopmentProfileUsesTheSandboxPushRelay() {
        #expect(PushRelayEnvironment(provisioningProfile: Self.profile(aps: "development")) == .sandbox)
        #expect(PushRelayEnvironment.sandbox.url("/v1/challenge")?.absoluteString == "https://push-sandbox.sheprd.org/v1/challenge")
    }

    @Test func noProfileOrADistributionOneUsesProduction() {
        #expect(PushRelayEnvironment(provisioningProfile: nil) == .production)
        #expect(PushRelayEnvironment(provisioningProfile: Self.profile(aps: "production")) == .production)
        #expect(PushRelayEnvironment(provisioningProfile: Self.profile(aps: nil)) == .production)
        #expect(PushRelayEnvironment(provisioningProfile: Data("garbage".utf8)) == .production)
        #expect(PushRelayEnvironment.production.url("/v1/attest")?.absoluteString == "https://push.sheprd.org/v1/attest")
    }
}
