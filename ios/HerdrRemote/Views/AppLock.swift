import SwiftUI
import UIKit

/// The optional Face ID lock (Settings). While locked, a window above every
/// other one covers the app, sheets and alerts included, and in the app
/// switcher, until the owner unlocks it. The setting lives in the App
/// Group, so the share extension honors it too.
struct AppLock: ViewModifier {
    @AppStorage(AppLock.setting, store: AppLock.store) private var required = false
    @Environment(\.scenePhase) private var phase
    @State private var locked = AppLock.isOn
    @State private var cover: UIWindow?

    nonisolated static let setting = "requireFaceID"
    nonisolated static var store: UserDefaults? { UserDefaults(suiteName: "group.so.pen.herdr-remote") }
    nonisolated static var isOn: Bool { store?.bool(forKey: setting) ?? false }

    func body(content: Content) -> some View {
        content
            .onChange(of: required && (locked || phase != .active), initial: true) { _, covered in
                if covered { showCover() } else { hideCover() }
            }
            .onChange(of: phase) { _, phase in
                if phase == .background, required { locked = true }
                if phase == .active, locked { Task { await unlock() } }
            }
            .task { if locked { await unlock() } }
    }

    private func showCover() {
        guard cover == nil,
              let scene = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).first
        else { return }
        let window = UIWindow(windowScene: scene)
        window.windowLevel = .alert + 1
        window.rootViewController = UIHostingController(rootView: LockCover { Task { await unlock() } })
        window.isHidden = false
        cover = window
    }

    private func hideCover() {
        cover?.isHidden = true
        cover = nil
    }

    private func unlock() async {
        if await OwnerCheck.confirm("Unlock Herdr Remote") { locked = false }
    }
}

private struct LockCover: View {
    let unlock: () -> Void

    var body: some View {
        ZStack {
            Rectangle().fill(.regularMaterial).ignoresSafeArea()
            VStack(spacing: 14) {
                Image(systemName: "lock.fill").font(.largeTitle)
                Text("Herdr Remote is locked").font(.headline)
                Button("Unlock", action: unlock).buttonStyle(.glassProminent)
            }
        }
    }
}
