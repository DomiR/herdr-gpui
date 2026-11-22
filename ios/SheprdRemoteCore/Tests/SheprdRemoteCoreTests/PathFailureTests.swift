import Foundation
import Testing
@testable import SheprdRemoteCore

/// A path can stop carrying traffic while the live socket still looks
/// open, so a call that gets no answer is what sends the link looking.
struct LinkPathFailureTests {
    @Test func aDeadPathMovesTheLinkToAnAddressThatAnswers() async {
        let computer = FakeComputer()
        await computer.setPing("https://tailscale", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://tailscale", "https://lan"], io: await computer.io())
        await link.start()
        _ = await collect(link) { $0.contains(.status(.online(origin: "https://tailscale"))) }
        await computer.setPing("https://tailscale", String(repeating: "CD", count: 32))
        await computer.setPing("https://lan", mac)
        await link.pathFailed("https://tailscale", started: .now)
        // The fake's backoff lasts a minute: this only arrives if the link
        // retries at once.
        let moved = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        #expect(moved.contains(.status(.online(origin: "https://lan"))))
        #expect(await computer.connects == ["https://tailscale", "https://lan"])
        await link.stop()
    }

    /// A slow call can fail after the link has already moved on.
    @Test func aFailureAtAnAddressTheLinkLeftKeepsTheSocket() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        _ = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        await link.pathFailed("https://tailscale", started: .now)
        await computer.emit(.error("still here"))
        let seen = await collect(link) { updates in
            updates.contains { update in
                switch update {
                case .event(.error), .status(.offline): true
                default: false
                }
            }
        }
        #expect(seen.contains(.event(.error("still here"))))
        #expect(await computer.connects == ["https://lan"])
        await link.stop()
    }

    /// Two calls can fail on one dead path: the second must not take down
    /// the socket the first one's failure got the link back on.
    @Test func aCallOlderThanTheSocketKeepsItEvenAtTheSameAddress() async {
        let computer = FakeComputer()
        await computer.setPing("https://lan", mac)
        let link = ComputerLink(fingerprint: mac, origins: ["https://lan"], io: await computer.io())
        await link.start()
        _ = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        let older = ContinuousClock.now
        await link.pathFailed("https://lan", started: .now)
        _ = await collect(link) { $0.contains(.status(.online(origin: "https://lan"))) }
        await link.pathFailed("https://lan", started: older)
        await computer.emit(.error("still here"))
        let seen = await collect(link) { updates in
            updates.contains { update in
                switch update {
                case .event(.error), .status(.offline): true
                default: false
                }
            }
        }
        #expect(seen.contains(.event(.error("still here"))))
        #expect(await computer.connects == ["https://lan", "https://lan"])
        await link.stop()
    }
}

struct UnreportedFailuresTests {
    @Test func aLineStaysUntilTheComputerAcceptsIt() {
        var failures = UnreportedFailures()
        failures.add("first")
        failures.add("second")
        failures.accepted("first")
        #expect(failures.lines == ["second"])
    }

    @Test func theOldestLinesGoPastTheLimit() {
        var failures = UnreportedFailures()
        for index in 0...UnreportedFailures.limit { failures.add("\(index)") }
        #expect(failures.lines.count == UnreportedFailures.limit)
        #expect(failures.lines.first == "1")
    }

    /// Newer failures can push out the line a report is carrying.
    @Test func acceptingALineAlreadyPushedOutRemovesNothingElse() {
        var failures = UnreportedFailures()
        failures.add("old")
        for index in 0..<UnreportedFailures.limit { failures.add("\(index)") }
        failures.accepted("old")
        #expect(failures.lines.count == UnreportedFailures.limit)
    }
}

struct PathFailureTests {
    @Test(arguments: [
        URLError.Code.cannotConnectToHost, .cannotFindHost, .dnsLookupFailed,
        .networkConnectionLost, .notConnectedToInternet, .timedOut,
    ])
    func noAnswerMeansTheAddressIsDead(code: URLError.Code) {
        #expect(PathFailure.isDead(URLError(code)))
    }

    @Test func aCancelledCallARefusedCertificateOrAnAnswerIsNot() {
        #expect(!PathFailure.isDead(URLError(.cancelled)))
        #expect(!PathFailure.isDead(URLError(.serverCertificateUntrusted)))
        #expect(!PathFailure.isDead(LinkError.outdated))
    }

