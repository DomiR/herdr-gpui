import Foundation

/// How Home lists the agents of every computer. Priority is the desktop's
/// agent panel order; workspace is its grouped one. Favorites come first in
/// every order, in a section of their own.
public enum AgentSort: String, CaseIterable, Identifiable, Sendable {
    case priority, recent, workspace

    public var id: Self { self }

    /// What a list needs to know about one of its rows.
    public struct Facts: Sendable {
        public var status: Home.AgentStatus
        /// When it last changed, if known.
        public var date: Date?
        public var title: String
        public var group: String
        public var groupTitle: String
        /// Starred on some device, so it lists in Favorites.
        public var favorite: Bool

        /// An unknown status lists as idle.
        var shownStatus: Home.AgentStatus { status == .unknown ? .idle : status }

        public init(
            status: Home.AgentStatus, date: Date?, title: String, group: String, groupTitle: String, favorite: Bool = false
        ) {
            self.status = status
            self.date = date
            self.title = title
            self.group = group
            self.groupTitle = groupTitle
            self.favorite = favorite
        }
    }

    public struct Section<Item>: Identifiable {
        /// Titles can repeat: two workspaces may share a name.
        public var id: String
        public var title: String
        public var items: [Item]
        /// The status every row shares, in the priority order only, which
        /// its header shows as a colored dot.
        public var status: Home.AgentStatus?
    }

    /// The id of the favorites' section, which no status or group has.
    public static let favoritesID = "favorites"

    /// The statuses the priority order lists, in order.
    private static let priorityTitles: [(Home.AgentStatus, String)] =
        [(.blocked, "Needs you"), (.done, "Done"), (.working, "Working"), (.idle, "Idle")]

    /// `items` in this order, after a Favorites section holding the starred
    /// ones whatever the order. Every section lists the newest first, then
    /// the undated by title.
    public func sections<Item>(of items: [Item], facts: (Item) -> Facts) -> [Section<Item>] {
        let all = items.map { (item: $0, facts: facts($0)) }
        let favorites = all.filter(\.facts.favorite)
        let rest = favorites.isEmpty ? all : all.filter { !$0.facts.favorite }
        let starred = favorites.isEmpty ? [] : [
            Section(id: Self.favoritesID, title: "Favorites", items: Self.newestFirst(favorites).map(\.item)),
        ]
        return starred + ordered(rest)
    }

    /// The newest first, then the undated by title.
    private static func newestFirst<Item>(_ rows: [(item: Item, facts: Facts)]) -> [(item: Item, facts: Facts)] {
        rows.sorted { a, b in
            switch (a.facts.date, b.facts.date) {
            case let (first?, second?) where first != second: first > second
            case (_?, nil): true
            case (nil, _?): false
            default: a.facts.title.localizedStandardCompare(b.facts.title) == .orderedAscending
            }
        }
    }

    /// `all`, none of them a favorite, in this order.
    private func ordered<Item>(_ all: [(item: Item, facts: Facts)]) -> [Section<Item>] {
        let rows = Self.newestFirst(all)
        switch self {
        case .recent:
            return rows.isEmpty ? [] : [Section(id: "recent", title: "Recent", items: rows.map(\.item))]
        case .priority:
            return Self.priorityTitles.compactMap { status, title in
                let matching = rows.filter { $0.facts.shownStatus == status }
                return matching.isEmpty ? nil : Section(id: status.rawValue, title: title, items: matching.map(\.item), status: status)
            }
        case .workspace:
            // Groups keep the order of `items`, the computers' own.
            var order: [String] = []
            for row in all where !order.contains(row.facts.group) {
                order.append(row.facts.group)
            }
            return order.compactMap { group in
                let matching = rows.filter { $0.facts.group == group }
                return matching.first.map { Section(id: group, title: $0.facts.groupTitle, items: matching.map(\.item)) }
            }
        }
    }
}
