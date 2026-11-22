### The pairing dialog's fleet switch: admit the phone approved here to
### this computer's fleet when it asks to join.

remote-fleet-join = Also add to your fleet ({ $computers ->
    [one] one computer
   *[other] { $computers } computers
}, { $hosts ->
    [one] one host
   *[other] { $hosts } hosts
})
remote-fleet-join-note = The phone may then reach every Sheprd host in the fleet, and approve computers joining it. Remove it any time in Settings > Fleet.

## The same dialog for another computer's Sheprd, which asked to join this
## computer's fleet or invited this computer into its own. $device is the
## name that computer gives itself.

remote-fleet-add-computer = Also add { $device } to your fleet
remote-fleet-add-computer-note = It will open terminals on your fleet's hosts, and your other computers will open terminals on it. Remove it any time in Settings > Fleet.
remote-fleet-join-computer = Join { $device }'s fleet
remote-fleet-join-computer-note = This computer will open terminals on that fleet's hosts, and its computers will open terminals here. Leave any time in Settings > Fleet.
remote-fleet-same-computer = { $device } is already in your fleet.
remote-fleet-other-computer = This computer is in another fleet, so Allow pairs the two only. To join { $device }'s, leave yours in Settings > Fleet first.
