import SwiftUI

/// The optional Face ID lock (Settings): the app is covered whenever it
/// leaves the foreground, including in the app switcher, until the owner
/// unlocks it.
struct AppLock: ViewModifier {
    @AppStorage(AppLock.setting) private var required = false
    @Environment(\.scenePhase) private var phase
    @State private var locked = UserDefaults.standard.bool(forKey: AppLock.setting)

    static let setting = "requireFaceID"

    func body(content: Content) -> some View {
        content
            .overlay {
                if required, locked || phase != .active {
                    ZStack {
                        Rectangle().fill(.regularMaterial).ignoresSafeArea()
                        VStack(spacing: 14) {
                            Image(systemName: "lock.fill").font(.largeTitle)
                            Text("Herdr Remote is locked").font(.headline)
                            Button("Unlock") { Task { await unlock() } }.buttonStyle(.glassProminent)
                        }
                    }
                    .transition(.opacity)
                }
            }
            .onChange(of: phase) { _, phase in
                if phase == .background, required { locked = true }
                if phase == .active, locked { Task { await unlock() } }
            }
            .task { if locked { await unlock() } }
    }

    private func unlock() async {
        if await OwnerCheck.confirm("Unlock Herdr Remote") {
            withAnimation { locked = false }
        }
    }
}
