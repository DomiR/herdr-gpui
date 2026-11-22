# Settings > Remote control: the push relay (push.sheprd.org), the only way
# a computer sends iPhone pushes. Always "push
# relay", never bare "relay": the host relays that let computers reach each
# other are a different thing with their own settings.

settings-push-relay-title = Push through the Sheprd push relay
settings-push-relay-description = Live Activities and alerts on a paired iPhone, sent through push.sheprd.org.
settings-push-relay-note = Get Live Activities and alerts when you are away from this computer. Without the push relay, notifications in the iPhone app stop; web notifications reach only the browser app, installed and subscribed separately.

# The confirmation before it turns on. Keep it true to what the push relay
# does (docs/apns-relay-plan.md, "Opt-in").
settings-push-relay-ask = Turn on the push relay?
settings-push-relay-ask-why = Apple only accepts pushes for this app from Sheprd, so they go through a small server we run.
settings-push-relay-ask-sees-title = What the push relay sees
settings-push-relay-ask-sees = Your phone's push token while it sends a push, the time and size of each push, and the IP addresses of your computer and phone.
settings-push-relay-ask-never-title = What it never sees
settings-push-relay-ask-never = Your prompts, chats, code, file names, or terminal output. Alerts and Live Activities are encrypted so that only your phone can read them.
settings-push-relay-ask-cannot-title = What it cannot do
settings-push-relay-ask-cannot = Connect to your computer, run anything, or answer for you. Tokens are not stored or logged.
settings-push-relay-ask-apple = Apple also sees the push token, timing, and size of every push.
settings-push-relay-ask-phone = Pushes flow once the iPhone app allows the push relay too, in its Settings > Live Activities.
settings-push-relay-ask-confirm = Turn on
settings-push-relay-ask-cancel = Cancel

# How the remote control daemon sends iPhone pushes now.
settings-push-relay-delivery = Push delivery
settings-push-relay-delivery-off = Off: the push relay is off.
settings-push-relay-delivery-push-relay = Through the push relay.
settings-push-relay-delivery-other = The daemon sends pushes in a way this Sheprd does not know; restart remote control, or update Sheprd.
settings-push-relay-delivery-unknown = Shown while remote control runs.

# Each linked host's push delivery, asked over SSH. $host is the host's
# name; $mode is how its daemon sends pushes.
settings-push-relay-hosts = Linked hosts:
settings-push-relay-host = { $mode ->
    [off] { $host }: off, with the push relay off there.
    [push_relay] { $host }: through the push relay.
   *[other] { $host }: a way this Sheprd does not know.
}
settings-push-relay-host-unknown = { $host }: no remote control daemon answered there.
settings-push-relay-host-failed = { $host }: could not be asked.

# How the last push of each kind went. $ago is how long ago, such as
# "5 min ago"; $status is an HTTP status code, such as 403.
settings-push-relay-last-alert = { $outcome ->
    [delivered] Last alert delivered { $ago }.
    [gone] Last alert { $ago }: the phone's registration had expired; it registers again when the app opens.
    [limited] Last alert { $ago }: held back, too many pushes for that phone.
    [refused] Last alert { $ago }: refused ({ $status }).
    [unreachable] Last alert { $ago }: could not reach the push relay.
    [unsent] Last alert { $ago }: not sent, as it could not be sealed.
   *[other] Last alert { $ago }: an outcome this Sheprd does not know.
}
settings-push-relay-last-alert-none = No alert since the daemon started.
settings-push-relay-last-live = { $outcome ->
    [delivered] Last Live Activity push delivered { $ago }.
    [gone] Last Live Activity push { $ago }: the phone's registration had expired; it registers again when the app opens.
    [limited] Last Live Activity push { $ago }: held back, too many pushes for that phone.
    [refused] Last Live Activity push { $ago }: refused ({ $status }).
    [unreachable] Last Live Activity push { $ago }: could not reach the push relay.
    [unsent] Last Live Activity push { $ago }: not sent, as it could not be sealed.
   *[other] Last Live Activity push { $ago }: an outcome this Sheprd does not know.
}
settings-push-relay-last-live-none = No Live Activity push since the daemon started.
