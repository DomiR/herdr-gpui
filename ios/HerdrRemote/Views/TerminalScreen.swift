import HerdrRemoteCore
import SwiftUI
import UIKit

/// Terminal A, read only for now: the pane's screen full bleed, fit to the
/// width. Typing (the two key rows) and the action sheet come with the
/// terminal phase.
struct TerminalScreen: View {
    @Environment(AppModel.self) private var model
    let key: PaneKey
    let title: String
    @State private var store: TermStore?

    var body: some View {
        GeometryReader { geometry in
            if let screen = store?.screen {
                let size = fit(columns: screen.columns, width: geometry.size.width - 16)
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(screen.rows.enumerated()), id: \.offset) { _, row in
                            Text(row.reduce(into: AttributedString()) { line, run in line += Self.styled(run) })
                                .font(.system(size: size, design: .monospaced))
                                .lineLimit(1)
                                .fixedSize(horizontal: true, vertical: false)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(8)
                    .textSelection(.enabled)
                }
                .defaultScrollAnchor(.bottom)
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Color(red: 0.07, green: 0.07, blue: 0.09))
        .environment(\.colorScheme, .dark)
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.hidden, for: .tabBar)
        .onAppear { store = model.fleet.openTerm(key) }
        .onDisappear {
            if let store { model.fleet.closeTerm(store) }
        }
    }

    /// The point size at which `columns` monospaced cells fill `width`.
    private func fit(columns: Int, width: CGFloat) -> CGFloat {
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
            (foreground, background) = (background ?? Color(red: 0.07, green: 0.07, blue: 0.09), foreground)
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
