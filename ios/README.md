# Sheprd for iOS

Sheprd is the display name on iPhone, iPad, Apple Watch, widgets, and the share
sheet. The Xcode targets and Swift modules use `SheprdRemote*`.

**Update compatibility:** production keeps `so.pen.herdr.remote` and its
extension suffixes, `group.so.pen.herdr.remote`, Keychain services and access
groups, APNs topics, `_herdr-remote._tcp`, Bluetooth UUIDs, signed-message labels,
and `herdr-remote://` links. These are deployed identities, not display branding.
Do not rename them when preparing an update to the existing App Store app.
The bundle-ID changes below are for an independently signed fork only.

The native app for Sheprd's remote control (`docs/ios-app-plan.md`).
- **What it shows:** the chats and terminal tabs running in Sheprd on every
  paired computer, merged into one list, with what needs you first.
- **What you can do:** answer questions in place, open a chat or a terminal,
  and send prompts.
- **Chat text:** an agent's replies are Markdown with the desktop's text sizes
  (`:big[…]` and the other steps) and message effects (`:::slam` bubbles,
  invisible ink revealed by a tap, and `::confetti` and the other screen
  effects), which play once as a reply arrives and show at rest with a Replay
  link afterwards (`docs/rich-chat.md`, Text size and effects). Reduce Motion
  draws them at rest.
- **Live Activities:** they put running chats on the lock screen and in the
  Dynamic Island.
- **Launch:** it draws the last state from its cache at once, then catches up
  over one live socket per computer.

| Path | What |
| --- | --- |
| `project.yml` | XcodeGen spec for the `SheprdRemote` app and the `SheprdRemoteWidgets` extension: iOS 26, Swift 6 with complete concurrency checking, iPhone and iPad |
| `SheprdRemoteCore/` | Swift package with no UIKit: the transcript model and its reducer, the v2 API's messages, `ComputerLink` (address racing, resubscription, backoff), pairing links, fingerprints, session signatures, sealed notifications |
| `SheprdRemoteCore/Sources/SheprdFleetLog/` | The phone's own copy of the fleet log: replayed and checked by the Rust `sheprd-fleet-ffi` crate, linked as `SheprdFleetFFI` (`tools/build-fleet-ffi.sh` builds it, `just ios-test` and `just ios-build` run it first), drafts checked before Face ID, and sync with any computer of the fleet. Linked by the app alone |
| `SheprdRemoteCore/Sources/SheprdGitHub/` | The phone's GitHub client, its own target over `SheprdRemoteCore` (never the reverse) and linked by the app alone: hosts, leased and signed-in tokens, the GraphQL and REST reads, pull requests, checks, and their logs |
| `SheprdRemoteCore/Sources/SheprdNearby/` | Nearby pairing over Bluetooth: the offer exchange, framing, the short code, and the sightings the scan keeps; the radio is in `SheprdRemote/Nearby/`. Over `SheprdRemoteCore`, linked by the app alone |
| `SheprdRemoteCore/Sources/SheprdRelay/` | Reaching a computer through Sheprd relays: dial proofs, sessions, the local proxy, racing, nearness, and the saved list. Over `SheprdRemoteCore` and `SheprdTunnel`, linked by the app alone; the saved `RelayRecord` itself stays in Core |
| `SheprdRemoteCore/Sources/SheprdTerminal/` | A terminal tab's input and selection: keys, echo, composition, dictation, handles, cells, and waiting. Over `SheprdRemoteCore`, linked by the app alone |
| `SheprdRemoteCore/Sources/SheprdTunnel/` | The end-to-end tunnel inside a relay stream, checked against the Rust `sheprd-tunnel` vectors. It depends on nothing, and Core uses it to check the identity a relay record pins |
| `SheprdRemoteCore/Sources/SheprdVoice/` | Voice commands: the parser, names, the on-device model's schema, drafts, plans, and follow-ups. Over `SheprdRemoteCore`, linked by the app alone |
| `SheprdRemote/` | The app: `Fleet` (one link per computer), `Cache`, the pinned HTTPS and WebSocket client, pairing, Keychain, and `Views/` |
| `SheprdRemoteWidgets/` | The lock screen and Dynamic Island layouts |
| `Shared/` | The Live Activity's attributes, compiled into both targets |
| `Mockups/` | The SwiftUI mockups the screens were picked from (`just ios-mockups`); the `effects` round draws text sizes and message effects with the app's own `Views/Effects` |
| `Spikes/` | Phase 0 spikes (`docs/ios-spikes.md`) |
| `tools/` | The simulator loop's control client and the TestFlight upload scripts |

