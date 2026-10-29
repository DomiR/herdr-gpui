import Testing
@testable import HerdrRemoteCore

struct TermEchoTests {
    let start = ContinuousClock.now

    func at(_ milliseconds: Int) -> TermEcho.Instant { start + .milliseconds(milliseconds) }

    @Test func showsTypedKeysUntilTheScreenHasThem() {
        var echo = TermEcho()
        echo.queue(.keys(["l", "s", "space", "minus", "l", "enter"]))
        #expect(echo.text == "ls -l⏎")
        echo.sending(6, at: at(0))
        echo.screen(at: at(10))
        // The screen came before the computer took the keys, read from
        // the regular timer: they stay until it answers.
        #expect(echo.text == "ls -l⏎")
        echo.delivered(at: at(40))
        echo.screen(at: at(60))
        #expect(echo.text.isEmpty)
    }

    @Test func aScreenReadWhileTheRequestWasOutCountsOnceItIsTaken() {
        var echo = TermEcho()
        echo.queue(.keys(["a"]))
        echo.sending(1, at: at(0))
        echo.screen(at: at(30))
        echo.delivered(at: at(40))
        #expect(echo.text.isEmpty)
    }

    @Test func keysTypedAfterTheRequestStayForTheirOwn() {
        var echo = TermEcho()
        echo.queue(.keys(["a"]))
        echo.sending(1, at: at(0))
        echo.queue(.keys(["b"]))
        echo.delivered(at: at(40))
        echo.screen(at: at(50))
        #expect(echo.text == "b")
    }

    @Test func backspaceTakesBackWhatItCanAndMarksTheRest() {
        var echo = TermEcho()
        echo.queue(.keys(["a", "b", "backspace"]))
        #expect(echo.text == "a")
        echo.queue(.keys(["backspace", "backspace", "ctrl+c", "up"]))
        #expect(echo.text == "⌫")
    }

    @Test func aScreenThatNeverChangesLetsGoAfterTheLinger() {
        var echo = TermEcho()
        echo.queue(.keys(["s", "e", "c", "r", "e", "t"]))
        echo.sending(6, at: at(0))
        echo.delivered(at: at(40))
        echo.expire(at: at(500))
        #expect(echo.text == "secret")
        echo.expire(at: at(1040))
        #expect(echo.text.isEmpty)
    }

    @Test func aFailedRequestDropsEverything() {
        var echo = TermEcho()
        echo.queue(.keys(["a"]))
        echo.queue(.paste("long text"))
        echo.queue(.keys(["b"]))
        #expect(echo.text == "ab")
        echo.sending(1, at: at(0))
        echo.clear()
        #expect(echo.text.isEmpty)
    }
}
