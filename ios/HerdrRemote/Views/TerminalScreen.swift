import HerdrRemoteCore
import PhotosUI
import SwiftUI
import UIKit

/// Terminal A: the pane's screen full bleed, fit to the width and pinched
/// larger or smaller, with the two key rows docked above the keyboard,
/// paste and pictures in the header, the tab's other panes as chips, and
/// the Herdr action sheet.
struct TerminalScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let key: PaneKey
    let title: String
    @State private var store: TermStore?
    @State private var typing: TermTyping?
    @State private var keyboard = false
    /// Pinch zoom over the size that fits the width.
    @State private var zoom: CGFloat = 1
    @GestureState private var pinch: CGFloat = 1
    @State private var actions = false
    @State private var picks: [PhotosPickerItem] = []
    @State private var choosingPhotos = false
    @State private var problem: String?
    @State private var clipboardHasText = UIPasteboard.general.hasStrings
    @State private var renaming = false
    @State private var newName = ""

    var body: some View {
        GeometryReader { geometry in
            if let screen = store?.screen {
                let fitted = fit(columns: screen.columns, width: geometry.size.width - 16)
                let size = min(28, max(4, fitted * zoom * pinch))
                ScrollView(.vertical) {
                    // Sideways only once pinched wider than the screen.
                    ScrollView(.horizontal, showsIndicators: false) {
                        VStack(alignment: .leading, spacing: 0) {
                            ForEach(Array(screen.rows.enumerated()), id: \.offset) { _, row in
                                Text(row.reduce(into: AttributedString()) { line, run in line += Self.styled(run) })
                                    .font(.system(size: size, design: .monospaced))
                                    .lineLimit(1)
                                    .fixedSize(horizontal: true, vertical: false)
                            }
                        }
                        .padding(8)
                        .textSelection(.enabled)
                    }
                    .scrollDisabled(size <= fitted)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .defaultScrollAnchor(.bottom)
                .gesture(
                    MagnifyGesture()
                        .updating($pinch) { value, state, _ in state = value.magnification }
                        .onEnded { value in zoom = min(3, max(0.5, zoom * value.magnification)) }
                )
                .onTapGesture(count: 2) { withAnimation(.snappy) { zoom = 1 } }
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Self.background)
        .background { if let typing { TermKeyboard(typing: typing, shown: $keyboard).frame(width: 1, height: 1).opacity(0) } }
        .safeAreaInset(edge: .top, spacing: 0) { paneChips }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let typing {
                VStack(spacing: 0) {
                    if let reason = problem ?? typing.problem {
                        Text(reason).font(.footnote).foregroundStyle(.orange).padding(6)
                    }
                    let echo = typing.echo.text
                    if !echo.isEmpty { TermEchoStrip(text: echo) }
                    TermKeyRows(typing: typing, keyboard: $keyboard)
                }
            }
        }
        .environment(\.colorScheme, .dark)
        .toolbarColorScheme(.dark, for: .navigationBar)
        // Window titles show in Stage Manager and the app switcher.
        .navigationTitle(LockState.shared.hidesNames ? "Herdr Remote" : name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.hidden, for: .tabBar)
        .toolbar {
            // `PasteButton` draws its own tinted fill and greys out with
            // nothing to paste, so it stands apart from the glass group and
            // only while the clipboard holds text.
            if clipboardHasText {
                ToolbarItem(placement: .topBarTrailing) {
                    PasteButton(payloadType: String.self) { strings in
                        typing?.paste(strings.joined(separator: "\n"))
                    }
                    .labelStyle(.iconOnly)
                    .buttonBorderShape(.circle)
                }
                .sharedBackgroundVisibility(.hidden)
                ToolbarSpacer(.fixed, placement: .topBarTrailing)
            }
            ToolbarItemGroup(placement: .topBarTrailing) {
                Button { choosingPhotos = true } label: { Image(systemName: "photo") }
                    .accessibilityLabel("Send a picture")
                Button { actions = true } label: { Image(systemName: "diamond.fill") }
                    .accessibilityLabel("Herdr actions")
            }
        }
        .confirmationDialog("Herdr", isPresented: $actions, titleVisibility: .hidden) {
            Button("Split Right") { run(.splitRight) }
            Button("Split Down") { run(.splitDown) }
            Button(item?.terminal.zoomed == true ? "Unzoom" : "Zoom") { run(.zoom) }
            Button("New Tab") { run(.newTab) }
            Button("Rename Tab…") {
                newName = item?.terminal.label ?? title
                renaming = true
            }
            if let item {
                Button("New Chat in \(item.workspace.label)") {
                    model.creating = .chat(device: key.device, workspace: item.workspace.id, label: item.workspace.label)
                }
            }
            Button("Close Pane", role: .destructive) { run(.closePane) }
            Button("Close Tab", role: .destructive) { run(.closeTab) }
        }
        .alert("Rename Tab", isPresented: $renaming) {
            TextField("Name", text: $newName)
                .textInputAutocapitalization(.never)
            Button("Cancel", role: .cancel) {}
            Button("Rename") { rename(newName) }
                .disabled(newName.trimmingCharacters(in: .whitespaces).isEmpty)
        } message: {
            Text("The name shows in Herdr and in your lists.")
        }
        .photosPicker(isPresented: $choosingPhotos, selection: $picks, maxSelectionCount: 1, matching: .images)
        .onChange(of: picks) { _, chosen in
            guard let item = chosen.first else { return }
            picks = []
            Task { await upload(item) }
        }
        .onAppear {
            store = model.fleet.openTerm(key)
            if typing == nil { typing = TermTyping(key: key, fleet: model.fleet) }
            #if DEBUG
            // The simulator loop's `-type TEXT` types into the terminal.
            if let text = UserDefaults.standard.string(forKey: "type") { typing?.type(text) }
            #endif
        }
        .onChange(of: store?.screen) { typing?.screenArrived() }
        .onReceive(NotificationCenter.default.publisher(for: UIPasteboard.changedNotification)) { _ in
            clipboardHasText = UIPasteboard.general.hasStrings
        }
        .onReceive(NotificationCenter.default.publisher(for: UIApplication.didBecomeActiveNotification)) { _ in
            clipboardHasText = UIPasteboard.general.hasStrings
        }
        .onDisappear {
            keyboard = false
            if let store { model.fleet.closeTerm(store) }
        }
    }

    /// The tab's name as the computer last sent it, so a rename shows at
    /// once; the route's title until the home arrives.
    private var name: String { item?.terminal.label ?? title }

    /// The terminal tab this pane is in, from the computer's home.
    private var item: TerminalItem? {
        model.fleet.terminalItems.first { $0.computer.device == key.device && $0.terminal.panes.contains { $0.pane == key.pane } }
    }

    /// The tab's panes as chips, when it has more than one.
    @ViewBuilder
    private var paneChips: some View {
        if let item, item.terminal.panes.count > 1 {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach(Array(item.terminal.panes.enumerated()), id: \.element.pane) { index, pane in
                        let current = pane.pane == key.pane
                        Button {
                            guard !current else { return }
                            model.pendingRoute = .term(PaneKey(device: key.device, pane: pane.pane), title: name)
                        } label: {
                            Text(pane.label ?? "Pane \(index + 1)").font(.caption.weight(.medium)).lineLimit(1)
                                .padding(.horizontal, 10).padding(.vertical, 5)
                                .background(current ? AnyShapeStyle(.tint.opacity(0.3)) : AnyShapeStyle(.fill.tertiary), in: .capsule)
                        }
                        .buttonStyle(.plain)
                        .accessibilityAddTraits(current ? .isSelected : [])
                    }
                }
                .padding(.horizontal, 10).padding(.vertical, 6)
            }
            .background(Self.background)
        }
    }

    // MARK: Actions

    private func rename(_ label: String) {
        Task {
            do {
                try await model.fleet.rename(key, to: label)
                problem = nil
            } catch {
                problem = error.localizedDescription
            }
        }
    }

    private func run(_ action: TermAction) {
        let pane = key.pane
        Task {
            do {
                let next = try await model.fleet.call(key.device) { client, token in
                    try await client.termAction(action, pane: pane, bearer: token)
                }
                switch next {
                case nil: dismiss()
                case let next? where next != pane:
                    model.pendingRoute = .term(PaneKey(device: key.device, pane: next), title: name)
                default: break
                }
                problem = nil
            } catch {
                problem = error.localizedDescription
            }
        }
    }

    private func upload(_ item: PhotosPickerItem) async {
        guard let data = try? await item.loadTransferable(type: Data.self), let image = UIImage(data: data),
              let prepared = PreparedImage(image)
        else {
            problem = "That picture could not be read."
            return
        }
        let pane = key.pane
        do {
            try await model.fleet.call(key.device) { client, token in
                try await client.termUpload(prepared.jpeg, pane: pane, bearer: token)
            }
            problem = nil
        } catch {
            problem = error.localizedDescription
        }
    }

    // MARK: Drawing

    static let background = Color(red: 0.07, green: 0.07, blue: 0.09)

    private func fit(columns: Int, width: CGFloat) -> CGFloat { Self.fit(columns: columns, width: width) }

    /// The point size at which `columns` monospaced cells fill `width`.
    static func fit(columns: Int, width: CGFloat) -> CGFloat {
        guard columns > 0 else { return 11 }
        return max(4, min(14, width / (CGFloat(columns) * Self.cellRatio)))
    }

    /// A monospaced cell's width per point of font size, measured once.
    private static let cellRatio: CGFloat = {
        let font = UIFont.monospacedSystemFont(ofSize: 100, weight: .regular)
        return ("M" as NSString).size(withAttributes: [.font: font]).width / 100
    }()

    static func styled(_ run: TermScreen.Run) -> AttributedString {
        var foreground = color(run.fg) ?? Color(white: 0.86)
        var background = color(run.bg)
        if run.inverse {
            (foreground, background) = (background ?? Self.background, foreground)
        }
        var text = AttributedString(run.hidden ? String(repeating: " ", count: run.text.count) : run.text)
        text.foregroundColor = run.dim ? foreground.opacity(0.6) : foreground
        if let background { text.backgroundColor = background }
        if run.underline { text.underlineStyle = .single }
        if run.strike { text.strikethroughStyle = .single }
        var intent: InlinePresentationIntent = []
        if run.bold { intent.insert(.stronglyEmphasized) }
        if run.italic { intent.insert(.emphasized) }
        if !intent.isEmpty { text.inlinePresentationIntent = intent }
        return text
    }

    private static let palette: [String: (Double, Double, Double)] = [
        "black": (0.1, 0.1, 0.1), "red": (0.87, 0.3, 0.3), "green": (0.45, 0.8, 0.4),
        "yellow": (0.9, 0.78, 0.35), "blue": (0.35, 0.55, 0.95), "magenta": (0.78, 0.45, 0.85),
        "cyan": (0.35, 0.78, 0.85), "white": (0.85, 0.85, 0.85),
        "bright_black": (0.45, 0.45, 0.45), "bright_red": (1, 0.45, 0.42), "bright_green": (0.55, 0.92, 0.5),
        "bright_yellow": (1, 0.9, 0.5), "bright_blue": (0.5, 0.68, 1), "bright_magenta": (0.9, 0.6, 1),
        "bright_cyan": (0.5, 0.9, 0.95), "bright_white": (1, 1, 1),
    ]

    private static func color(_ name: String?) -> Color? {
        guard let name else { return nil }
        if let rgb = palette[name] { return Color(red: rgb.0, green: rgb.1, blue: rgb.2) }
        guard name.hasPrefix("#"), name.count == 7, let value = UInt32(name.dropFirst(), radix: 16) else { return nil }
        return Color(
            red: Double((value >> 16) & 0xff) / 255,
            green: Double((value >> 8) & 0xff) / 255,
            blue: Double(value & 0xff) / 255
        )
    }
}
