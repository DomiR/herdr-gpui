import CryptoKit
import DeviceCheck
import SwiftUI

/// Spike 3 of `docs/apns-relay-plan.md`: what App Attest hands a server on
/// a real phone. Makes a key, attests it over a random challenge, signs one
/// assertion, and saves all of it to `Documents/attest.json`, which
/// `xcrun devicectl device copy from` reads off a development build so a
/// checker on the Mac can verify it as the relay will.
struct AttestTab: View {
    @State private var lines: [String] = []
    @State private var running = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Button(running ? "Attesting…" : "Attest a new key") { Task { await attest() } }
                        .disabled(running)
                } footer: {
                    Text("Then copy Documents/attest.json off the phone with devicectl.")
                }
                Section("Result") {
                    ForEach(Array(lines.enumerated()), id: \.offset) { Text($0.element).font(.caption.monospaced()) }
                }
            }
            .navigationTitle("App Attest")
        }
    }

    private func attest() async {
        running = true
        defer { running = false }
        lines = []
        let service = DCAppAttestService.shared
        lines.append("supported: \(service.isSupported)")
        guard service.isSupported else { return }
        do {
            let clock = ContinuousClock()
            var keyID = ""
            let made = try await clock.measure { keyID = try await service.generateKey() }
            lines.append("generateKey: \(ms(made)) ms")

            let challenge = Data((0..<32).map { _ in UInt8.random(in: .min ... .max) })
            var attestation = Data()
            let attested = try await clock.measure {
                attestation = try await service.attestKey(keyID, clientDataHash: Data(SHA256.hash(data: challenge)))
            }
            lines.append("attestKey: \(ms(attested)) ms, \(attestation.count) bytes")

            let clientData = Data("spike assertion \(Date().timeIntervalSince1970)".utf8)
            var assertion = Data()
            let asserted = try await clock.measure {
                assertion = try await service.generateAssertion(keyID, clientDataHash: Data(SHA256.hash(data: clientData)))
            }
            lines.append("generateAssertion: \(ms(asserted)) ms, \(assertion.count) bytes")

            let record: [String: String] = [
                "bundle_id": Bundle.main.bundleIdentifier ?? "",
                "key_id": keyID,
                "challenge": challenge.base64EncodedString(),
                "attestation": attestation.base64EncodedString(),
                "client_data": clientData.base64EncodedString(),
                "assertion": assertion.base64EncodedString(),
            ]
            let file = URL.documentsDirectory.appending(path: "attest.json")
            try JSONSerialization.data(withJSONObject: record, options: [.sortedKeys]).write(to: file, options: .atomic)
            lines.append("saved Documents/attest.json")
        } catch let error as DCError {
            lines.append("DCError \(error.code.rawValue): \(error.localizedDescription)")
        } catch {
            lines.append("failed: \(error)")
        }
    }

    private func ms(_ duration: Duration) -> Int {
        let parts = duration.components
        return Int(parts.seconds * 1000 + parts.attoseconds / 1_000_000_000_000_000)
    }
}
