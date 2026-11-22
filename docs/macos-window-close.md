# macOS window-close crash (#290)

On affected Macs, closing Settings or the main window can terminate Sheprd
with an Objective-C exception in AppKit's `_NSTouchBarFinderObservation
invalidate`. AppKit observes the view's `nextResponder` by installing a
Key-Value Observing (KVO) subclass over AccessKit's accessibility subclass.
The old adapter restored the original view class while the observer was still
registered. When AppKit tried to remove the observer, it raised
`NSRangeException`. The Settings and main-window close paths both drop the
same adapter. [Sheprd #290](https://github.com/sheprd-labs/sheprd/issues/290)
and [AccessKit #823](https://github.com/AccessKit/accesskit/issues/823) record
the symptoms and the class transitions.

[AccessKit #835](https://github.com/AccessKit/accesskit/pull/835) drops the
adapter's state while preserving any outer KVO subclass. Until AppKit removes
its observer, accessibility queries fall through to the original view methods.
An ordinary unobserved view still returns to its original class on close.

GPUI 0.3.10 requires `accesskit_macos` 0.26 and AccessKit's 0.24 tree types.
The published fix in `accesskit_macos` 0.27.2 uses AccessKit 0.25 tree types.
The workspace patches GPUI's 0.26 dependency with a backport kept on the
[`sheprd/macos-0.26.3-kvo-close`](https://github.com/sheprd-labs/accesskit/tree/sheprd/macos-0.26.3-kvo-close)
branch of Sheprd's AccessKit fork: the 0.26.3 release plus #835, with
`accesskit` and `accesskit_consumer` taken from crates.io so GPUI and the
adapter share one copy of the tree types. Drop the patch once GPUI depends on
an adapter that carries the fix.
Sheprd's separate 0.27.2 dependency still supplies its window-level
accessibility focus forwarder.

## Native regression

Run on a Mac with an active desktop:

```sh
cargo test --locked -p sheprd-gpui --features integration-test --test live_gui native_window_close -- --ignored --nocapture
```

The fixture uses the real GPUI/AppKit windows and an isolated HOME/XDG
environment. It installs a `nextResponder` observer directly, so it also runs
on Macs without Touch Bar hardware. It closes Settings with the red button,
reopens Settings and uses Cmd-W to close it with a pending theme change,
then closes an additional main window. AppKit must be able to remove each
observer after close, the theme must be saved, and closing the final main
window must exit the process successfully.

To diagnose another close crash, check macOS's
`~/Library/Logs/DiagnosticReports` for an AppKit exception. Sheprd's Rust
panic reporter does not catch Objective-C exceptions.
