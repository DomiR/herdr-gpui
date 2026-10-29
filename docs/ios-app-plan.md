# Plan: Herdr Remote for iOS

Turn `ios/` from a Live Activity companion into the main way to use remote
control on an iPhone or iPad: everything the web app does, natively, plus
what only a native app can do. Pairing works over Bluetooth when the phone
is near the computer, over the local network without a camera, or with the
QR code as today. Bluetooth also keeps working after pairing.

This supersedes "It does not duplicate the chat UI" in
`docs/remote-control-plan.md` (iOS Live Activities). The web app stays for
Android and for phones without the app. Both talk to the same daemon,
`herdr-remote-control`.

## Where things stand

| Piece | Today |
| --- | --- |
| `ios/` | About 1,600 lines of Swift. QR pairing, a Secure Enclave device key, pinned TLS, push token upload, and Live Activity layouts. Tapping anything opens the web app in Safari. iOS 17.2, Swift 5, iPhone only. |
| Daemon web app | Server-rendered HTML fragments (`/f/...`) plus an SSE stream of fragment patches. JSON is used only for commands, pairing, and push. A native client has nothing typed to read. |
| Typed data | `herdr-chat-protocol` already has serde JSON for `Transcript`, `Entry`, `Change`, and a shared reducer. The hub keeps a `Layout` (workspaces, chats, terminals) and per-chat `View`s. The fragments render from these. |
| APNs | `src/apns` signs tokens, decides start/update/end, and posts. It is not yet wired to `POST /api/apns` or to the notifier. |
| Moltis iOS (`~/code/moltis/apps/ios`) | XcodeGen plus `just` recipes. An actor-based WebSocket RPC with continuation maps and typed events. Capped backoff with readable connection diagnostics. Bonjour discovery with a TLS trust probe. A Live Activity "step stack" (current plus previous two steps, debounced). A tok/s footer, and tool names mapped to SF Symbols. No tests, no APNs, no markdown, no Observation, and not Swift 6. Borrow the first five ideas, not the gaps. |

## Decisions

Settled with the user on 2026-10-28: iOS 26 minimum; cache the last
state and open instantly; Bluetooth pairing on macOS only for now;
presence holds phone alerts by default once it is on; TestFlight under
the user's team, built and uploaded by CI.


- **iOS 26 minimum, Swift 6 language mode, complete strict concurrency.**
  This brings Liquid Glass, `@Observable`, on-device `SpeechAnalyzer`,
  interactive Live Activities, Controls, and current App Intents. It is a
  self-built or TestFlight developer tool, so the oldest iOS versions do not
  need to be reached.
- **iPhone and iPad** from the start (`TARGETED_DEVICE_FAMILY: "1,2"`). The
  iPad uses a three-column split view and works with a hardware keyboard.
  Apple Watch comes later.
- **One typed API, shared by contract.** The daemon gains a JSON model API
  and one WebSocket per computer. The Swift types mirror the Rust serde
  types. The JSON fixtures are checked by both the Rust and the Swift tests,
  as `ChatActivityState` already is. There is no code generator.
- **Port the change reducer to Swift**, so the app applies the same
  `Change`s the desktop and `herdr-acp` apply, against shared fixtures.
- **The trust model stays as it is.** The daemon's CA is pinned by
  fingerprint, the device key stays in the Secure Enclave, and a person
  approves on both screens. The native app needs no certificate profile and
  no Safari handoff.
- **Few dependencies**: Apple's `swift-markdown` (cmark-gfm) and nothing
  else. Everything else is first-party (CryptoKit, Network, CoreBluetooth,
  ActivityKit, AppIntents).

## Architecture

### Targets (XcodeGen, `ios/project.yml`)

