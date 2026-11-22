import SheprdRemoteCore
import SwiftUI

/// What a row's star marks: an agent's pane on its computer, which keeps
/// the favorites every phone and Sheprd share, and whether it is one now.
struct FavoriteTarget: Hashable {
    let key: PaneKey
    let favorite: Bool
}

extension HomeRow {
    /// The agent the row's star marks; none for a terminal agent an older
    /// computer does not name the pane of.
    var favoriteTarget: FavoriteTarget? {
        switch self {
        case let .chat(item):
            FavoriteTarget(key: item.key, favorite: item.chat.favorite == true)
        case let .agent(item):
            item.agent?.pane.map {
                FavoriteTarget(key: PaneKey(device: item.computer.device, pane: $0), favorite: item.agent?.favorite == true)
            }
        }
    }
}

extension AppModel {
    /// Stars or unstars `target` on its computer. The row moves when the
    /// computer's next home says so, on every device; a refusal is told.
    func toggleFavorite(_ target: FavoriteTarget) {
        Task {
            do {
                try await fleet.setFavorite(!target.favorite, for: target.key)
            } catch {
                noticeTitle = target.favorite ? String(localized: "Not removed from Favorites") : String(localized: "Not added to Favorites")
                notice = error.localizedDescription
            }
        }
    }
}

/// The star a row's leading swipe shows, as Messages pins a conversation.
struct FavoriteSwipeButton: View {
    @Environment(AppModel.self) private var model
    let target: FavoriteTarget

    var body: some View {
        Button { model.toggleFavorite(target) } label: {
            target.favorite
                ? Label("Unfavorite", systemImage: "star.slash.fill")
                : Label("Favorite", systemImage: "star.fill")
        }
        .tint(.yellow)
    }
}

/// The star a row's context menu shows.
struct FavoriteMenuButton: View {
    @Environment(AppModel.self) private var model
    let target: FavoriteTarget

    var body: some View {
        if target.favorite {
            Button("Remove from Favorites", systemImage: "star.slash") { model.toggleFavorite(target) }
        } else {
            Button("Add to Favorites", systemImage: "star") { model.toggleFavorite(target) }
        }
    }
}

extension View {
    /// The star at the row's leading edge, for a row that offers nothing
    /// else there (`summarizeSwipe` shows it beside Summarize).
    func favoriteSwipe(_ target: FavoriteTarget?) -> some View {
        swipeActions(edge: .leading) {
            if let target { FavoriteSwipeButton(target: target) }
        }
    }
}
