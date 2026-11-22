import SheprdRemoteCore
import SwiftUI
import UIKit

/// Settings > Computers: the paired computers, each opening its page, with
/// Edit to forget them and a button to pair another.
struct ComputersSettingsView: View {
    @Environment(AppModel.self) private var model
    @Environment(WindowState.self) private var window: WindowState?
    @State private var forgetting: Computer?
    @State private var editMode = EditMode.inactive

    var body: some View {
        List {
            Section {
                ForEach(model.computers) { computer in
                    NavigationLink {
                        ComputerScreen(device: computer.device).makesRoomForConnectionToast()
                    } label: {
                        ComputerRow(computer: computer)
                    }
                    .swipeActions {
                        // Not `.destructive`: that removes the row before
                        // the confirmation answers.
                        Button("Forget", systemImage: "trash") { forgetting = computer }.tint(.red)
                    }
                }
                .onDelete { offsets in forgetting = offsets.first.map { model.computers[$0] } }
                Button("Pair a Computer", systemImage: "plus") { window?.pairing = true }
            } footer: {
                if !model.computers.isEmpty {
                    Text("Forgetting a computer removes its chats from this \(UIDevice.current.model). It does not stop Sheprd there.")
                }
            }
        }
        .navigationTitle("Computers")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { if !model.computers.isEmpty { EditButton() } }
        .forgetConfirmation($forgetting)
        .environment(\.editMode, $editMode)
        #if DEBUG
        .task { if UserDefaults.standard.bool(forKey: "editComputers") { editMode = .active } }
        #endif
    }
}

/// A computer in Settings: its name, then how it stands or where it is.
private struct ComputerRow: View {
    @Environment(AppModel.self) private var model
    var computer: Computer

    var body: some View {
        let machine = model.fleet.machines.first { $0.id == computer.device }
        let status = machine?.status
        VStack(alignment: .leading, spacing: 2) {
            Text(model.displayName(computer))
            if status == .revoked {
                ComputerStatusLine(status: .revoked).font(.caption)
            } else if status == .outdated || machine?.needsUpdate == true {
                ComputerStatusLine(status: .outdated).font(.caption)
            } else if let vouching = model.vouching(computer) {
                let via = "via \(model.displayName(vouching))"
                Text(computer.relay == nil ? via : "\(via) · \(Computer.relayLabel)")
                    .font(.caption).foregroundStyle(.secondary)
            } else if let label = computer.reachLabel {
                Text(label).font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}

/// Settings > Chats.
struct ChatSettingsView: View {
    @Environment(AppModel.self) private var model
    @AppStorage(RowLines.key) private var rowLines = RowLines.choices.lowerBound

    var body: some View {
        List {
            Section {
                Picker("Lines per Row", selection: $rowLines) {
                    ForEach(RowLines.choices, id: \.self) { Text($0, format: .number).tag($0) }
                }
            } footer: {
                Text("How much of a chat's latest reply or summary its row shows in the lists.")
            }
            Section {
                NavigationLink {
                    TextShortcutsView().makesRoomForConnectionToast()
                } label: {
                    LabeledContent("Text Shortcuts", value: "\(model.shortcuts.shortcuts.count)")
                }
            }
        }
        .navigationTitle("Chats")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// Settings > Voice.
struct VoiceSettingsView: View {
    @AppStorage(VoiceSettings.speaksKey) private var voiceSpeaks = true
    @AppStorage(VoiceSettings.confirmsKey) private var voiceConfirms = false
    @AppStorage(VoiceSettings.languageKey) private var voicePhoneLanguage = false

    var body: some View {
        List {
            Section {
                Toggle("Speak Results", isOn: $voiceSpeaks)
            }
            Section {
                Toggle("Confirm by Voice", isOn: $voiceConfirms)
            } footer: {
                Text("Reads the plan aloud and starts on a plain yes, for a new worktree only. Anything else still needs a tap.")
            }
            Section {
                Toggle("Use Phone Language", isOn: $voicePhoneLanguage)
            } footer: {
                Text("Experimental: commands other than English are less reliable, and cannot stop or answer a chat.")
            }
        }
        .navigationTitle("Voice")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// Settings > Sounds & Haptics.
struct SoundsSettingsView: View {
    @AppStorage(FeedbackSettings.soundsKey) private var effectSounds = true
    @AppStorage(FeedbackSettings.hapticsKey) private var haptics = true

    var body: some View {
        List {
            Section {
                Toggle("Effect Sounds", isOn: $effectSounds)
            } footer: {
                Text(FeedbackSettings.canTap
                    ? "Sounds that go with an agent's message effects, such as slam and fireworks. Quiet when the Ring/Silent switch is on silent."
                    : "Sounds that go with an agent's message effects, such as slam and fireworks. Quiet when the \(UIDevice.current.model) is muted.")
            }
            if FeedbackSettings.canTap {
                Section {
                    Toggle("Haptics", isOn: $haptics)
                } footer: {
                    Text("What you feel on sending, answering, terminal keys, and effects.")
                }
            }
        }
        .navigationTitle("Sounds & Haptics")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// Settings > Privacy & Storage: the Face ID lock and the cache.
struct PrivacySettingsView: View {
    @Environment(AppModel.self) private var model
    @AppStorage(AppLock.setting, store: AppLock.store) private var requireFaceID = false
    @State private var cacheSize = 0

    var body: some View {
        List {
            Section {
                Toggle("Require Face ID", isOn: Binding(
                    get: { requireFaceID },
                    set: { on in
                        // Turning the lock off asks too, so a borrowed,
                        // unlocked phone cannot drop it.
                        Task {
                            if await OwnerCheck.confirm(on ? "Lock Sheprd" : "Stop locking Sheprd") {
                                requireFaceID = on
                                model.appLockChanged()
                            }
                        }
                    }
                ))
            } footer: {
                Text("Asks for Face ID when the app opens and hides it in the app switcher. \"Always\" answers ask either way.")
            }
            Section("Storage") {
                LabeledContent("Cached chats and lists", value: ByteCountFormatter.string(fromByteCount: Int64(cacheSize), countStyle: .file))
                Button("Clear Cache", role: .destructive) {
                    model.fleet.clearCache()
                    model.drafts = [:]
                    cacheSize = 0
                }
            }
        }
        .navigationTitle("Privacy & Storage")
        .navigationBarTitleDisplayMode(.inline)
        .task { cacheSize = model.fleet.cache.size() }
    }
}
