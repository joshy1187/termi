# Testing Termi

Termi combines pure Rust logic, PTY/process behavior, Slint compilation,
desktop integration, and backend-specific interaction. A release candidate is
not fully verified by unit tests alone.

## Automated checks

Run the repository gate from the root:

```bash
./scripts/check.sh
```

It validates shell syntax and shell-integration escaping, desktop and AppStream
metadata when their validators are installed, Rust formatting, compilation,
tests, and Clippy with warnings denied. `rust-toolchain.toml` keeps this on the
declared Rust 1.92.0 minimum.

Check the dependency graph separately:

```bash
cargo install --locked cargo-deny
cargo deny --locked check advisories sources
```

The exact exceptions in `deny.toml` are limited to build-time Wayland XML
generation against locked protocol files. Review every exception when Slint or
Wayland dependencies change.

Exercise non-UI commands in an isolated configuration directory:

```bash
cargo build --release --locked --target x86_64-unknown-linux-gnu
./target/x86_64-unknown-linux-gnu/release/termi --version
./target/x86_64-unknown-linux-gnu/release/termi --print-config-path
XDG_CONFIG_HOME=$(mktemp -d) \
  ./target/x86_64-unknown-linux-gnu/release/termi --check-config
```

Build and inspect release artifacts:

```bash
cargo about generate --locked --fail \
  --output-file THIRD_PARTY_NOTICES.html about.hbs
./packaging/build-packages.sh 1.0.0
./scripts/verify-packages.sh
```

Use the version reported by `cargo metadata` instead of `1.0.0` after a version
bump.

## Manual desktop matrix

Perform the following on Ubuntu 24.04 x86_64 for both a native Wayland session
and X11. For an explicit X11 run:

```bash
WINIT_UNIX_BACKEND=x11 cargo run --locked
```

### Startup and window behavior

- Launch from a terminal and from the installed desktop entry.
- Confirm the app ID/class groups correctly in the desktop shell.
- Drag, resize, maximize, restore, minimize, and close the frameless window.
- After every window drag, immediately exercise minimize, maximize/restore,
  close, tab activation, tab close, and new-tab controls; none may retain a
  stale pressed state or stop receiving clicks.
- Confirm only the inset 400-pixel region on the title bar's right side moves
  the window, double-clicking it toggles maximize, and the surrounding top and
  right resize borders still resize normally.
- Resize rapidly and confirm the reported rows/columns and full-screen
  applications follow without crashes or stale geometry.
- Confirm an invalid config fails before opening a window and reports its path.

### Shell and tab lifecycle

- Test Bash plus each configured shell claimed in release notes.
- Open, switch, cycle, and close tabs; verify a 33rd tab is rejected.
- Run output continuously in a background tab while typing in another.
- Exit a shell normally and confirm the tab shows the exited state.
- Close a tab with a child process, then close the window; confirm there are no
  remaining Termi processes or obvious child-process leaks.
- Enable `assets/shell/termi.bash`; test paths with spaces, `#`, `?`, Unicode,
  and control-byte test fixtures without allowing title/OSC injection.

### Rendering and input

- Verify ANSI 16-color, 256-color, and true-color samples.
- Verify bold, dim, italic, underline, inverse, Unicode, combining marks, emoji,
  and double-width CJK cells.
- Run at least one alternate-screen application such as `less`, `vim`, `nano`,
  `top`, or `htop` and return to the original screen.
- Test arrows, Home/End, Insert/Delete, Page Up/Down, F1-F12, modifiers,
  Ctrl+C, Ctrl+D, Ctrl+L, Alt-prefixed input, and Shift+Tab.
- Enter mixed lowercase, uppercase, numeric, and symbol text at a hidden
  `read -rs` prompt, then verify the received bytes after safely revealing the
  test value; modifier key presses must not add hidden bytes.
- Confirm application-cursor mode and terminal status queries work in a program
  that uses them.

### Scrollback, clipboard, and pointer

- Produce more history than one screen, scroll in both directions, and return
  to live output by typing.
- Search forward and backward, wrap matches, edit the query rapidly, and close
  search while output is active.
- Select forward/backward across lines and wide cells; copy selected and
  unselected screens.
- Paste a single safe line, multiple lines, control characters, and input near
  the 1 MiB limit. Confirm multiline/sanitized input requires approval.
- Test bracketed paste in an application that enables it.
- Right-click in normal output and in `vim` or another compatible full-screen
  application. Confirm a Copy/Paste menu opens at the pointer, neither action
  is sent as a terminal mouse event, and paste still requires confirmation for
  multiline or sanitized clipboard text.
- Drag with the left button in normal output and while a full-screen program
  has mouse reporting enabled. Confirm text highlights and can be copied from
  the context menu; verify middle-click paste and wheel/touchpad history scrolling
  in both states.

## Performance and soak checks

No formal frame-time threshold is claimed yet. Before a release, run a
high-volume producer in one and several tabs, grow scrollback near its configured
limit, search a large history, resize repeatedly, and leave the app running for
at least 30 minutes. Watch CPU, resident memory, responsiveness, worker/thread
count, and process cleanup. Record measurements in the release issue when they
change materially.
