### Settings > Fleet: your computers and phones, the hosts they share,
### and who may reach which.

settings-fleet-section = Fleet
settings-fleet-lead = Joins all your computers and phones, so a host added on one is reachable from every other. Remote control pairs one phone with this computer; a fleet links every device you own, and needs Remote control on.

settings-fleet-needs-remote = Fleet needs Remote control
settings-fleet-needs-remote-off = Remote control is off. Its service on this computer keeps and signs the fleet, so turn it on to create or join one.
settings-fleet-needs-remote-down = Remote control is on, but its service is not answering: { $error }
settings-fleet-needs-remote-open = Open Remote control

settings-fleet-none = Not in a fleet
settings-fleet-none-description = Create a fleet to share hosts between your computers and phones. Computers join it from this page, once this Sheprd reaches them over SSH.
settings-fleet-none-failed = Could not read the fleet: { $error }
settings-fleet-reading = Reading the fleet…
settings-fleet-create = Create a fleet
settings-fleet-failed = The fleet did not do that

settings-fleet-members = Members
settings-fleet-hosts = Hosts
settings-fleet-machines = Machines this Sheprd reaches

settings-fleet-kind-computer = Computer
settings-fleet-kind-phone = Phone
settings-fleet-admitted-by = admitted by { $member }
settings-fleet-founder = founded the fleet
settings-fleet-this-computer = this computer

settings-fleet-remove = Remove
settings-fleet-leave = Leave
settings-fleet-cancel = Cancel
settings-fleet-remove-why = Why? A compromised device takes everything it added with it.
settings-fleet-reason-lost = Lost
settings-fleet-reason-compromised = Compromised
settings-fleet-reason-retired = Retired
settings-fleet-remove-only = Removed everywhere; what it added stays.
settings-fleet-remove-with = Removed everywhere, with { $members ->
    [one] one member
   *[other] { $members } members
} and { $hosts ->
    [one] one host
   *[other] { $hosts } hosts
} it added.

settings-fleet-host-line = Added by { $member } · { $allowed ->
    [0] no computers allowed
    [one] one computer allowed
   *[other] { $allowed } computers allowed
}
settings-fleet-host-held = Two members added it with different keys: open it from a computer that can check them.
settings-fleet-host-member = Allow { $member }
settings-fleet-remove-host = Remove everywhere
settings-fleet-remove-host-ask = Every computer loses its access to this host.

settings-fleet-suspended = { $member } is cut off
settings-fleet-suspended-description = A removal of it arrived before its history did; it stays cut off until the history settles it.
settings-fleet-equivocating = { $member } signed in two places
settings-fleet-equivocating-description = Its key is in use on two devices: a cloned disk, a restored backup, or a theft. What it adds counts for nothing until you remove it.

settings-fleet-machines-none = No other machine reached
settings-fleet-machines-none-description = Machines this Sheprd connects to over SSH show here, to invite or share.
settings-fleet-machine-member = In your fleet
settings-fleet-machine-outside = Not in your fleet
settings-fleet-machine-failed = Could not be asked
settings-fleet-machine-removed = Removed from your fleet
settings-fleet-machine-unknown = Checking…
settings-fleet-machine-working = Working…
settings-fleet-invite = Invite
settings-fleet-invite-ask = Invite this computer? It will open terminals on your hosts, and your computers on it.
settings-fleet-share = Share
settings-fleet-share-ask = Share this host with the computers allowed below:
settings-fleet-machine-agent = Plain host in your fleet
settings-fleet-machine-no-daemon = No Sheprd here; it can be shared as a plain host
settings-fleet-machine-unchecked = Its SSH key is not checked, so the fleet cannot share it
settings-fleet-share-plain = Share as a plain host
settings-fleet-share-plain-ask = Installs sheprd-relay-agent on this machine, runs it as a user service on this computer's relays, and lets the computers allowed below in over SSH:

settings-fleet-removed = This computer is not in the fleet any more
settings-fleet-hosted = This computer is a shared host of the fleet
settings-fleet-hosted-description = The fleet lets the computers allowed here open terminals on it. It is not a member, so it signs nothing; a member removes it from the fleet.
settings-fleet-removed-description = A member removed it. Its keys no longer count anywhere; forget the fleet here to remove what it wrote, then join again with a new approval.
settings-fleet-forget = Forget the fleet
settings-fleet-scrub = Remove fleet access
settings-fleet-scrub-ask = Remove every way a fleet reaches this account: its authorized_keys lines and the relay agent's entries. Nothing is signed; it only takes access away.
settings-fleet-scrub-description = The last resort for a machine no member can reach: removes what any fleet wrote on it.
settings-fleet-unsaved = { $count ->
    [one] One fleet host could not be added to Devices
   *[other] { $count } fleet hosts could not be added to Devices
}

settings-fleet-joins = Waiting to join
settings-fleet-join-line = Phone · asked through { $device }
settings-fleet-join-ask = Admit this phone? It will reach every Sheprd host in the fleet, and may approve computers joining it.
settings-fleet-admit = Admit
