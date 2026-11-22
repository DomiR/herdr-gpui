# Add Device > Nearby Sheprd: another computer running Sheprd within
# Bluetooth range, on another network. $computer is the name that computer
# gives itself.

devices-nearby-intro = For a computer nearby that is on another network. On it, open Remote control > Pair device; it then shows up here, and Sheprd reaches it through its relays.
devices-nearby-looking = Looking for computers showing Pair device…
devices-nearby-pair = Pair
devices-nearby-platform-unsupported = Finding computers over Bluetooth needs macOS, Linux, or Windows.
devices-nearby-bluetooth-off = Turn on Bluetooth to find computers nearby.
devices-nearby-bluetooth-allow = To find computers nearby, allow Sheprd in System Settings > Privacy & Security > Bluetooth.
devices-nearby-bluetooth-unsupported = This computer has no Bluetooth to find computers nearby with.
devices-nearby-unavailable = Bluetooth is not available in this window.
devices-nearby-connecting = Connecting to { $computer }…
# The six digits follow on their own line.
devices-nearby-compare = Check that { $computer } shows this code:
devices-nearby-claiming = Asking { $computer } through its relays…
devices-nearby-waiting = Click Allow on { $computer } if the codes match.
devices-nearby-saving = Checking Herdr on { $computer } through its relays…
devices-nearby-cancel = Cancel
# $error is the reason, in a sentence of its own.
devices-nearby-failed = Pairing stopped: { $error }
devices-nearby-lost = Lost { $computer } over Bluetooth before the codes were shown. Bring the two closer and try again.
devices-nearby-no-herdr = { $computer } was paired, but Herdr does not answer there. Open Sheprd on it, then pair again.
devices-nearby-no-answer = { $computer } was paired, but could not be reached through its relays. Check that Remote Login is on there and that its relays are listening.
devices-nearby-save-failed = { $computer } was paired, but could not be saved: { $error }

## A pairing link, for a computer on another network out of Bluetooth range.

devices-nearby-link-intro = Out of Bluetooth range? On the other computer, open Remote control > Pair device, click Copy link, and paste the link here.
devices-nearby-link-placeholder = Pairing link
devices-nearby-link-pair = Pair

## The fleet, once both screens show the code. $computers and $hosts count
## the fleet's computers and the hosts it shares.

devices-nearby-fleet-invite = Add { $computer } to your fleet? It will open terminals on your fleet's { $hosts ->
    [one] one host
   *[other] { $hosts } hosts
}, and your { $computers ->
    [one] computer
   *[other] { $computers } computers
} will open terminals on it.
devices-nearby-fleet-join = Join { $computer }'s fleet? This computer will open terminals on its { $hosts ->
    [one] one host
   *[other] { $hosts } hosts
}, and its { $computers ->
    [one] computer
   *[other] { $computers } computers
} will open terminals here.
devices-nearby-fleet-found = Neither computer is in a fleet. Create one, so this computer and { $computer } reach each other both ways and share the hosts either adds?
devices-nearby-fleet-same = { $computer } is already in your fleet.
devices-nearby-fleet-other = { $computer } is in another fleet, so the two are paired only.
devices-nearby-fleet-add = Add to fleet
devices-nearby-fleet-join-button = Join fleet
devices-nearby-fleet-create = Create a fleet
devices-nearby-fleet-pair-only = Pair only
