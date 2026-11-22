import Foundation
import Testing
import Synchronization
@testable import SheprdRemoteCore

/// On the same network, a Tailscale address listed first won the race and
/// sends hung on it once its path went quiet.
struct AddressOrderTests {
    @Test func tailscaleAddressesGoAfterTheRestInTheirOrder() {
        let origins = [
            "https://127.0.0.1:7443",
            "https://[fd7a:115c:a1e0::d801:3b74]:7443",
            "https://100.101.102.103:7443",
            "https://m4max.local:7443",
            "https://m4max.tail1234.ts.net:7443",
            "https://192.168.1.20:7443",
        ]
        #expect(AddressOrder.preferred(origins) == [
            "https://127.0.0.1:7443",
            "https://m4max.local:7443",
            "https://192.168.1.20:7443",
            "https://[fd7a:115c:a1e0::d801:3b74]:7443",
            "https://100.101.102.103:7443",
            "https://m4max.tail1234.ts.net:7443",
        ])
    }

    @Test func onlyTailscalesRangesCount() {
        #expect(!AddressOrder.isTailscale("https://100.63.255.1:7443"))
        #expect(!AddressOrder.isTailscale("https://100.128.0.1:7443"))
        #expect(AddressOrder.isTailscale("https://100.64.0.1:7443"))
        #expect(!AddressOrder.isTailscale("https://[fd07:b51a:cc66::1]:7443"))
        #expect(!AddressOrder.isTailscale("https://notts.net:7443"))
        #expect(!AddressOrder.isTailscale("not a url"))
    }
}

/// The live socket's keepalive: a ping counts only when its pong comes in time.
struct CompletesWithinTests {
    @Test func anAnswerInTimeCounts() async {
        #expect(await completes(within: .seconds(5)) { $0(nil) })
    }

    @Test func aFailureDoesNot() async {
        #expect(await !completes(within: .seconds(5)) { $0(URLError(.timedOut)) })
    }

    @Test func noAnswerInTimeDoesNotAndALateOneIsIgnored() async {
        let late = Mutex<(@Sendable (Error?) -> Void)?>(nil)
        let answered = await completes(within: .seconds(5), sleep: { _ in }) { handler in
            late.withLock { $0 = handler }
        }
        #expect(!answered)
        late.withLock { $0 }?(nil)
        late.withLock { $0 }?(nil)
    }
}