| Target | What |
| --- | --- |
| `HerdrRemote` | The app |
| `HerdrRemoteWidgets` | Live Activities, Home and Lock Screen widgets, and Controls |
| `HerdrRemoteNotifications` | Notification Service Extension: decrypts pushes and attaches the agent icon and thumbnails |
| `HerdrRemoteShare` | Share extension: send text, a URL, or images to a chat or a new chat |
| `HerdrRemoteCore` (SPM) | Pure Swift, no UIKit: API types, the reducer, pairing and SAS crypto, address racing, ANSI runs, and the markdown model. `swift test` runs it on the Mac. |
| `HerdrRemoteKit` (SPM) | Shared by the app and its extensions: Keychain and Secure Enclave, the connection actor, session signing, and the App Intents |

One App Group and one Keychain access group are shared by all targets, so
the extensions can sign requests (an Allow from the lock screen) without
opening the app.

### State and concurrency

- One `actor Computer` per paired computer. It owns the address race, the
  session token, the WebSocket, its subscriptions, and reconnection with
  capped backoff (borrowed from Moltis). Each failure is turned into a typed
  diagnostic ("Herdr is not running", "TLS answered as another computer",
  "no route").
- The views read `@Observable @MainActor` stores (`HomeStore`, `ChatStore`,
  `TerminalStore`). They are fed from the actor through `AsyncStream`s with
  latest-value coalescing, never an unbounded queue. This is the same rule
  as Herdr UI's mailbox.
- Requests are correlated by ID through `CheckedContinuation` maps, with a
  bounded number in flight. Cancelling a request never replays it.

### Daemon: the native API (`crates/herdr-remote-control/src/native/`)

This is a new module beside `web/`. It reads the same hub and uses the same
`Device` bearer auth and limits.

- `GET /api/v2/home`: computers, workspaces, chats, terminals, and status
  marks, as serde types next to `hub::layout`. Rendering only becomes an
  HTML concern inside `web/`.
- `GET /api/v2/chat/{pane}`: the transcript snapshot (`herdr_chat_protocol::Transcript`)
  with its revision.
- `GET /api/v2/live` (WebSocket):
  - The client sends `subscribe {home}`, `{chat, pane, after_revision}`, or
    `{term, pane}`, and their `unsubscribe`s.
  - The server sends `home` (whole value, coalesced at 400 ms),
    `chat_changes {pane, revision, changes: [Change]}` (coalesced at 120 ms),
    `term_screen {pane, rows: [[Run]]}` (only when it changed, 300 ms), and
    `reload {view}` when the client lagged past the bounded queue.
  - Commands keep the existing `POST` routes. The socket carries
    subscriptions and data, not writes, so one rate-limit and `Origin`
    policy still covers every write.
  - It is one socket per computer instead of one SSE stream per view.
    Switching views is then a message, not a new connection with a new TLS
    handshake. This is a concrete need, not a speculative transport: axum's
    `ws` feature, under the existing Tokio.
- Terminal screens are sent as styled runs `{text, fg, bg, bold, italic,
  underline, inverse}`. They come from the parser `web/render/ansi.rs`
  already has, which moves into a shared `ansi` module both renderers use.
  Swift draws runs and never parses escapes.
- `POST /api/v2/notify-key`: the device's notification key-agreement public
  key (see Notifications).
- Fixtures: `ios/HerdrRemoteCore/Tests/.../Fixtures/v2/*.json`, generated
  from the Rust types in a test and compared byte for byte, then decoded in
  Swift. A field added on one side without the other fails CI.

## Connectivity

- Each computer stores every address it reported: LAN, Tailscale,
  `HOST.local`. The app races them (connections staggered by about 250 ms)
  and keeps the first that answers `/api/ping` with the right fingerprint.
  `NWPathMonitor` reruns the race when the network changes, so moving from
  Wi-Fi to cellular plus Tailscale picks the other address without the user
  noticing.
- **Bonjour**: the daemon advertises `_herdr-remote._tcp` through `mdns-sd`
  (already a workspace dependency) with a TXT record holding a short
  fingerprint hash and the protocol version. The app browses with
  `NWBrowser`. This finds a computer whose DHCP address changed, and it is
  how network pairing discovers computers. It can be turned off in
  Settings > Remote control.
- **Bluetooth** helps when there is no IP path (see Bluetooth after
  pairing).
