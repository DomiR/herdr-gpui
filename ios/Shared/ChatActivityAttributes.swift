import ActivityKit
import Foundation
import SheprdRemoteCore

/// A chat's running turn as a Live Activity. Compiled into both the app and
/// the widget extension; the daemon starts it with push-to-start, naming
/// this type as `attributes-type`.
///
/// Wire shape (see `ChatActivityState` for `content-state`):
///
/// ```json
/// "attributes": { "agent": "Claude", "chatTitle": "remote plan",
///                 "computer": "studio", "computerId": "AB12…(64 hex)",
///                 "pane": "w1-p3", "workspace": "sheprd" }
/// ```
///
/// A computer pushing through the push relay seals both instead, each as
/// `{"sealed": "…"}` (`ActivitySealable`), and only this phone's notify key
/// for that computer opens them (`opened()`). The fields are decoded flat
/// through `ChatActivityFacts`, so the keys are declared once, in
/// `SheprdRemoteCore`, where `swift test` checks them against the daemon's
/// fixtures.
struct ChatActivityAttributes: ActivityAttributes {
    typealias ContentState = ActivitySealable<ChatActivityState>

    var content: ActivitySealable<ChatActivityFacts>

    init(facts: ChatActivityFacts) {
        content = .plain(facts)
    }

    init(from decoder: Decoder) throws {
        content = try ActivitySealable(from: decoder)
    }

    func encode(to encoder: Encoder) throws {
        try content.encode(to: encoder)
    }

    /// The attributes, opened when sealed, for routing the activity's
    /// tokens; `nil` when no key opens them for the computer they name.
    func facts(keys: [SealedChatActivity.Key]) -> ChatActivityFacts? {
        SealedChatActivity.facts(content, keys: keys)?.0
    }

    /// What to draw, or `nil` for the fixed cover. While this phone uses
    /// the push relay, only sealed content shows. The keys are read only
    /// for sealed content, so plain content (the debug sample, or an older
    /// computer's) costs no keychain read per render.
    func opened(_ state: ContentState) -> SealedChatActivity.Opened? {
        let sealed = content.plain == nil || state.plain == nil
        return SealedChatActivity.open(
            attributes: content,
            state: state,
            keys: sealed ? NotifyKey.activityKeys() : [],
            allowsPlain: !PushRelaySetting.isOn
        )
    }
}
