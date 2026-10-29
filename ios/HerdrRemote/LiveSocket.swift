import Foundation
import HerdrRemoteCore

/// `ComputerLink`'s network for one paired computer: pings, sessions from
/// the device key, and the live socket over the pinned session.
enum LiveSocket {
    static func io(for computer: Computer, sessions: Sessions) -> LinkIO {
        LinkIO(
            ping: { origin in
                try await RemoteClient(origin: origin, fingerprint: computer.fingerprint).fingerprint()
            },
            session: { origin in
                try await sessions.token(for: computer, at: origin)
            },
            connect: { origin, token in
                let client = RemoteClient(origin: origin, fingerprint: computer.fingerprint)
                return try await open(client.live(bearer: token), client: client, computer: computer, sessions: sessions)
            }
        )
    }

    private static func open(
        _ task: URLSessionWebSocketTask,
        client: RemoteClient,
        computer: Computer,
        sessions: Sessions
    ) async throws -> LiveConnection {
        task.resume()
        let events = AsyncThrowingStream<LiveEvent, Error> { continuation in
            let reader = Task {
                // Keeps the client, and so its session and pinning, alive
                // as long as the socket reads.
                _ = client
                do {
                    while !Task.isCancelled {
                        let message = try await task.receive()
                        guard case let .string(text) = message else { continue }
                        // A message a newer daemon adds is skipped, not fatal.
                        if let event = try? JSONDecoder().decode(LiveEvent.self, from: Data(text.utf8)) {
                            continuation.yield(event)
                        }
                    }
                    continuation.finish()
                } catch {
                    if (task.response as? HTTPURLResponse)?.statusCode == 401 {
                        // The token expired between minting and use.
                        await sessions.forget(computer)
                    }
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in
                reader.cancel()
                task.cancel(with: .goingAway, reason: nil)
            }
        }
        return LiveConnection(
            events: events,
            send: { request in
                let data = try JSONEncoder().encode(request)
                try await task.send(.string(String(decoding: data, as: UTF8.self)))
            },
            close: { task.cancel(with: .goingAway, reason: nil) }
        )
    }
}
