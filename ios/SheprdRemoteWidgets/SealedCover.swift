import SwiftUI
import WidgetKit

/// The activity shown when its content does not open: sealed to a key
/// this phone no longer has, changed on the way, or sent unsealed while
/// this phone uses the push relay. Fixed text and no buttons, so nothing a
/// push says reaches the lock screen unless its computer sealed it.
struct SealedCover: View {
    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "lock.fill")
                .foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 2) {
                Text("Sheprd")
                    .font(.headline)
                Text("New activity. Open Sheprd to see it.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .padding(14)
    }

    static var island: DynamicIsland {
        DynamicIsland {
            DynamicIslandExpandedRegion(.center) {
                Text("Sheprd").font(.subheadline.weight(.semibold))
            }
        } compactLeading: {
            Image(systemName: "lock.fill")
        } compactTrailing: {
            Text("Sheprd").font(.caption2)
        } minimal: {
            Image(systemName: "lock.fill")
        }
    }
}