- In the background, the app keeps no sockets: pushes wake what needs
  waking. In the foreground, the app holds a live socket to the computer it
  shows and polls `home` from the others every 30 s, to keep their badges
  current.

## Pairing

There are three ways in. All end in the same device record, Secure Enclave
key, and pinned CA.

| Way | When | Channel that authenticates the computer |
| --- | --- | --- |
| **Nearby (Bluetooth)** | Phone near the computer, no camera needed | SAS: a six-digit code compared on both screens, with commitments |
| **Same network (Bonjour)** | Bluetooth off or unavailable on the desktop | The same SAS, carried over the daemon's TLS port without pinning |
| **QR code** | Anywhere, including remote over Tailscale | The camera reads the fingerprint off the computer's screen (as today) |

### Why nearby pairing needs a new code exchange

The current matching code mixes in a server nonce chosen *after* the
server sees the device key. Over the QR path that is fine, because pinned
TLS already proves who the server is. Over an unauthenticated channel
(Bluetooth, Bonjour), a man in the middle could run a fake daemon,
relay the claim to the real one, and grind its nonce until the phone shows
the code the desktop shows: a million tries is cheap. Nearby pairing
therefore uses a numeric-comparison SAS with commitments, as Bluetooth LE
Secure Connections does:

1. The phone (A) and the daemon (B) exchange ephemeral P-256 public keys.
   B's hello also carries its CA fingerprint, name, and addresses.
2. B sends `Cb = SHA-256("herdr-sas-commit" ‖ PKb ‖ PKa ‖ Nb)`.
3. A sends `Na`. B reveals `Nb`, and A checks `Cb`.
4. Both show `code = HKDF(Na ‖ Nb ‖ PKa ‖ PKb ‖ fingerprint) mod 10⁶`.
   A man in the middle gets one guess in a million. The fingerprint is part
   of the transcript, so a matching code also authenticates the CA.
5. A key derived from ECDH plus the nonces encrypts the rest (ChaCha20-Poly1305):
   B sends a one-time claim token. A then claims over pinned TLS with its
   Secure Enclave key, exactly like the QR claim, tied to this SAS session.
6. The desktop dialog shows that one code with Allow and Deny, and the phone
   shows the same code. No second code appears.

The daemon runs both sides of the protocol (`src/auth/sas.rs`; `p256`,
`hkdf`, and `aes-gcm` are already there, plus `chacha20poly1305`). Herdr UI
only carries bytes between Bluetooth and the daemon's control socket, so
"Herdr UI never holds secrets" still holds. On iOS everything is CryptoKit.
The protocol is pure functions in `HerdrRemoteCore` and `auth::sas`, tested
against shared vectors, including a relayed man-in-the-middle that must
fail.

### Bluetooth on the desktop

The Mac is the peripheral and the phone the central, because iOS supports
central mode best in the background (state restoration, long-lived
connections).

- `crates/herdr-remote-control` (the non-`server` part Herdr UI already
  uses) gains `nearby/`. It holds the GATT service definition, framing
  (chunks up to the negotiated MTU, length-prefixed, bounded), and the relay
  to the control socket. Platform backends:
  - **macOS first**: `CBPeripheralManager` through `objc2-core-bluetooth`
    (objc2 is already in the workspace). The delegate class is the only
    unsafe-adjacent code; keep it tiny and document its invariants. The
    bundle's Info.plist needs `NSBluetoothAlwaysUsageDescription`. The
    daemon is a bare LaunchAgent binary with no bundle, which TCC would
    refuse, so Bluetooth lives in Herdr UI.
  - **Linux next**: BlueZ's D-Bus GATT server API through `zbus`, not
    `bluer`, because `bluer` brings Tokio into the GUI.
  - **Windows**: WinRT `GattServiceProvider`. Until it is built, the pairing
    dialog says Bluetooth pairing is not available there and offers the
    network and QR paths. Nothing pretends to work.
