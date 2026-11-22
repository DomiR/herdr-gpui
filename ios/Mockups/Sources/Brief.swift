import SwiftUI

// A chat another agent started, whose first prompt is a long brief.
// Today it shows as the turn's title3 heading, a wall of large text, with
// the wrapper tag Claude puts around pasted text. A: a brief card with a
// faded preview that expands in place. B: the first sentence as the heading,
// the rest as body text clamped under it. C: the brief leaves the stream for
// a capsule under the title that opens it in a sheet. D: the brief as an
// outline: the goal, then folded sections.
// State 0 is how the chat opens; state 1 is after a tap.

private enum BriefSample {
    static let goal = "Make the iOS app use the hosted push relay, so a TestFlight or App Store install gets sealed alerts and Live Activities from a computer with no APNs key."
    static let context = "Branch feat/ios-push-relay is checked out in this worktree, based on origin/main. The server side is merged and deployed."
    static let readFirst = [
        ("docs/apns-relay-plan.md", "Design, Opt-in, Live Activity privacy, Acceptance"),
        ("docs/ios-spikes.md", "the widget gets errSecNotAvailable from the keychain"),
        ("ios/Spikes/Shared/NotifyKey.swift", "loadAny, mirror"),
    ]
    static let steps = [
        "Attest once with App Attest and make handles on the phone",
        "Mirror the Secure Enclave key's handle to an App Group file",
        "Send sealed alerts and Live Activity updates through the relay",
        "Opt-in switch in Settings, off without a relay",
    ]
    static let acceptance = [
        "A TestFlight install gets an alert from a computer with no APNs key",
        "The Live Activity updates on the lock screen",
        "No token or payload reaches the relay's logs",
    ]
    static let words = 412
    static let reply = "I read the plan and the spikes. The widget can't use the keychain, so I'll mirror the key's handle to the App Group file first, then wire the relay client. Starting with `NotifyKey`."
}

private struct BriefChrome<Content: View>: View {
    var title = "iOS hosted push relay"
    @ViewBuilder var content: Content

    var body: some View {
        NavigationStack {
            content
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .principal) {
                        VStack(spacing: 0) {
                            Text(title).font(.headline)
                            Text("Opus 5.5 · Auto · Bypass permissions").font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    ToolbarItem(placement: .topBarTrailing) { ContextRing(fraction: 0.18) }
                }
                .safeAreaInset(edge: .bottom) { Composer() }
        }
    }
}

/// Who wrote the brief: an agent in another tab, not the person.
private struct BriefOrigin: View {
    var body: some View {
        HStack(spacing: 6) {
            AgentIcon(agent: .claude, size: 18)
            Text("Brief from Claude").font(.subheadline.weight(.semibold))
            Text("· Studio").font(.subheadline).foregroundStyle(.secondary)
        }
        .lineLimit(1)
    }
}

private struct BriefBody: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(BriefSample.context)
            BriefList(title: "Read first", symbol: "doc.text", items: BriefSample.readFirst.map { "`\($0.0)` — \($0.1)" })
            BriefList(title: "Steps", symbol: "list.number", items: BriefSample.steps)
            BriefList(title: "Acceptance", symbol: "checkmark.seal", items: BriefSample.acceptance)
        }
        .font(.subheadline)
    }
}

private struct BriefList: View {
    var title: String
    var symbol: String
    var items: [String]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(title, systemImage: symbol).font(.subheadline.weight(.semibold))
            ForEach(items, id: \.self) { item in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(verbatim: "•").foregroundStyle(.secondary)
                    Text(.init(item))
                }
            }
        }
    }
}

private struct BriefReply: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Label("4 steps", systemImage: "list.bullet.indent").font(.subheadline).foregroundStyle(.secondary)
            Divider()
            Text(.init(BriefSample.reply)).lineSpacing(3)
        }
    }
}

// MARK: A, card

struct BriefCard: View {
    var state: Int

    var body: some View {
        BriefChrome {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 10) {
                        BriefOrigin()
                        Text(BriefSample.goal).font(.headline)
                        if state == 0 {
                            Text(BriefSample.context).font(.subheadline).foregroundStyle(.secondary).lineLimit(2)
                            Button { } label: {
                                Label("Read the brief · \(BriefSample.words) words", systemImage: "chevron.down")
                                    .font(.subheadline.weight(.medium))
                            }
                        } else {
                            BriefBody()
                            Button { } label: {
                                Label("Fold", systemImage: "chevron.up").font(.subheadline.weight(.medium))
                            }
                        }
                    }
                    .padding(14)
                    .background(.fill.quinary, in: .rect(cornerRadius: 16))
                    .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(.separator))
                    BriefReply()
                }
                .padding(.horizontal, 20).padding(.top, 12).padding(.bottom, 80)
            }
        }
    }
}

