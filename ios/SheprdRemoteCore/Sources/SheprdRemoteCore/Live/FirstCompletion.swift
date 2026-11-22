import Synchronization

/// Waits for a completion handler that may be called more than once, and
/// answers with its first call.
///
/// `URLSessionWebSocketTask.sendPing` is such a handler: when the network
/// drops it can report the failure twice, and resuming a continuation a
/// second time traps, so losing the network took the whole app down.
public func firstCompletion(_ start: (@escaping @Sendable (Error?) -> Void) -> Void) async throws {
    try await withCheckedThrowingContinuation { (done: CheckedContinuation<Void, Error>) in
        let once = Once(done)
        start { once.resume($0) }
    }
}

/// Whether a handler like `sendPing`'s reports success within `limit`.
/// A failure, or no call in time, answers false; a call after that, or a
/// second one, is ignored.
public func completes(
    within limit: Duration,
    sleep: @escaping @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) },
    _ start: (@escaping @Sendable (Error?) -> Void) -> Void
) async -> Bool {
    let timer = Mutex<Task<Void, Never>?>(nil)
    let answered = await withCheckedContinuation { (done: CheckedContinuation<Bool, Never>) in
        let once = Mutex<CheckedContinuation<Bool, Never>?>(done)
        let finish: @Sendable (Bool) -> Void = { value in once.withLock { $0.take() }?.resume(returning: value) }
        timer.withLock {
            $0 = Task {
                try? await sleep(limit)
                finish(false)
            }
        }
        start { finish($0 == nil) }
    }
    timer.withLock { $0?.cancel() }
    return answered
}

private final class Once: Sendable {
    private let pending: Mutex<CheckedContinuation<Void, Error>?>

    init(_ continuation: CheckedContinuation<Void, Error>) {
        pending = Mutex(continuation)
    }

    func resume(_ error: Error?) {
        guard let continuation = pending.withLock({ $0.take() }) else { return }
        if let error {
            continuation.resume(throwing: error)
        } else {
            continuation.resume()
        }
    }
}