    @Test(arguments: [
        ("https://mac:7443/api/chat/p1/upload", "/api/chat/{id}/upload"),
        ("https://mac:7443/api/v2/chat/p1", "/api/v2/chat/{id}"),
        ("https://mac:7443/api/pair/claim/secret", "/api/pair/claim/{id}"),
        ("https://mac:7443/api/v2/workspaces", "/api/v2/workspaces"),
        ("https://mac:7443/api/v2/search?q=private", "/api/v2/search"),
    ])
    func logsNameARouteWithoutItsIdentifiers(url: String, route: String) {
        #expect(PathFailure.route(URL(string: url)) == route)
    }

    @Test func aSummaryCarriesTheCodeAndTheTime() {
        let summary = PathFailure.summary(URLError(.networkConnectionLost), after: .milliseconds(1250))
        #expect(summary.contains("NSURLErrorDomain -1005"))
        #expect(summary.hasSuffix("after 1.2s") || summary.hasSuffix("after 1.3s"))
    }
}

struct RefusalTests {
    @Test func aDaemonErrorReadsAsItsMessage() {
        let body = Data(#"{"error":"sheprd-acp was not found"}"#.utf8)
        #expect(Refusal.text(status: 503, body: body) == "sheprd-acp was not found")
    }

    @Test func aLongMessageIsParsedBeforeItIsCut() {
        let message = String(repeating: "x", count: Refusal.limit + 40)
        let body = Data(#"{"error":"\#(message)"}"#.utf8)
        #expect(Refusal.text(status: 503, body: body) == String(repeating: "x", count: Refusal.limit))
    }

    @Test func anEmptyAnswerStillSaysTheStatus() {
        #expect(Refusal.text(status: 503, body: Data()) == "The computer answered 503.")
    }

    @Test func anythingElseKeepsTheStatusAndBody() {
        #expect(Refusal.text(status: 502, body: Data("Bad Gateway".utf8)) == "The computer answered 502: Bad Gateway")
        #expect(Refusal.text(status: 500, body: Data(#"{"error":""}"#.utf8)) == #"The computer answered 500: {"error":""}"#)
    }
}

/// A call's tag goes in a header the computer logs, so a report and the
/// computer's line for the same call can be matched.
struct CallTagTests {
    @Test func aTagIsEightHexDigits() {
        for _ in 0..<64 {
            let tag = PathFailure.newTag()
            #expect(tag.count == 8)
            #expect(tag.allSatisfy { $0.isHexDigit && !$0.isUppercase })
        }
    }

    @Test func aReportNamesTheCallItsAddressAndWhenItBeganToTheMillisecond() {
        let began = Date(timeIntervalSince1970: 1_763_743_935.25)
        let line = PathFailure.call(tag: "3fa9c2d1", origin: "https://m4max.local:7443", began: began)
        #expect(line == "call 3fa9c2d1 at https://m4max.local:7443 begun 2025-11-21T16:52:15.250Z")
    }
}

/// A call can make several requests, such as one upload per picture: each
/// has its own tag, and a report names the last, the one that failed.
struct CallTagsTests {
    @Test func eachRequestGetsItsOwnTagAndTheReportNamesTheLast() {
        let tags = CallTags()
        let began = Date(timeIntervalSince1970: 1_763_743_935.25)
        let first = tags.next(at: began.addingTimeInterval(-1))
        let last = tags.next(at: began)
        #expect(first != last)
        #expect(tags.report(origin: "https://lan") == "call \(last) at https://lan begun 2025-11-21T16:52:15.250Z, after \(first)")
    }

    @Test func aCallWithNoRequestHasNoReport() {
        #expect(CallTags().report(origin: "https://lan") == nil)
    }

    @Test func aLongCallKeepsItsLatestRequestsAndListsAFew() {
        let tags = CallTags()
        let sent = (0..<(CallTags.limit + 3)).map { _ in tags.next() }
        #expect(tags.requests.map(\.tag) == Array(sent.suffix(CallTags.limit)))
        let report = tags.report(origin: "https://lan") ?? ""
        #expect(report.hasSuffix(", after " + sent.dropLast().suffix(CallTags.listed).joined(separator: " ")))
    }
}