// MARK: B, clamped heading

struct BriefClamped: View {
    var state: Int

    var body: some View {
        BriefChrome {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    HStack(alignment: .top, spacing: 10) {
                        Rectangle().fill(.tint).frame(width: 3)
                        VStack(alignment: .leading, spacing: 8) {
                            Label("From Claude in another tab", systemImage: "arrow.turn.down.right")
                                .font(.caption).foregroundStyle(.secondary)
                            Text(BriefSample.goal).font(.title3.weight(.semibold))
                            if state == 0 {
                                Text("\(BriefSample.context) Read first: docs/apns-relay-plan.md (whole), especially Design (attest once, make handles on the phone), Opt-in, Live Activity privacy…")
                                    .font(.subheadline).foregroundStyle(.secondary)
                                    .lineLimit(4)
                                    .mask(LinearGradient(stops: [.init(color: .black, location: 0.55), .init(color: .clear, location: 1)],
                                                         startPoint: .top, endPoint: .bottom))
                                Button("Show all \(BriefSample.words) words") { }.font(.subheadline.weight(.medium))
                            } else {
                                BriefBody().foregroundStyle(.secondary)
                                Button("Show less") { }.font(.subheadline.weight(.medium))
                            }
                        }
                    }
                    .fixedSize(horizontal: false, vertical: true)
                    BriefReply()
                }
                .padding(.horizontal, 20).padding(.top, 12).padding(.bottom, 80)
            }
        }
    }
}

// MARK: C, capsule and sheet

struct BriefCapsule: View {
    var state: Int

    var body: some View {
        BriefChrome {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    Text(BriefSample.goal).font(.title3.weight(.semibold))
                    BriefReply()
                }
                .padding(.horizontal, 20).padding(.top, 60).padding(.bottom, 80)
            }
            .overlay(alignment: .top) {
                HStack(spacing: 8) {
                    AgentIcon(agent: .claude, size: 20)
                    Text("Brief from Claude").font(.subheadline.weight(.medium))
                    Text("\(BriefSample.words) words").font(.caption).foregroundStyle(.secondary)
                    Spacer()
                    Image(systemName: "doc.text.magnifyingglass").foregroundStyle(.secondary)
                }
                .padding(.horizontal, 14).padding(.vertical, 9)
                .glassEffect(.regular, in: .capsule)
                .padding(.horizontal, 16)
            }
            .sheet(isPresented: .constant(state == 1)) {
                NavigationStack {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 16) {
                            BriefOrigin()
                            Text(BriefSample.goal).font(.title3.weight(.semibold))
                            BriefBody().font(.body)
                        }
                        .padding(20)
                    }
                    .navigationTitle("Brief").navigationBarTitleDisplayMode(.inline)
                    .toolbar {
                        ToolbarItem(placement: .topBarLeading) { Image(systemName: "doc.on.doc") }
                        ToolbarItem(placement: .topBarTrailing) { Image(systemName: "xmark") }
                    }
                }
                .presentationDetents([.medium, .large])
            }
        }
    }
}

// MARK: D, outline

struct BriefOutline: View {
    var state: Int

