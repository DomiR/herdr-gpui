# Windows browser composition (#826)

## Finding

At `d4a48831`, the pinned `gpui-pre-windows = 0.3.6` creates its window
composition target in `src/directx_renderer.rs`, `DirectComposition::new`:

```rust
let comp_target = unsafe { comp_device.CreateTargetForHwnd(hwnd, true) }?;
```

[Microsoft's contract][target] says `true` places this visual tree **above
the HWND's children**. GPUI attaches its full-window swap chain to that tree.
Herdr UI builds WebView2 as a child of that same HWND, then paints an opaque
background in the page area. Consequently GPUI covers the page even when
WebView2's child windows are visible, correctly sized, and producing frames.
Changing a child's HWND z-order cannot move it above this composition layer.

This agrees with the report's live DOM/title/favicon, healthy WebView2, and
successful plain lb-wry reproduction. It is a source-level explanation,
not a Windows reproduction performed on the Linux investigation host.

Merged PR #662 fixes a different failure: building WebView2 synchronously
inside an App borrow re-enters GPUI through the message pump. Current
`browser/building.rs` already uses `build_as_child_async` outside that borrow.
Its numbered build, close/discard, hide/show, and navigation lifecycle must
stay intact. No related open fix was found in the repository search.

Upstream, [zed-industries/zed#62379][composition-pr] is open and introduces a
broader, opt-in composition tree; its predecessor [#61945][overlay-pr] is
closed. Neither API exists in this checkout's pinned snapshot. These are
related work, not evidence that #826 is fixed here.

## Upstream boundary

The narrow correction is in GPUI's Windows renderer:

```diff
-        let comp_target = unsafe { comp_device.CreateTargetForHwnd(hwnd, true) }?;
+        let comp_target = unsafe { comp_device.CreateTargetForHwnd(hwnd, false) }?;
```

That puts the swap chain below native children, matching this client's
existing Windows page/overlay policy. Renderer recreation uses the same
constructor, so it must preserve that choice too.

The target and renderer are private in the pinned dependency; neither GPUI's
window API nor wry exposes this choice. A second target cannot replace the
occupied topmost slot. The repository pins GPUI and does not vendor its
renderer, so this checkout contains the regression guard and investigation,
**not a shipped rendering fix**. A reviewed upstream change and dependency
release are still required. No dependency cache or external checkout was
modified.

GPUI also reads `GPUI_DISABLE_DIRECT_COMPOSITION=1` at platform startup.
That is a diagnostic comparison, not the proposed fix: it switches the
renderer for every window and changes the transparency path. Disabling
WebView2's GPU changes a different compositor and does not address the
parent's stacking.

## Regression and native validation

The existing Windows `native_pages` fixture previously passed after the page
existed and its title arrived. It now also attempts to acquire the parent's
topmost composition slot. Windows permits only one target per HWND per
layer, so the current GPUI target deterministically rejects the probe with
`DCOMPOSITION_ERROR_WINDOW_ALREADY_COMPOSED`. The fixture names #826 in that
failure. With GPUI below children, the probe succeeds and immediately drops
its empty target without setting a root or committing any visual changes.

Run on an active Windows desktop with WebView2 installed:

```powershell
cargo test --locked -p herdr-gpui --features integration-test --test live_gui native_pages -- --ignored --nocapture --test-threads=1
```

This uses the existing isolated, daemon-free fixture and loopback page. It
keeps queued GPUI work and the real Enter-key path from #662. **The new
assertion is expected to fail with the current pinned GPUI.** It has not
been executed on Windows during this investigation. It tests the composition
contract, not final screen pixels; it is not evidence that visual QA passed.

After the upstream correction, verify on Windows x64 and ARM64:

- [ ] Run `native_pages`; retain both the pre-fix failure and post-fix pass.
- [ ] Capture the real window showing a high-contrast local page, then a web
      page, at 100% and 125% DPI; check screen pixels as well as DOM/title.
- [ ] Open and restore tabs; resize, switch tabs/workspaces, and show pages
      in split groups. Close a tab and a window while WebView2 is building.
- [ ] Check focus/typing, zoom, menus, dialogs, toasts, and the transfer card:
      overlapping pages must still hide and return under the existing policy.
- [ ] Check opaque, transparent, acrylic, Mica, and picture backdrops, plus
      renderer/device recreation; native children must remain above GPUI.

[target]: https://learn.microsoft.com/en-us/windows/win32/api/dcomp/nf-dcomp-idcompositiondevice-createtargetforhwnd
[composition-pr]: https://github.com/zed-industries/zed/pull/62379
[overlay-pr]: https://github.com/zed-industries/zed/pull/61945
