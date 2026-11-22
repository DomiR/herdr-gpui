import Foundation
import Testing
@testable import SheprdRemoteCore

/// Briefs: the mark the reducer keeps, and the outline the card draws.
struct BriefTests {
    private static let sample = """
    <pasted_content id="0e15">
    Task: make the iOS app use the hosted push relay, so a TestFlight install gets sealed alerts. Branch feat/ios-push-relay is checked out.

    Read first:
    - docs/apns-relay-plan.md (whole)
      - especially Design
    - docs/ios-spikes.md

    ## Steps
    1. Attest once
    2. Mirror the key
    3. Send through the relay

    Acceptance:
    * An alert arrives
    </pasted_content>
    """

    private func user(_ id: UInt64) -> Entry { Entry(id: EntryId(id), kind: .user(text: "Brief", files: 0)) }

    @Test func aBriefMarksAPromptAndDecodesFromTheWire() throws {
        var transcript = Transcript(agent: "Claude")
        try transcript.apply(.push(user(1)))
        let change = try JSONDecoder().decode(Change.self, from: Data(#"{"change":"brief","brief":{"entry":1,"from":"claude"}}"#.utf8))
        try transcript.apply(change)
        #expect(transcript.brief(1)?.fromName == "Claude")
        try transcript.apply(.brief(Brief(entry: 1)))
        #expect(transcript.briefs == [Brief(entry: 1)])
        #expect(transcript.brief(1)?.fromName == nil)
    }

    @Test func onlyAPromptIsABriefAndBriefsAreBounded() throws {
        var transcript = Transcript(agent: "Claude")
        try transcript.apply(.push(Entry(id: 1, kind: .agent(text: "Answer"))))
        #expect(throws: ChangeError.notPrompt) { try transcript.apply(.brief(Brief(entry: 1))) }
        #expect(throws: ChangeError.notPrompt) { try transcript.apply(.brief(Brief(entry: 9))) }
        for id in 2 ... UInt64(Brief.limit + 2) {
            try transcript.apply(.push(user(id)))
        }
        for id in 2 ... UInt64(Brief.limit + 1) {
            try transcript.apply(.brief(Brief(entry: EntryId(id))))
        }
        #expect(throws: ChangeError.briefLimit) { try transcript.apply(.brief(Brief(entry: EntryId(UInt64(Brief.limit + 2))))) }
    }

    @Test func truncatingDropsTheBriefsOfDroppedEntries() throws {
        var transcript = Transcript(agent: "Claude")
        for id: UInt64 in 1 ... 3 {
            try transcript.apply(.push(user(id)))
            try transcript.apply(.brief(Brief(entry: EntryId(id))))
        }
        try transcript.apply(.truncate(before: 3))
        #expect(transcript.briefs == [Brief(entry: 3)])
    }

    @Test func aTranscriptWithoutBriefsStillDecodes() throws {
        let transcript = try JSONDecoder().decode(Transcript.self, from: Data(#"{"agent":"Claude"}"#.utf8))
        #expect(transcript.briefs.isEmpty)
        let marked = try JSONDecoder().decode(Transcript.self, from: Data(#"{"agent":"Claude","briefs":[{"entry":4}]}"#.utf8))
        #expect(marked.briefs == [Brief(entry: 4)])
    }

    @Test func theOutlineFindsTheGoalAndSections() {
        let outline = BriefOutline(Self.sample)
        #expect(outline.goal == "make the iOS app use the hosted push relay, so a TestFlight install gets sealed alerts.")
        #expect(outline.sections.map(\.title) == ["Read first", "Steps", "Acceptance"])
        #expect(outline.sections.map(\.count) == [2, 3, 1])
        #expect(outline.sections.map(\.kind) == [.reading, .steps, .checks])
        #expect(!outline.text.contains("pasted_content"))
        #expect(outline.text.hasPrefix("Task:"))
    }

    @Test func aBriefOpeningWithAHeadingUsesItAsTheGoal() {
        let outline = BriefOutline("# Fix the relay\n\nSome context. More.\n\n## Steps\n- one")
        #expect(outline.goal == "Fix the relay")
        #expect(outline.sections.map(\.title) == ["Steps"])
    }

    @Test func plainTextHasNoSections() {
        let outline = BriefOutline("Just do it. Then stop: now")
        #expect(outline.goal == "Just do it.")
        #expect(outline.sections.isEmpty)
        #expect(outline.words == 6)
    }

    @Test func onlyALeadingEnvelopeIsRemoved() {
        #expect(PastedEnvelope.unwrap("<pasted_content id=\"1\">\nHi\n</pasted_content>") == "Hi")
        #expect(PastedEnvelope.unwrap("<pasted_content>Hi") == "Hi")
        #expect(PastedEnvelope.unwrap("<pasted_contents>Hi") == "<pasted_contents>Hi")
        #expect(PastedEnvelope.unwrap("Say <pasted_content>") == "Say <pasted_content>")
        // A tag that could hide words from the person shows as it is.
        for text in [
            "<pasted_content note=\"rm -rf\">Hi",
            "<pasted_content id=\"1\" note=\"rm -rf\">Hi",
            "<pasted_content id=\"a b\">Hi",
            "<pasted_content id=\"\">Hi",
            "<pasted_content id=\"\(String(repeating: "a", count: 33))\">Hi",
        ] {
            #expect(PastedEnvelope.unwrap(text) == text)
        }
    }

    /// A heading or list inside a fenced code block is code: no section
    /// starts there, so the sheet never splits the block.
    @Test func codeInAFenceIsNeverASection() {
        let outline = BriefOutline("""
        Set up the project.

        ## Steps
        - Install
        - Run

        ```sh
        # Install dependencies
        Steps:
        - not an item
        ```

        ~~~
        # Also code
        ~~~

        ## Done when
        - It runs
        """)
        #expect(outline.goal == "Set up the project.")
        #expect(outline.sections.map(\.title) == ["Steps", "Done when"])
        #expect(outline.sections.map(\.count) == [2, 1])
    }
}