    var body: some View {
        BriefChrome {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 0) {
                        VStack(alignment: .leading, spacing: 8) {
                            BriefOrigin()
                            Text(BriefSample.goal).font(.headline)
                        }
                        .padding(14)
                        Divider()
                        section("Context", symbol: "info.circle", count: nil, open: false) { Text(BriefSample.context) }
                        Divider().padding(.leading, 42)
                        section("Read first", symbol: "doc.text", count: BriefSample.readFirst.count, open: state == 1) {
                            VStack(alignment: .leading, spacing: 8) {
                                ForEach(BriefSample.readFirst, id: \.0) { file in
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(file.0).font(.caption.monospaced())
                                        Text(file.1).font(.caption).foregroundStyle(.secondary)
                                    }
                                }
                            }
                        }
                        Divider().padding(.leading, 42)
                        section("Steps", symbol: "list.number", count: BriefSample.steps.count, open: false) { EmptyView() }
                        Divider().padding(.leading, 42)
                        section("Acceptance", symbol: "checkmark.seal", count: BriefSample.acceptance.count, open: false) { EmptyView() }
                    }
                    .background(.fill.quinary, in: .rect(cornerRadius: 16))
                    BriefReply()
                }
                .padding(.horizontal, 20).padding(.top, 12).padding(.bottom, 80)
            }
        }
    }

    private func section(_ title: String, symbol: String, count: Int?, open: Bool,
                         @ViewBuilder content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image(systemName: symbol).frame(width: 18).foregroundStyle(.tint)
                Text(title).font(.subheadline.weight(.medium))
                Spacer()
                if let count { Text("\(count)").font(.caption.monospacedDigit()).foregroundStyle(.secondary) }
                Image(systemName: open ? "chevron.up" : "chevron.down").font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            if open { content().font(.subheadline).padding(.leading, 28) }
        }
        .padding(.horizontal, 14).padding(.vertical, 11)
    }
}

// MARK: Now

/// What the app shows today: the brief as the heading, tag and all.
struct BriefNow: View {
    var body: some View {
        BriefChrome {
            ScrollView {
                HStack(alignment: .top, spacing: 10) {
                    Rectangle().fill(.tint).frame(width: 3)
                    Text("<pasted_content id=\"0e15\">\nTask: \(BriefSample.goal) \(BriefSample.context)\n\nRead first:\n- docs/apns-relay-plan.md (whole), especially Design (attest once, make handles on the phone), Opt-in, Live Activity privacy, Changes by component > iOS, Acceptance.")
                        .font(.title3.weight(.semibold))
                }
                .fixedSize(horizontal: false, vertical: true)
                .padding(.horizontal, 20).padding(.top, 12).padding(.bottom, 80)
            }
        }
    }
}

// MARK: Mix, picked: A's card, D's sections, C's sheet

/// The card in the stream: who sent it, the goal, and D's section rows with
/// their counts. Any row, or "Read the brief", opens C's sheet.
struct BriefMix: View {
    var state: Int

    var body: some View {
        BriefChrome {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 0) {
                        VStack(alignment: .leading, spacing: 8) {
                            BriefOrigin()
                            Text(BriefSample.goal).font(.headline)
                        }
                        .padding(14)
                        Divider()
                        row("Read first", symbol: "doc.text", count: BriefSample.readFirst.count)
                        Divider().padding(.leading, 42)
                        row("Steps", symbol: "list.number", count: BriefSample.steps.count)
                        Divider().padding(.leading, 42)
                        row("Acceptance", symbol: "checkmark.seal", count: BriefSample.acceptance.count)
                        Divider()
                        HStack {
                            Label("Read the brief", systemImage: "doc.text.magnifyingglass")
                            Spacer()
                            Text("\(BriefSample.words) words").foregroundStyle(.secondary)
                        }
                        .font(.subheadline.weight(.medium)).foregroundStyle(.tint)
                        .padding(.horizontal, 14).padding(.vertical, 11)
                    }
                    .background(.fill.quinary, in: .rect(cornerRadius: 16))
                    .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(.separator))
                    BriefReply()
                }
                .padding(.horizontal, 20).padding(.top, 12).padding(.bottom, 80)
            }
            .sheet(isPresented: .constant(state == 1)) {
                NavigationStack {
                    ScrollView {
                        VStack(alignment: .leading, spacing: 16) {
                            BriefOrigin()
                            Text(BriefSample.goal).font(.title3.weight(.semibold))
                            BriefBody().font(.body)
                        }
                        .padding(20)
                    }
                    .navigationTitle("Brief").navigationBarTitleDisplayMode(.inline)
                    .toolbar {
                        ToolbarItem(placement: .topBarLeading) { Image(systemName: "doc.on.doc") }
                        ToolbarItem(placement: .topBarTrailing) { Image(systemName: "xmark") }
                    }
                }
                .presentationDetents([.medium, .large])
            }
        }
    }

    private func row(_ title: String, symbol: String, count: Int) -> some View {
        HStack(spacing: 10) {
            Image(systemName: symbol).frame(width: 18).foregroundStyle(.tint)
            Text(title).font(.subheadline.weight(.medium))
            Spacer()
            Text("\(count)").font(.caption.monospacedDigit()).foregroundStyle(.secondary)
            Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
        }
        .padding(.horizontal, 14).padding(.vertical, 11)
    }
}