## Build and test

Requires Xcode 27 or later (App Store uploads need the iOS 27 SDK) and [XcodeGen](https://github.com/yonaskolb/XcodeGen).

Building the app or the package needs the fleet log library from Rust
first, with the pinned toolchain (`rust-toolchain.toml`); `just ios-test` and
`just ios-build` build it.

```sh
just ios-fleet-ffi # the fleet log library (ios/tools/build-fleet-ffi.sh)
just ios-test      # Swift package tests on the Mac, no simulator
just ios-build     # unsigned simulator build of the app and its extensions
just ios-generate  # then open ios/SheprdRemote.xcodeproj
python3 ios/tools/test_branding.py  # generated names and stable deployed identities
```

- **Fixtures:** the package tests decode the JSON fixtures that the
  daemon's Rust tests write from its own types
  (`SheprdRemoteCore/Tests/SheprdRemoteCoreTests/Fixtures/v2`). The reducer is
  checked against the same change sequences on both sides, so the two
  cannot drift apart.
- **Generated files:** the `.xcodeproj`, Info.plists, and entitlements are
  not committed. Edit `project.yml` and run `xcodegen` again.

## The simulator loop

Test against a sandbox daemon in a headless simulator. No taps are needed,
and nothing goes through your phone.

1. **Start the daemon.** Use a short state directory, since Unix socket
   paths are limited:

   ```sh
   cargo build -p sheprd-remote-control --locked
   state=$(mktemp -d /tmp/hrc.XXXXXX)
   ./target/debug/sheprd-remote-control serve --state-dir "$state" --port 7543 2> "$state/daemon.log" &
   ```

    - It reads the local Herdr daemon's workspaces.
   - The macOS firewall blocks an unsigned debug daemon on LAN and
     Tailscale addresses, so the simulator uses `127.0.0.1`.
2. **Pair without the camera.** Build signed to run locally, so the app
   has its entitlements and the simulator's keychain keeps the pairing
   across launches (`just ios-build` is unsigned, and its keychain calls
   fail with "A required entitlement isn't present"):

   ```sh
   cd ios && xcodebuild -project SheprdRemote.xcodeproj -scheme SheprdRemote \
     -destination 'platform=iOS Simulator,id=DEVICE' -derivedDataPath build \
     CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Manual DEVELOPMENT_TEAM= build
   ```

   Then pair. The tool approves as the desktop dialog would:

   ```sh
   link=$(ios/tools/remote_control.py "$state" pair | sed -E 's#https://[^/]+/pair#https://127.0.0.1:7543/pair#')
   xcrun simctl launch DEVICE so.pen.herdr.remote -pairLink "$link"
   ios/tools/remote_control.py "$state" approve
   ```

3. **Drive it with debug launch arguments.**
   - `-openLink 'herdr-remote://chat?id=FINGERPRINT&computer=NAME&pane=PANE'`
     opens a chat, as tapping a Live Activity does.
   - `-openTerm PANE` opens a terminal; `-type TEXT` then types into it
     (a newline presses Enter).
   - `-homeSearch TEXT` types into Home's search, and `-pullRequestSearch TEXT`
     into the Pull Requests tab's. With `-gitHubDemo signedIn`,
     `-gitHubDemoLoading N` holds the Pull Requests tab loading N seconds.
   - `-homeJump N` scrolls Home to the bottom, then jumps to section N as
     its edge index does; `-homeJumpRow YES` jumps to that section's first
     row, as its first dot does, and `-homeJumpSweep YES` first crosses every
     entry from the bottom up, as a drag along the index does.
   - `-tab workspaces|pullRequests|settings` opens that tab, and `-openWorkspace ID`
     opens that workspace's screen in it.
   - `-otherAgents YES` unfolds a workspace's other agents.
   - With `-openWorkspace`, `-openChanges YES` opens its changes; then
     `-openDiff PATH` opens a file's diff, and `-sendLog YES` opens the
     confirmation for the first failing check's log.
   - `-renameWorkspace YES` opens a workspace's rename alert. It lists the
     local Herdr daemon's real workspaces, so pressing Rename changes one.
     `-renamePending NAME` shows a rename to NAME on its way, sending nothing.
     `-askAgentName YES` asks the workspace's most recent agent chat for a
     short name and renames the workspace to it, as the alert's Ask button
     does; that prompts the chat's agent once.
   - `-summarizeFirst YES` asks for the row summary of a workspace's first
     chat with a reply, as its row's swipe does. It runs Claude Code once
     on the computer, which costs a model call.
   - A workspace's screen ends with Close Workspace and, for a linked
     worktree, Delete Worktree. Both act on the paired computer's real
     workspaces, so try them on one made for the purpose.
   - `-newWorkspace YES` opens the new workspace form.
   - `-computerTags work,lab` tags the first computer, `-computerTagFilter work`
     filters Workspaces by a tag, and `-openComputer YES` opens the first
     computer's page from that tab.
   - `-syncBar waiting|failed` shows the sync bar as if a change were on
     its way or had failed.
   - `-voiceSay TEXT` runs a spoken command as if it were heard, since the
     simulator has no microphone, and `-voiceConfirm YES` then presses Start
     which creates what the card shows. Run it
     against a sandboxed Herdr, not your own.
   - `-sampleChat YES` opens a made-up chat that needs no daemon, for
     screenshots of the chat screen without real prompts. `-sampleChat effects`
     opens one with text sizes and message effects at rest; `-samplePlay NAME`
     then asks for the screen effect NAME (`confetti`, `zap`, …) in its last
     reply and plays it and every bubble a second after it opens.
   - `-sampleChat activity` opens a made-up running chat with sub-agents,
     shells, a monitor, a plan, and changed files. In any chat,
     `-agentPanel YES` opens its activity panel (`-agentPanel large` at full
     height), and `-agentPanelScroll plan|changed` scrolls it to a section.
   - With `-openLink`, `-session YES` opens the chat's session sheet and
     `-draft TEXT` types into its composer (`-draft /` lists the slash
     commands).
4. **Read the results.** `xcrun simctl io DEVICE screenshot FILE` captures
   the screen, and `daemon.log` lists every request.
   - The cache sits in the App Group container
     (`xcrun simctl get_app_container DEVICE so.pen.herdr.remote group.so.pen.herdr.remote`).
   - Avoid `simctl openurl`: its confirmation alert needs a tap, and it
     keeps the app inactive until the simulator restarts.

## Voice commands

The waveform button on Home, the Talk to Sheprd Control (for the Action
button), and Siri open a sheet that listens for a command, such as "new
worktree on studio about the login bug, and ask Claude to fix it". Commands
are read as English unless Settings > Voice > Use Phone Language is on, which
is experimental: only the model reads those, and it cannot stop or answer a
chat. They can refer back ("stop it", "same thing on the mini"), name several
computers ("on every host"), and reach agents in terminal tabs, which take
pasted text and Escape but show no structured questions. How long the app
waits for more words depends on what was said (`VoiceEndpoint`). Confirm by
Voice reads the plan aloud and takes a plain yes, for a new worktree only.

| Step | Where | What |
| --- | --- | --- |
| Hear | `SheprdRemote/Voice/VoiceListener.swift` | `DictationTranscriber` on the phone, primed with the fleet's names |
| Read | `SheprdRemoteCore/Sources/SheprdVoice/VoiceParser.swift` | Rules, on every word, so the card fills in during the sentence |
| Read what is left | `VoiceModel.swift` | The on-device model, only for sentences the rules cannot place; its schema lists only what exists |
| Check | `VoiceDraft.adopting` | A name counts only if it was heard, and a task only if the words are the speaker's own. The model cannot stop a chat or answer a question |
| Plan | `VoicePlan.swift` | Fills in what was not said and labels it. Something named that does not exist becomes a question, never a substitute |
| Confirm | `SheprdRemote/Views/Voice/VoiceSheet.swift` | The card is the request. Starting an agent, sending, stopping, and answering take a tap on Start |

`VOICE_EVAL=1 swift test --filter VoiceEvalTests` scores every sentence in
`Fixtures/voice` against the real model, which needs Apple Intelligence on the
Mac. Add new phrasing to `heldout2.json` only after checking it fails or
passes untouched: a set the rules were tuned on stops measuring anything.

## Sign with your own team

Running the app on a device needs a paid Apple Developer account. Pushes
are another matter: computers send them only through the Sheprd push relay,
which holds the APNs key for the published app (`so.pen.herdr.remote`), so a
copy signed by another team gets no pushes. The app has no Web Push
subscription of its own: Web Push reaches only the browser app, installed
and subscribed separately.

1. Set `DEVELOPMENT_TEAM` in `project.yml` (or in Xcode, Signing &
   Capabilities, for both targets).
2. Rename the bundle identifiers if `so.pen.herdr.remote` is taken for your
   team.
   - Change both `so.pen.herdr.remote` and `so.pen.herdr.remote.widgets`;
     the extension's must start with the app's.
   - Change the App Group `group.so.pen.herdr.remote` too.
3. Run on a device. Live Activities and the Secure Enclave do not work fully
   in the simulator; there the app uses a software device key.

## TestFlight

`.github/workflows/ios.yml` uploads every change to `main` that touches the
app, once the repository's `testflight` environment holds these secrets:

| Secret | What |
| --- | --- |
| `APPLE_TEAM_ID` | Your ten-character team |
| `ASC_KEY_ID`, `ASC_ISSUER_ID` | An App Store Connect API key with the App Manager role (Users and Access > Integrations) |
| `ASC_KEY_P8` | The key's `.p8` file, its whole text |
| `IOS_SIGNING_P12_BASE64`, `IOS_SIGNING_P12_PASSWORD` | One Apple Development certificate with its private key, as base64 of a `.p12`, and that file's passphrase |

- **How it signs:** Xcode signs automatically with that key and makes the
  profiles it needs. The archive signs with the stored development
  certificate, imported into a keychain of its own (`ios/tools/signing.sh`,
  which puts the keychain search list back afterwards): a runner starts with
  none, and Xcode would otherwise create a certificate on every upload until
  the team hits Apple's limit ("Your account has reached the maximum number
  of certificates"). The upload refuses to run in CI without it. The export
  signs with the team's cloud-managed distribution certificate.
- **What it uploads:** the export re-signs for distribution and uploads to
  App Store Connect.
- **Build number:** one more than the highest build App Store Connect has
  for the app, asked for with the same key (`ios/tools/next-build.py`), so
  CI and local uploads count up together. Set `BUILD_NUMBER` to choose one.
- **What to Test:** the commits that touched the app since the last build,
  grouped as the release notes are (`ios/tools/testflight_notes.py`). The
  text ends in a `Source:` line naming the commit, which is how the next
  upload finds where to start, so leave that line in if you edit the notes
  in App Store Connect. A build whose notes lack it is skipped for the one
  before it; with none to go on, the notes list the latest 20 commits. The
  upload fails if the notes cannot be set within 30 minutes, though the
  build stays uploaded; the next build's notes then cover both.
- **Running it locally:** `just ios-testflight` does the same with the same
  variables. Any it does not find set come from `pass`: the issuer ID from
  `apple/app-store-connect-issuer-id` and the key from
  `herdr-gpui/apple-api-key-KEYID`.

## Pair

1. In Sheprd, open Remote control > Pair device.
2. In the app, tap +, and scan the QR (or paste its link).
3. Compare the 6-digit code with the computer's dialog and choose Allow
   there.

The app keeps the computer's addresses and its CA fingerprint in the
Keychain, with a device key in the Secure Enclave. It trusts the computer
through the fingerprint alone, so it needs no certificate profile.

### Nearby, without the camera

While the pairing dialog is open, the computer can also be found nearby:

- **Same network:** `_herdr-remote._tcp` with `pairing=1` in its TXT
  record.
  - On macOS, the `sheprd-nearby` helper advertises it through the system.
  - Elsewhere, the daemon advertises it.
- **Bluetooth:** the `sheprd-nearby` helper beside Sheprd advertises the
  nearby service. On macOS it is the Swift helper; on Linux (BlueZ) and
  Windows (WinRT) it is the Rust crate `crates/sheprd-nearby`.

A phone with no computers yet shows a card for it. After that, a banner
at the top of Home and Workspaces offers it, and tapping it opens the card
(the pair sheet in Settings lists it too). Tap Pair, compare the six digits with the computer's dialog, and choose Allow
there. The code comes from a commitment-based exchange
(`docs/ios-app-plan.md`, "Nearby pairing, exactly"), so it also vouches
for the CA fingerprint the claim then pins.

## Push relay

The app has one publisher, so Apple takes pushes for it only with Sheprd's
APNs key, and only the Sheprd push relay (`crates/sheprd-push-relay`,
`docs/apns-relay-plan.md`) holds that key. It forwards pushes, sealed so it
reads none of them. A computer's remote control daemon
(`sheprd-remote-control`) never holds an APNs key and never contacts Apple:
it pushes only through the push relay. The app turns the push relay on at
launch, after a new install or a reinstall, unless the user turned it off in
Settings > Push Relay, which also shows what the push relay sees. Computers
are ready by default too (`push_relay` in `[remote]`, which a user may turn
off per computer), so every paired computer pushes through it.

How a computer delivers this phone's pushes:

| Mode | Computer | Phone sends | What travels |
| --- | --- | --- | --- |
| Off | `push_relay = false`, or the push relay off here | No tokens; with `push_relay` off the registration is answered 503 | Nothing through APNs: no alerts or Live Activities |
| Push relay | `push_relay` on (the default) | Handles, with the push relay on here | Through push.sheprd.org. Alerts and Live Activities sealed |

Web Push is separate: only the browser app, installed and subscribed on its
own, receives it. A computer still accepts and keeps the plain tokens an app
from before the push relay sends, so that app keeps registering without
errors, but nothing is sent to them.

Settings shows each computer's mode, as it answered the last
registration (older computers do not say, or say `own_key`, a mode they no
longer have, which shows as not reported).

- **Attesting.** On first use the app asks the push relay for a challenge,
  attests a new App Attest key over the challenge's SHA-256, and gets a
  credential for 30 days and the push relay's key (`PushRelayAccess`). The
  credential is kept in the app's own keychain item, which the extensions
  cannot read, and the app attests again a week before it runs out, the
  next time it syncs. Renewal by assertion is not built yet.
- **Handles.** Each token becomes a handle made on the phone: the token,
  its kind, the credential, and a random id, sealed to the push relay's key
  (`PushRelayCredential.handle`). Computers hold handles and never see a
  token. While the push relay is on, no token is sent, and the notify key
  goes to every computer, since Live Activity content is sealed to it.
- **Live Activities.** The computer seals `attributes` and `content-state`.
  The widget extension cannot read the keychain on a locked phone, so while
  the push relay is on the app also keeps the notify keys in an App Group
  file protected until first unlock, which the widget opens them with
  (`docs/ios-spikes.md`). Attributes count only when opened by the key of
  the computer they name. Anything that does not open, and any plain
  content while the push relay is on, shows a fixed "Sheprd" activity with
  no buttons (`SealedCover`). The debug sample activity is plain, so it
  shows that cover while the push relay is on.
- **Turning it off** revokes every handle the phone made, forgets the
  credential, and removes the App Group key file. Each computer then gets
  `APNsRegistration.pushRelayOff`: an explicit `null` push-to-start and
  alert and no activities, which makes it drop the handles it held, so it
  stops pushing even when the revocation failed or was refused (a computer
  that is away drops them when the phone next reaches it). No token goes
  out instead: native notifications stop until it is on again. Web Push is
  separate and reaches only the browser app, installed and subscribed on
  its own.
- **Environments.** Xcode builds attest in App Attest's development
  environment and use `push-sandbox.sheprd.org`; TestFlight and App Store
  builds attest in production and use `push.sheprd.org`. The app tells them
  apart by `aps-environment` in its embedded provisioning profile, which
  distribution builds do not carry. Each push relay refuses the other's
  builds. The simulator has no App Attest, so the push relay cannot be used
  there.

## Live Activity detail

Detailed, the default, shows the chat and its current step. Generic shows the
agent and what it is doing in general. A computer chooses in Sheprd's Settings >
Remote control (`live_activities` in `[remote]`), and a phone chooses with Keep
Generic in its Settings. The stricter choice wins. Turning on Keep Generic ends
the activities showing, since a title set when one starts cannot change, and the
app ends any naming a chat that a computer starts before it hears. A computer that is away
hears at the next sync, and while the app is closed such an activity can show
until the app next runs. Choosing
Generic on the computer applies from the next activity, so one already showing
keeps its title until it ends.

Questions, their answer buttons, and failure messages also need `notifications =
"excerpt"`, the default. With `"generic"` a question shows Open, which answers in
the app.

A daemon older than this refuses a phone that keeps its activities generic, so
update Sheprd first. Phones on the default are unaffected.

## Sealed alerts

With the push relay on, questions, finished turns, and errors reach the
phone as APNs alerts. With it off the app gets none; Web Push reaches only
the browser app, installed and subscribed on its own. These alerts are
sealed to
the phone: Apple sees only "Sheprd", "New activity", and ciphertext.

1. **Registration.** The app asks to show alerts after the first pairing.
   It then registers its APNs token with each computer, along with the
   public half of that computer's own key-agreement key: a Secure Enclave
   key in the App Group's keychain group, or a software key in the
   simulator.
   - An alert counts as from the computer whose key opens it, and is
     dropped if its text names another computer.
   - So one paired computer cannot send an alert whose actions reach
     another.
2. **Opening.** The Notification Service Extension
   (`SheprdRemoteNotifications`) opens each alert. It shows the chat's
   title and text, groups the alert with the chat's other notifications,
   and adds the actions.
3. **Actions.**
   - **Questions:** up to four answers. "Always" answers ask for Face ID
     first.
   - **Finished turns:** Reply, whose text becomes the next prompt. Reply
     asks for Face ID or the passcode on a locked phone.
   - **Where they run:** an action runs in the app without opening it,
     and signs in with the device key.
4. **Clears and badges.** An answer given elsewhere sends a background
   push that removes the chat's notifications. Badges show the questions
   waiting.

5. **Test push.** Settings > Notifications > Send Test Push from a computer
   asks it (`POST /api/apns/test`) for one sealed alert carrying a random
   id. The extension records the id in the App Group as it opens the alert
   (`PushTestReceipts`), and Settings shows "Arrived in N s", or where it
   stopped: the push relay off on the computer, no alert registration from
   this phone yet, what the push relay refused, or nothing within 30
   seconds. A tap on it only opens
   the app.
6. **Test push from this iPhone.** With the push relay on, Settings >
   Notifications > Send Test Push from This iPhone needs no computer online.
   The phone makes (or reuses) a push relay handle for its own alert token
   and posts a test alert to `POST /v1/push` on this build's push relay
   (`push-sandbox.sheprd.org` for Xcode builds, `push.sheprd.org`
   otherwise), shaped like a computer's test push: the same cover,
   `mutable-content`, and `sealed` field under the same seal.
   - Its thread is always `test` and its collapse id `test-ID`. A
     computer's carry the pairing's scope (`scope/test`), from the notify
     key the phone gave that computer, so two computers' pushes never
     group or replace each other. A self test has no computer and so no
     scope, and a fixed thread is all it needs. Once the extension opens an
     alert it sets the thread itself: a computer's test goes in that
     computer's thread, and a self test in `test`. The Swift tests compare
     the self test with the daemon's test push on everything except these
     two ids.
   - It is sealed to a key of the phone's own (`NotifyKey.own`), kept in
     its own keychain item and never given to a computer, and names
     `this-phone` as its computer, which is no fingerprint.
   - The extension tries the computers' keys first. Only an alert none of
     them opens is tried with the phone's own key, and it shows only if it
     is exactly a self test (`SelfTestPush.open`): a test id, no badge,
     permission, wizz, or clear. It records the id and shows the text with
     no computer, chat, or actions.
   - Settings then waits for the receipt as above. A push relay answer
     other than `delivered` says where it stopped: an expired credential or
     a refused handle (the next try attests again), a stale token (`gone`),
     `limited`, or `unavailable`.
   - With the push relay off, the row says the phone cannot push itself
     without it, as no computer can either.
   - A phone that never paired was never asked to show alerts, so it has
     no alert token: the test asks then, as pairing does, and waits up to
     10 seconds for iOS to hand the token over.

A Live Activity in the question phase also offers up to three answers
(never the "always" ones) as buttons that answer from the lock screen.
Any computer can start an activity that names another computer's chat.
So a button answers only after the app reads that chat and finds the
same question still waiting, with the same answer offered.

The simulator delivers `simctl push` payloads but does not run the
extension. For the simulator loop:
- `-provisionalAlerts YES` grants quiet permission without the prompt.
- `-exportNotifyKey FINGERPRINT` writes that computer's alert key to the
  App Group, where a script can seal a payload to it.
- `-dumpNotifications YES` writes what was delivered, and opens each
  alert in the app as the extension would.

### Notification filtering

Each sealed alert carries the daemon's `event` id for the decision it
copies. The extension keeps the ids it showed (`SeenAlerts`, App Group) and
orders copies it handles at once (`AlertGate`), so a copy that arrives
again, from a retry, a failover, or a second sender, shows once:

- Without Apple's notification filtering entitlement, which is the case
  today, a copy is silent. While the first still shows, the copy takes its
  place: they share the APNs collapse id, which iOS uses as the
  notification's identifier. When the person already dismissed or opened
  the first, the copy is noted (`RetractedAlerts`) and taken back
  (`AlertSweep`): by the extension a moment after showing it, or at its
  next run, and by the app at launch, whenever it becomes active (any
  window), and on a background push. The list is a file in the App Group,
  changed only under `NSFileCoordinator`, so the app and the extension
  never overwrite each other. A sweep reads what is shown, checks, and
  removes while holding it, and the extension forgets a delivered copy's
  identifier under the same hold before showing another alert there, so a
  newer alert for the same chat, under the same identifier, is never taken
  back. A copy counts as replaced only once the extension marked it
  delivered: until then another event under its identifier proves nothing,
  as the copy may still land on top of it.
- With the entitlement, the copy is dropped. The account holder requests
  `com.apple.developer.usernotifications.filtering` for
  `so.pen.herdr.remote.notifications` at
  https://developer.apple.com/contact/request/notification-service . Once
  Apple grants it, add the entitlement to the extension in `project.yml`
  and set `SHEPRD_FILTERS_REPEATS` to `YES` there, in one change: a
  provisioning profile without it fails signing, and the flag without it
  would drop nothing.

## Around the system

- **Widgets:**
  - Needs You: small, medium (the chats as links), and the lock screen
    sizes. It is drawn from `summary.json` in the App Group, which the app
    rewrites as homes change.
  - The file is readable after the first unlock, so lock screen widgets
    draw. The full cache stays readable only while unlocked.
- **Controls:** Sheprd Chats and New Sheprd Workspace, for Control Center,
  the Lock Screen, and the Action button.
- **Siri and Shortcuts:**
  - What needs me, Ask a chat, Stop a chat, and Open the chats.
  - Asking and stopping need an unlocked phone.
  - Chats are `ChatEntity`s that Spotlight indexes. Tapping one opens the
    chat.
- **Share sheet:** "Sheprd" sends text, a link, or up to four pictures to
  a chat as a prompt. It signs in like the app does, because the paired
  computers' keychain item lives in the App Group's keychain group. An
  item from before moves there on first launch.
- **Face ID:**
  - Optional: Settings > Privacy locks the app and hides it in the app
    switcher.
  - "Always" answers ask either way.

## iPad and Apple Watch

- **iPad:**
  - The attention list sits beside the chat, with the workspace's terminal
    in an inspector (⌘⌥I).
  - A chat or terminal opens in its own window from its context menu.
  - Keyboard: ⌘N, ⌘⇧B, ⌘, and ⌘⌥↑ / ⌘⌥↓.
- **Apple Watch:**
  - Shows what needs you and what is running, and answers with a tap or a
    dictated reply.
  - It holds no keys. Every request goes to the phone over
    WatchConnectivity, and the phone checks it against the chats it last
    saw before sending it with its device key.
  - "Always" answers stay on the phone.
  - With the Face ID lock on, the phone refuses Watch requests, as it
    refuses Siri's.
  - For the simulator: `-sample YES` shows made-up chats, and
    `-openFirst YES` opens the first.

## What the daemon sends

Live Activity payloads follow `ChatActivityState` (`SheprdRemoteCore`) and
`crates/sheprd-remote-control/src/apns/payload.rs`; both sides test the
fixtures under `SheprdRemoteCore/Tests/SheprdRemoteCoreTests/Fixtures`.

- **Start:** a turn starting sends `start` through the push-to-start token.
- **Update:** tool calls, plan steps, and questions send `update`. Questions
  go at priority 10, with an alert.
- **End:** the turn's end sends `end`, which stays on the lock screen for
  fifteen minutes. A cancelled turn leaves at once.
