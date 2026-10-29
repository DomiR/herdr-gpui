import Foundation
import HerdrRemoteCore

/// Sends a chat command while the app is not in front: from a
/// notification's action or a Live Activity's button. Live links are
/// closed then, so it signs in afresh at whichever of the computer's
/// addresses answers first.
enum BackgroundCommands {
    enum Failure: Error, LocalizedError {
        case unknownComputer
        case rejected(String)

        var errorDescription: String? {
            switch self {
            case .unknownComputer: "This phone is not paired with that computer."
            case let .rejected(reason): reason
            }
        }
    }

    /// `computer` is the CA fingerprint the notification or activity named.
    static func send(_ command: ChatCommand, computer fingerprint: String, pane: String) async throws {
        let computers = try ComputerStore.load()
        guard let wanted = Fingerprint(fingerprint),
              let computer = computers.first(where: { $0.fingerprint == wanted })
        else { throw Failure.unknownComputer }
        let session = try await Sessions().session(for: computer)
        if let rejected = try await session.client.command(command, pane: pane, bearer: session.token) {
            throw Failure.rejected(rejected)
        }
    }
}
