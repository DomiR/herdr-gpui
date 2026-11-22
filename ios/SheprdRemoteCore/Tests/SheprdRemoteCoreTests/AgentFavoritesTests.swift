import Foundation
import Testing
@testable import SheprdRemoteCore

/// Favorites list first, in a section of their own, whatever the order.
struct AgentFavoritesTests {
    private struct Row {
        var name: String
        var status: Home.AgentStatus = .idle
        var age: TimeInterval?
        var group = "g"
        var favorite = false
    }

    private static func sections(_ sort: AgentSort, _ rows: [Row]) -> [(String, [String])] {
        sort.sections(of: rows) { row in
            AgentSort.Facts(status: row.status, date: row.age.map { Date(timeIntervalSince1970: 1_000_000 - $0) },
                            title: row.name, group: row.group, groupTitle: "title \(row.group)", favorite: row.favorite)
        }
        .map { ($0.id, $0.items.map(\.name)) }
    }

    private static let rows = [
        Row(name: "asking", status: .blocked, age: 1, group: "a"),
        Row(name: "starred idle", status: .idle, age: 50, group: "b", favorite: true),
        Row(name: "working", status: .working, age: 2, group: "b"),
        Row(name: "starred asking", status: .blocked, age: 5, group: "a", favorite: true),
    ]

    @Test(arguments: AgentSort.allCases)
    func favoritesComeFirstInEveryOrder(_ sort: AgentSort) {
        let sections = Self.sections(sort, Self.rows)
        #expect(sections.first?.0 == AgentSort.favoritesID)
        #expect(sections.first?.1 == ["starred asking", "starred idle"], "the newest first")
        let rest = sections.dropFirst().flatMap(\.1)
        #expect(!rest.contains { $0.hasPrefix("starred") }, "a favorite lists once")
        #expect(Set(rest) == ["asking", "working"])
    }

    @Test func theOtherSectionsKeepTheirOrder() {
        #expect(Self.sections(.priority, Self.rows).map(\.0) == [AgentSort.favoritesID, "blocked", "working"])
        #expect(Self.sections(.workspace, Self.rows).map(\.0) == [AgentSort.favoritesID, "a", "b"])
    }

    @Test func noFavoritesNoSection() {
        let rows = Self.rows.map { Row(name: $0.name, status: $0.status, age: $0.age, group: $0.group) }
        #expect(!Self.sections(.recent, rows).contains { $0.0 == AgentSort.favoritesID })
    }

    @Test func onlyFavoritesLeaveNoEmptySections() {
        let rows = [Row(name: "a", favorite: true), Row(name: "b", status: .done, favorite: true)]
        #expect(Self.sections(.priority, rows).map(\.0) == [AgentSort.favoritesID])
    }
}

/// The computer marks starred rows in its home, which the Rust fixture
/// writes.
struct AgentFavoritesWireTests {
    @Test func homeMarksStarredChatsAndTerminalAgents() throws {
        guard case let .home(home) = try WireTests.event("home") else { Issue.record("not home"); return }
        let workspace = try #require(home.workspaces.first)
        #expect(workspace.chats.first { $0.pane == "p1" }?.favorite == true)
        #expect(workspace.chats.first { $0.pane == "p3" }?.favorite == nil, "unstarred rows leave it out")
        #expect(workspace.terminals.first?.agent?.favorite == true)
    }
}
