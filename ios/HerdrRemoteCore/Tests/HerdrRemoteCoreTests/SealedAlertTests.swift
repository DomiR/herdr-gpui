import CryptoKit
import Foundation
import Testing
@testable import HerdrRemoteCore

@Suite struct SealedAlertTests {
    static func alert(_ json: String) throws -> SealedAlert {
        try JSONDecoder().decode(SealedAlert.self, from: Data(json.utf8))
    }

    static let question = #"""
    {"v":1,"kind":"question","clear":false,"computer":"AB","pane":"w1:p2","title":"Claude in app","body":"Run git push?","badge":2,
     "permission":{"id":7,"options":[
       {"id":"allow","name":"Allow once","kind":"allow_once"},
       {"id":"always","name":"Always allow","kind":"allow_always"},
       {"id":"deny","name":"Deny","kind":"reject_once"},
       {"id":"x","name":"Other","kind":"something_new"},
       {"id":"fifth","name":"Fifth","kind":"other"}]}}
    """#

    @Test func aQuestionOffersItsFirstFourAnswers() throws {
        let presentation = try Self.alert(Self.question).presentation
        #expect(presentation.actions.map(\.id) == ["answer:allow", "answer:always", "answer:deny", "answer:x"])
        #expect(presentation.actions.map(\.style) == [.plain, .authenticated, .destructive, .plain])
        #expect(presentation.category?.hasPrefix("question.") == true)
    }

    @Test func theSameAnswersShareACategory() throws {
        let one = try Self.alert(Self.question).presentation.category
        let two = try Self.alert(Self.question.replacingOccurrences(of: "\"id\":7", with: "\"id\":8")).presentation.category
        let other = try Self.alert(Self.question.replacingOccurrences(of: "Deny", with: "No")).presentation.category
        #expect(one == two)
        #expect(one != other)
    }

    @Test func aFinishedTurnOffersReplyAndAnErrorNothing() throws {
        let finished = try Self.alert(#"{"kind":"turn_ended","computer":"AB","pane":"p","title":"t","body":"b"}"#).presentation
        #expect(finished.category == AlertPresentation.finishedCategory)
        #expect(finished.actions.map(\.id) == ["reply"])
        let failed = try Self.alert(#"{"kind":"error","computer":"AB","pane":"p","title":"t","body":"b"}"#).presentation
        #expect(failed.category == nil && failed.actions.isEmpty)
    }

    @Test func actionsBecomeChatCommands() {
        #expect(AlertPresentation.command(action: "answer:allow", permission: 7, text: nil) == .answer(permission: 7, option: "allow"))
        #expect(AlertPresentation.command(action: "answer:allow", permission: nil, text: nil) == nil)
        #expect(AlertPresentation.command(action: "reply", permission: nil, text: "  ship it \n") == .prompt(text: "ship it"))
        #expect(AlertPresentation.command(action: "reply", permission: nil, text: "   ") == nil)
        #expect(AlertPresentation.command(action: "com.apple.UNNotificationDefaultActionIdentifier", permission: 7, text: nil) == nil)
    }
}

/// The daemon's own sealed alert (`Fixtures/notify/alert.json`, written by
/// `apns/tests/alert.rs`) opens here to the plaintext it sealed.
@Suite struct SealedAlertFixtureTests {
    struct Fixture: Decodable {
        let device_private_key: String
        let apns: APNs
        struct APNs: Decodable { let sealed: String }
    }

    @Test func theDaemonsAlertOpensWithTheDeviceKey() throws {
        let url = try #require(Bundle.module.url(forResource: "Fixtures/notify/alert", withExtension: "json"))
        let data = try Data(contentsOf: url)
        let fixture = try JSONDecoder().decode(Fixture.self, from: data)
        let key = try P256.KeyAgreement.PrivateKey(rawRepresentation: #require(Base64URL.decode(fixture.device_private_key)))
        let alert = try SealedAlert.open(fixture.apns.sealed, device: key.publicKey) { try key.sharedSecretFromKeyAgreement(with: $0) }
        let raw = try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let plaintext = try #require(raw["plaintext"] as? [String: Any])
        #expect(alert.pane == plaintext["pane"] as? String)
        #expect(alert.title == plaintext["title"] as? String)
        #expect(alert.computer == plaintext["computer"] as? String)
        if let permission = plaintext["permission"] as? [String: Any] {
            #expect(alert.permission?.id.value == (permission["id"] as? NSNumber)?.uint64Value)
            #expect(alert.permission?.options.count == (permission["options"] as? [Any])?.count)
        }
        #expect(!alert.presentation.actions.isEmpty || alert.kind != .question)
    }
}
