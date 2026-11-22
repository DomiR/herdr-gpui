# Native SSH setup

Native macOS captures of PR #1480's Add Device views, using the production
GPUI renderer and an isolated development fixture based on `858251b2c`.

- `confirmation-light.png`: installation confirmation inside Add Device,
  with Cancel, No, and Yes controls; Catppuccin Latte, 1100 by 760 points.
- `masked-response-dark.png`: SSH authentication response with masked input,
  Cancel, and Send response; Catppuccin Mocha, 760 by 760 points.

The host, progress output, and entered response are synthetic. The fixture
does not contact an SSH server, install a binary, read saved credentials, or
capture personal terminal content. These are actual native views, not HTML
mockups or generated images, but do not prove end-to-end SSH provisioning.

Captured from `Window::render_to_image` in a daemon-free `--sidebar-test`
build with `integration-test,mockup`, in a temporary home directory. The
capture-only scaffolding is not included in the application changes.