- GATT service (one fixed 128-bit UUID):
  - `info` (read): protocol version, and the computer's name only while the
    pairing dialog is open.
  - `pair` (write and notify): SAS frames.
  - `presence` (write and notify): challenge and signed answer for paired
    devices, and encrypted current addresses.
  - `l2cap` (read): the PSM of the fallback channel, when it is on.
- Advertising runs only while the pairing dialog is open, or while presence
  is on (opt-in) for at least one paired device. The advertisement carries
  only the service UUID, which iOS background scanning needs. The
  computer's name and identity never appear unauthenticated.

### Flow on the phone

1. Tap **+ Add computer**. Nearby computers appear (Bluetooth or Bonjour,
   merged by fingerprint hash) with a "Scan QR" button below.
2. Tap one. The desktop's pairing dialog must already be open: an
   advertisement exists only then, and Bonjour pairing is refused
   otherwise. The phone then shows the six-digit code with a haptic tick.
3. Allow on the desktop. The phone shows the computer's home at once.

## Bluetooth after pairing

All of this is opt-in per device, under Settings > Remote control on both
sides.

- **Presence: notifications follow you.**
  - The phone keeps a background Bluetooth connection to each paired
    computer it is near, and proves itself by signing the `presence`
    challenge with its device key.
  - Herdr UI reports to the daemon whether the phone is "near" (connected,
    RSSI above a threshold, with hysteresis) and whether the desktop is in
    use (input in the last N minutes).
  - The notifier then holds back phone alerts while you are at the desk,
    and sends them when you walk away. This hold is on by default once
    presence is on. Live Activities still update quietly.
  - This is the "Away" behaviour the orchestrator left for later, with a
    real signal instead of a timer.
- **Address refresh and wake.**
  - The `presence` characteristic sends the computer's current addresses,
    encrypted to the device. A computer whose IP changed is found again
    even where Bonjour is filtered.
  - Core Bluetooth state restoration relaunches the app in the background
    when the computer comes back in range. The app then refreshes its home
    badges and Live Activity tokens.
- **Fallback transport (after a spike).**
  - An L2CAP channel, piped by Herdr UI to the daemon's loopback port. The
    phone runs its ordinary pinned TLS through a loopback `NWListener`, so
    there is no second protocol and the encryption is end-to-end.
  - This is for when there is no IP path: a plane, hotel Wi-Fi with client
    isolation, or a laptop without network.
  - The bandwidth suits chats, questions, and approvals. Terminals fall
    back to one screen per second.
  - Shown as "Connected over Bluetooth" in the connection banner.

## Feature parity with the web app

Every row ships before the app is called complete. Native mockup picks
(round 1, `ios/Mockups/`, chosen 2026-10-28), all B:

- **Pairing B**: a proximity card, like AirPods setup, rises when a Mac in
  pairing mode is near (Pair / Not now, Bluetooth or Wi-Fi badge), and the
  same card turns into the six-digit code with "They don't match".
