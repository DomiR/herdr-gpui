import SheprdRemoteCore
import SwiftUI

extension View {
    /// Summarize at the row's leading edge: Claude, run once on
    /// its own, says where the chat stands, and its row shows that. The
    /// row's star, if any, comes first, where a full swipe reaches it.
    func summarizeSwipe(_ key: PaneKey, favorite: FavoriteTarget? = nil) -> some View {
        modifier(SummarizeSwipe(key: key, favorite: favorite))
    }
}

private struct SummarizeSwipe: ViewModifier {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    let favorite: FavoriteTarget?
    @State private var problem: String?

    func body(content: Content) -> some View {
        content
            .swipeActions(edge: .leading) {
                if let favorite { FavoriteSwipeButton(target: favorite) }
                Button(action: ask) { Label("Summarize", systemImage: "sparkles") }
                    .tint(.indigo)
            }
            .alert("No summary", isPresented: Binding(get: { problem != nil }, set: { if !$0 { problem = nil } })) {
            } message: {
                Text(problem ?? "")
            }
    }

    /// The computer answers at once; the summary reaches the row on its
    /// next `home`, so only a refusal is shown here.
    private func ask() {
        Task {
            do {
                try await model.fleet.summarize(key)
            } catch {
                problem = error.localizedDescription
            }
        }
    }
}