- **Home B**: attention first. "Needs you" cards answer inline (Allow once,
  Deny, Always…, or full-width options), then "Working" with progress, then
  "Recent". A tab bar holds Home, Board, Computers, and Search; the board
  (web pick D) becomes its own tab and Computers lists every computer's
  workspaces (the web's grouped home).
- **Chat B**: a document. The prompt as a heading, steps folded to one
  summary line ("5 steps · 3 files changed · +140 −3"), the running step
  pinned under the header as a glass capsule, diffs inline, earlier turns
  summed up at the end.
- **Live Activity B**: the step stack (current step with the previous two
  fading above), plan progress, elapsed time and cost, and the computer's
  counts. A question or permission still adds its answer buttons below.

Web picks that carry over: questions A (bottom sheet), terminal B/A/D.

Round 2 picks (2026-10-28):

- **Computers tab C**: an outline (sidebar list style) from computer to
  workspace to chats and terminals, with CPU and counts on each computer
  row and New chat inside each workspace.
- **New workspace A**: one form sheet: computer, project (opens the
  searchable folder list), new worktree with its branch, an agent grid,
  and an optional first prompt; Create in the toolbar.
- **Session sheet C**: cards: a large context ring with model, tokens, cost,
  and tok/s; mode tiles 2x2; an effort slider; auto-accept as a red card
  that asks for Face ID.
- **Terminal A**: full bleed with the two key rows docked; paste and the
  action sheet in the header.
- **Settings A**: one grouped list: computers, notifications (encrypted,
  hold at desk, lock screen detail, Live Activities), presence, Face ID,
  storage, about and diagnostics.
- **iPad B**: the attention list and chat B, with the pane's terminal in an
  inspector beside the chat.

| Web app | Native |
| --- | --- |
| Home: needs-you strip, computers > workspaces > tabs, status marks, ⊕ new chat, collapse | `List` with sections per computer. Swipe actions (mute, close tab), context menus (new chat here, open on board), status marks from `look`, pull to refresh, search with `.searchable` |
| Attention board | The same three groups. Inline Allow once and Deny with haptics, matched-geometry transition into the chat |
| Chat focus layout: latest turn, step timeline, folded earlier turns, markdown, tool calls, diffs, queued prompts | `swift-markdown` into native blocks. Code blocks scroll sideways with Copy. Diffs show added and removed lines. Tool rows use SF Symbols (borrowed from Moltis). Text appended to the streaming answer is rendered without a reflow jump. Text is selectable, and long-press copies a block. |
| Header: title, status, context ring, session line, session sheet | Toolbar title menu that opens a sheet with detents. Effort is a segmented control, the mode list shows descriptions, and auto-accept asks for confirmation (Face ID optional) |
| Questions bottom sheet | `.sheet` at a medium detent that opens by itself. Full-width option buttons, with "always" options set apart |
| Composer: send and stop, photos, pasted screenshots, `/` completion | Native text view with the keyboard toolbar, `PhotosPicker`, the camera, paste including images (`UIPasteControl`, no Safari prompt), `/` completion, and **dictation** with `SpeechAnalyzer` on device |
| New workspace and new chat | The project picker with search (recent roots, then folders), a branch for a worktree, and an agent grid with the agents' icons |
| Terminal tabs | A Core Text view of the runs, fit to width, pinch to zoom, and selection. Two fixed key rows (B) above the keyboard. Hardware keys (Esc, Ctrl, arrows, ⌘V) through `UIKeyCommand` on iPad. The action sheet (A). Paste and image upload as today |
| Several computers, SSH hosts | All computers merged on home, not one at a time. SSH hosts as their own sections |
| Settings | Notifications (per chat and per computer, privacy), presence, Bonjour, Face ID, the paired computers with their fingerprints, unpair |
| Reconnecting banner | A thin glass banner that says why, such as "No route to MacBook (LAN, Tailscale tried)" |

## Beyond the web app

- **Interactive Live Activities.**
  - Finish the APNs wiring (the "still to do" in the remote-control plan).
  - When a question or a permission waits, the lock screen and the expanded
    Dynamic Island show Allow once, Deny, or up to three answers as
    `Button(intent:)`. A `LiveActivityIntent` signs and posts from the
    app's process without opening it.
  - Add the step stack (current step plus the previous two) from Moltis.
- **Actionable, encrypted notifications** (APNs, replacing Web Push for
  this device):
  - Categories: permission (Allow once, Deny, and Always, which needs
    authentication), question (up to four answers), and finished (a text
    Reply that becomes the next prompt).
  - Encryption: the payload is encrypted to a second Secure Enclave key, for
    key agreement, in the shared access group. The Notification Service
    Extension decrypts it, so Apple sees only "Herdr" and ciphertext.
  - The existing privacy setting decides how much the decrypted text shows.
  - Thread identifiers are set per chat and interruption levels are set per
    kind (a question is time-sensitive).
  - Badges show the needs-you count.
- **Widgets and Controls.**
  - Home and Lock Screen widgets: needs-you count and working chats per
    computer.
  - Controls (Control Center, Lock Screen, Action button): open the board,
    new chat on the last computer, and dictate a prompt.
- **App Intents, Siri, Shortcuts, Spotlight.**
  - `AppEntity` types for computers, workspaces, and chats.
  - Intents: "Ask Claude in herdr-gpui to …", "What needs me?", "Stop the
    running chat", and "Answer the question".
  - `IndexedEntity` makes chats searchable in Spotlight.
- **Share extension**: send a page, text, or screenshots to a chat as a
  prompt, or to a new chat in a chosen workspace.
- **Face ID** gate, optional: on opening the app, and always before "Allow
  always" or turning on auto-accept.
- **iPad**:
  - `NavigationSplitView` with computers and workspaces, then chats, then
    the chat or terminal.
  - Keyboard shortcuts with the ⌘ discoverability overlay, pointer hover,
    and Stage Manager windows (one per chat, through `openWindow`).
- **Liquid Glass** toolbars and banners. The app follows the desktop theme
  sent with `look`, with system light and dark, Dynamic Type, VoiceOver
  labels on every status mark, and Reduce Motion.
- **Later**: an Apple Watch app (needs-you list, approve with a tap, and
  dictated answers) and the terminal's "Size for phone" once Herdr has an
  active non-sizing surface.

## Data and privacy

- **Keychain**:
  - Device keys live in the Secure Enclave. The signing key is
    non-exportable and stays this-device-only, and it is never synced to
    iCloud: each device pairs itself.
  - Sessions and fingerprints use `AfterFirstUnlockThisDeviceOnly`, so the
    extensions can sign while the phone is locked.
- **Cache, for speed**: the last `home` of every computer and the last 200
  entries of recently opened chats (bounded by size, oldest chat evicted
  first), as one file per computer under the App Group with
  `NSFileProtectionComplete`. Launch draws from it in the first frame, with
  a thin "Updating" mark, then the live socket resumes each chat from its
  cached revision, so only the changes since cross the network. Images go
  to a bounded disk cache keyed by content hash. Unpairing a computer
  deletes its files; one switch in Settings clears all of it.
- **Logs**: `os.Logger` with private fields redacted. "Send diagnostics"
  posts the last bounded log to `/api/log` (which exists) on the user's
  request only.

## Herdr UI changes

- The pairing dialog gets a "Nearby" state: advertising while it is open,
  then the SAS code with Allow and Deny. The QR stays in the same dialog.
- Settings > Remote control:
  - Each device shows its kind (iOS app or web app) and whether it is near.
  - New switches: Bluetooth presence, "Hold phone notifications while I'm
    at this computer", and Bonjour advertising.
- Control protocol: Bluetooth relay frames, and presence and desktop-in-use
  reports to the daemon.
- CLI:
  - `herdr-gpui remote devices --json` gains `kind` and `near`.
  - `herdr-gpui remote pair` gains `--nearby`, which opens the same dialog.
    The user still approves in the dialog.
  - Both go in `docs/cli.md`.

## Engineering

- The 1,000-line limit already covers Swift (`just check-file-size`). Split
  by responsibility.
- Swift Testing (`@Test`) in the packages. `ViewInspector` is not needed:
  test the stores and reducers, and look at the views in the simulator.
- `just` recipes (borrowed from Moltis):
  - `ios-generate` (xcodegen)
  - `ios-test` (`swift test` for both packages)
  - `ios-build` (simulator build, unsigned)
  - `ios-lint` (`swift-format lint --strict`)
  - `ios-sim` (the headless simulator loop below)
- CI: a macOS job runs `ios-test` and `ios-build` when `ios/**` or the
  fixtures change. The Rust side checks the same fixtures in `cargo test`.
- **The simulator loop** (from the earlier phone work):
  - Boot a headless simulator and start a sandbox daemon.
  - Pair through `simctl openurl` with a QR link, and approve over the
    control socket.
  - `simctl io screenshot` plus `daemon.log`, with no taps on the shared
    display.
  - Bluetooth does not work in the simulator, so nearby pairing is tested
    by its protocol vectors plus a manual device check.
- `SWIFT_VERSION: 6.0` and `SWIFT_STRICT_CONCURRENCY: complete` from the
  first change. No `@unchecked Sendable` without a comment stating the
  invariant.

## TestFlight from CI

A `ios-testflight` job in `.github/workflows/ios.yml` builds and uploads on
every push to `main` that touches `ios/**` or the v2 fixtures, and on each
release tag; pull requests run only `ios-test` and the unsigned build.

- `macos-26` runner with Xcode 26 (or later) selected explicitly.
- `xcodegen`, then `xcodebuild archive` with automatic signing through an
  App Store Connect API key (`-allowProvisioningUpdates
  -authenticationKeyPath ... -authenticationKeyID ... -authenticationKeyIssuerID ...`),
  so no certificates or profiles are stored in the repository or secrets.
- `xcodebuild -exportArchive` with an `ExportOptions.plist` whose
  `destination` is `upload` and `method` is `app-store-connect`: Xcode
  uploads to TestFlight itself, without fastlane or `altool`.
- `CURRENT_PROJECT_VERSION` is the workflow's run number;
  `MARKETING_VERSION` follows the app's release version.
- Secrets (environment `testflight`, required reviewers off for `main`):
  `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_P8`, `APPLE_TEAM_ID`. The key is
  written to a temporary file and removed in an `always()` step.
- `aps-environment` is `production` for the archive, `development` for
  local runs, set by configuration in `project.yml`.
- Internal testers get each build at once; an external group is a manual
  step in App Store Connect.

## Phases

0. **Spikes**, each with a written result:
   - A macOS GATT peripheral from Herdr.app through objc2, including the
     TCC prompt.
   - iOS background central with state restoration that reconnects to it.
   - L2CAP throughput between iPhone and Mac.
   - A `LiveActivityIntent` that posts over pinned TLS from the lock screen.
   - A Notification Service Extension that decrypts with a Secure Enclave
     key from the shared access group.
1. **Foundation**:
   - Daemon `native/`: v2 home and chat, the live WebSocket, and v2
     fixtures.
   - Swift 6 and iOS 26 shell, and the `Computer` actor with address racing.
   - The reducer port, QR pairing carried over, and read-only home and
     board across every computer.
2. **Nearby pairing**: SAS in `auth::sas` and Swift, Bonjour advertising
   and pairing, and the macOS Bluetooth backend with the dialog's Nearby
   state.
3. **Chat parity**: transcript, composer with photos and dictation, slash
   completion, session sheet, questions, queue, new workspace, and new chat.
4. **Terminal parity**: runs, key rows, hardware keys, actions, paste, and
   upload.
5. **Notifications**: APNs alerts with encryption and actions,
   interactive Live Activities, and badges. Web Push stays for the web app.
6. **Bluetooth after pairing**: presence and the notification hold, address
   refresh and wake, then the L2CAP fallback if its spike holds up.
7. **System**: widgets, Controls, App Intents, Spotlight, the share
   extension, and Face ID.
8. **iPad** polish (split view, windows, keyboard), then the Linux and
   Windows Bluetooth backends, then Apple Watch.

## Tests

- Shared fixtures for every v2 type and every `Change`, decoded and
  re-encoded on both sides. The reducer is run against the same change
  sequences in Rust and Swift, and the results are compared.
- SAS vectors, including the relayed man-in-the-middle with nonce grinding
  (it must fail), a wrong commitment, replay, an oversized frame, and a
  truncated frame.
- `Computer` actor:
  - Address race order.
  - Fingerprint mismatch skipped.
  - Lag that yields `reload`.
  - Cancel without replay.
  - Session expiry mid-stream.
  - Revocation closes the socket.
- Daemon: the WebSocket's auth, subscription bounds, coalescing, lag, and
  per-device stream caps; Bonjour TXT contents; pairing refused while no
  dialog is open.
- Manual, on devices:
  - Bluetooth pairing.
  - Presence walking away and back.
  - Push on a locked phone, and actions from the lock screen.
  - Live Activity buttons.
  - Wi-Fi to cellular plus Tailscale.
  - Revoke while streaming.
  - iPad with a keyboard.

## Open questions

- The App Store Connect API key and team ID, added as the `testflight`
  environment's secrets.
