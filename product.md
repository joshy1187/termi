---
id: termi
name: Termi
category: desktop-application
status: active
source_roots:
  - repository-root
systems:
  - ubuntu-24.04-x86_64
  - wayland
  - x11
updated_at: 2026-07-18
---

# Termi product specification

## Product summary

Termi is a native graphical Linux terminal emulator built in Rust with Slint.
Version 1 is intentionally scoped to Ubuntu 24.04 LTS on x86_64 desktop systems.
It provides a polished multi-tab terminal surface while keeping PTY, parser,
input, configuration, and process ownership explicit enough to harden and
extend over time.

The product is a local desktop application. It does not provide accounts,
network synchronization, telemetry, a backend service, or a remote shell. Any
network access comes from programs the user launches inside a terminal tab.

## Intended users and jobs

- Developers and operators who want a native, visually distinct terminal for
  ordinary shell work on Ubuntu.
- Users who need multiple independent shell tabs, searchable history, guarded
  clipboard paste, mouse-aware full-screen tools, and predictable local
  configuration.
- Maintainers who need a compact Rust codebase with a real PTY and testable
  boundaries rather than a visual terminal mockup.

The primary job is interactive local shell use. Termi is not yet positioned as
a drop-in replacement for every xterm-compatible terminal, a multiplexing
workspace, or a cross-platform terminal suite.

## Supported production contract

- Operating system: Ubuntu 24.04 LTS.
- CPU/package architecture: x86_64 / amd64.
- Display systems: native Wayland and X11 through Slint's winit backend.
- Minimum Rust toolchain for source builds: 1.92.0.
- Shells: any executable selected by absolute path; Bash, Zsh, and Fish are the
  expected common cases.
- Terminal identity: `TERM=xterm-256color`, `COLORTERM=truecolor`,
  `TERM_PROGRAM=Termi`, and a versioned `TERM_PROGRAM_VERSION`.
- Application/desktop ID: `ai.clairos.termi` across Wayland, X11 class metadata,
  and the desktop entry.

ARM packages, other Linux distributions, Windows, macOS, Android, iOS, Flatpak,
Snap, and containerized desktop delivery are outside the version 1 support
commitment unless a release note explicitly expands it.

## User-visible capabilities

### Window interaction

- Uses a frameless 44-pixel title bar above the existing 54-pixel tab and
  navigation toolbar; the two-row visual layout remains stable while window
  movement and application controls use separate pointer paths.
- Provides native minimize, maximize/restore, and close actions through the
  custom left-side title-bar controls. Those controls remain ordinary Slint
  hit targets and are never covered by the window drag region.
- Reserves a dedicated 400-logical-pixel window-move target on the title bar's
  right side, inset eight logical pixels from the top and right window edges.
  The inset preserves the frameless window's resize targets.
- Starts native window movement from the raw winit mouse-press event and
  consumes the initiating press before Slint can retain pointer capture. The
  native window manager owns the remainder of that move sequence, so moving a
  window leaves the title controls, tabs, tab-close buttons, new-tab button,
  directory display, and application menu immediately usable.
- Double-clicking the dedicated move target toggles maximized state. Presses
  outside that target propagate normally and cannot begin a window move.
- Keeps tab activation, tab closing, and new-tab creation in the separate
  toolbar below the title bar, outside the draggable area.
- Tracks only the latest logical cursor position and a bounded double-click
  timestamp/coordinate pair. Window-drag handling is event-driven and adds no
  polling timer, worker, or per-frame rendering work.

### Sessions and tabs

- Starts one configured shell in a real pseudo-terminal.
- Supports up to 32 concurrent tabs, each with its own shell process, PTY,
  parser state, scrollback, selection, search state, title, current-directory
  metadata, and worker lifecycle.
- Opens a new tab in the active shell process's actual local `/proc/<pid>/cwd`
  when available. Remote OSC 7 metadata is display-only and cannot select a
  local process directory.
- Marks exited tabs and terminates a live child when a tab or the application
  closes.
- Supports next/previous tab cycling and close-active-tab shortcuts.

### Terminal protocol and rendering

- Parses ANSI styling, the standard 16 colors, 256-color indexes, and 24-bit
  RGB colors.
- Renders Unicode text, wide cells, bold, dim, italic, underline, inverse
  colors, cursor state, selection, and the active search result.
- Supports normal and alternate screens, configurable scrollback, application
  cursor keys, bracketed paste, and the mouse modes exposed by the `vt100`
  parser.
- Replies to primary and secondary device attributes, device status, cursor
  position, and text-area size queries used by common terminal programs.
- Exposes normal keyboard text, control characters, Alt-prefixed text, arrows,
  navigation keys, and F1-F12 with modifier encoding.
- Filters standalone Shift, Control, Alt/AltGr, Meta, and Caps Lock state
  events before the PTY write path, preserving the exact bytes of capitalized
  and symbol-containing input at hidden password prompts.
- Uses a bundled galactic PNG as an aspect-ratio cover background, with balanced
  default dimming and terminal translucency so the artwork remains visible.
- Uses a frameless Slint surface with draggable, resizable desktop chrome and
  Texti-style icon controls for native minimize, maximize, and close actions.

### History, selection, and clipboard

- Keeps up to 100,000 configured scrollback lines per tab; the default is
  10,000.
- Always scrolls local history with mouse-wheel and touchpad input, including
  when a full-screen application has mouse reporting enabled. Fine-grained
  touchpad deltas accumulate into whole history lines without three-line jumps.
- Searches history forward and backward on a coalescing background worker and
  highlights the current match.
- Selects and highlights visible terminal text with a left-button pointer drag,
  including while a full-screen application has mouse reporting enabled, and
  copies either the selection or visible screen through the desktop clipboard.
- Supports native Wayland data-control clipboard integration and X11/XWayland
  paths exposed by `arboard`; clipboard access retries when a backend becomes
  available after startup.
- Pastes one safe line directly. Multiline input or input from which controls
  were removed requires explicit confirmation. Bracketed-paste markers are
  used only when the active application requested them.
- Middle click requests paste when application mouse reporting is inactive.
  Right click opens a compact terminal-local Copy/Paste menu instead of sending
  a mouse event to the PTY; the menu uses the same guarded paste path as the
  keyboard shortcut.

### Pointer reporting

- Handles X10 press, VT200 press/release, button-motion, and any-motion modes.
- Encodes default, UTF-8, and SGR coordinate formats.
- Keeps wheel and touchpad events local to scrollback, even when an application
  captures other mouse events. Left-button drags remain local selection so text
  can always be highlighted and copied.
- Tracks pressed buttons per session and clears tracking when local selection
  takes over.

### Configuration and diagnostics

- Creates `config.toml` under the platform's XDG-compatible Termi configuration
  directory on first use.
- Uses configuration schema version 1, defaults missing version-1 fields, rejects
  unknown fields, validates ranges and shell executability, and caps files at
  64 KiB.
- Creates a new configuration atomically with user-only file permissions on
  Unix and syncs the containing directory.
- Configures shell, font family and size, cell geometry, scrollback, initial
  grid, background dimming, and terminal surface opacity. New configurations
  default to `background_dim = 0.34` and `terminal_opacity = 0.72`; existing
  configurations retain their chosen values.
- Provides `--version`, `--help`, `--print-config-path`, and `--check-config`
  commands that do not open a GUI.
- Supports filtered diagnostic logs through `RUST_LOG`.

### Desktop and release delivery

- Includes a Freedesktop desktop entry, a repository-owned scalable icon,
  validated AppStream metadata, and an optional Bash integration script.
- Includes a local user installer for the supported architecture. It installs
  under `~/.local`, refreshes the desktop/icon caches, and migrates the earlier
  `termi.desktop` launcher and unscoped `termi.png` icon when they identify
  this application. An existing Termi shortcut in the user's XDG Desktop is
  edited in place so its trusted state and desktop position survive while its
  executable, icon, and window class move to the stable application identity.
- Produces an amd64 Debian package, x86_64 AppImage, and x86_64 tarball. Each
  artifact includes or accompanies SHA-256 checksums, the MIT license,
  changelog, README, shell integration, desktop metadata, icon, and full
  generated third-party license notices.
- Builds AppImages with checksum-pinned linuxdeploy, output-plugin, and type-2
  runtime files. The binary bundles its selected non-base shared libraries and
  remains within the Ubuntu 24.04 x86_64 support contract.
- GitHub tag builds attach provenance attestations to all three release
  artifacts and publish them as both workflow artifacts and GitHub Release
  assets.

## Safety and reliability boundaries

The terminal consumes arbitrary output from local programs, so all unbounded or
cross-thread paths require deliberate limits:

- maximum tabs: 32;
- maximum visible grid: 512 columns, 256 rows, and 131,072 total cells;
- maximum scrollback: 100,000 lines per tab;
- maximum clipboard paste input: 1 MiB;
- maximum queued PTY input: 2 MiB across at most 256 queued messages;
- maximum accepted OSC payload before discard: 4 KiB;
- maximum displayed title: 256 characters;
- maximum search query: 1,024 characters;
- maximum configuration file: 64 KiB.

PTY output is parsed under a session mutex. UI input is placed on a bounded,
non-blocking channel; one writer worker owns and flushes the PTY writer in order.
The worker does not retain a sender, so dropping the session disconnects its
queue. Parser-generated replies share the same ordered, bounded path. Oversized
OSC input is terminated for the parser and discarded through the real string
terminator. Display metadata strips controls and bidirectional overrides.

Paste text is normalized to LF, control characters and bidi controls are
removed, and the user is told when sanitization occurred. The optional Bash
integration percent-encodes unsafe path bytes and prevents raw controls in its
title metadata.

The crate denies Rust `unsafe` code. Dependencies are locked, registry sources
are constrained, advisories are checked in CI, third-party licenses are
generated from the lockfile, and GitHub Actions are pinned to immutable commit
SHAs. Exact advisory exceptions must state both reachability and the upstream
constraint.

## Architecture and ownership

- `src/main.rs` owns CLI dispatch, logging initialization, configuration load,
  and handoff to the desktop application.
- `src/config.rs` owns schema defaults, validation, paths, bounded reads, and
  atomic first-write behavior.
- `src/app.rs` owns the UI-thread controller, Slint callbacks, tab collection,
  clipboard, search dispatch/results, grid calculation, and UI snapshots.
- `src/terminal/session.rs` owns each PTY, child lifecycle, parser, reader,
  writer queue, input encoding handoff, selection, search state, metadata, and
  terminal snapshot.
- `src/terminal/keymap.rs` translates Slint keyboard events into terminal byte
  sequences.
- `src/terminal/palette.rs` resolves terminal colors into the Termi palette.
- `ui/app-window.slint` owns layout, focus, shortcut routing, pointer event
  capture, modal surfaces, and cell presentation.
- `packaging/` builds release artifacts and installs a user-local build.
- `scripts/` holds the repository and package verification gates.

The detailed concurrency and lock-order contract lives in
`docs/ARCHITECTURE.md`.

## Performance design

- PTY reads use a 16 KiB buffer on a named worker thread per session.
- PTY writes never wait on the UI thread and are limited by both message count
  and total queued bytes.
- A 16 ms UI timer caps state polling near display cadence and only snapshots
  when a session reports dirty state.
- Blank cells are omitted unless they carry a background, cursor, selection,
  or search highlight, reducing the number of Slint items created.
- Search requests debounce for 45 ms and coalesce queued edits into the newest
  request so rapid typing does not spawn unbounded workers.
- Release builds use thin LTO, one codegen unit, symbol stripping, and aborting
  panics.

The current cell-per-item Slint renderer favors correctness and clear ownership
over maximum throughput. No formal frame-time, memory, startup-time, or terminal
throughput SLA is claimed yet. Large grids, maximum scrollback, and high-volume
background tabs require the soak checks in `docs/TESTING.md`; a future renderer
may batch glyphs without changing PTY/session ownership.

## Quality and release gates

A public release candidate must pass:

- Bash syntax and shell-integration injection regressions;
- desktop-file and AppStream metadata validation;
- Rustfmt, `cargo check`, all tests, and Clippy with warnings denied;
- the same checks on Rust 1.92.0;
- dependency advisory and source policy;
- optimized target build and non-UI CLI smoke tests;
- third-party notice generation;
- Debian, AppImage, and tar archive extraction, content, checksum,
  dynamic-library, desktop metadata, icon, license, and CLI verification;
- the Wayland and X11 manual matrix in `docs/TESTING.md`.

CI covers the automated gates on pushes and pull requests. Tags matching
`v<package-version>` trigger the release workflow; mismatched tags fail before
publication.

## Explicitly deferred capabilities

- Split panes and terminal multiplexing.
- Session/workspace restoration and crash recovery.
- Clickable hyperlinks and OSC 8 activation.
- Inline terminal image protocols.
- Ligature shaping tuned for terminal cell semantics.
- Settings UI and live configuration reload.
- ARM or non-Ubuntu packages, Flatpak, Snap, and automatic updates.
- Formal VT/xterm conformance suites and compatibility guarantees for every
  terminal application.
- GPU-batched glyph rendering and published performance budgets.

These are roadmap candidates, not missing promises in the version 1 contract.

## Current readiness record

As of 2026-07-15, the repository has a valid MIT license, public README,
architecture notes, contribution/security/conduct policies, issue and pull
request templates, Dependabot configuration, pinned CI/release actions,
packaging scripts, dependency notice generation, and release runbooks. The
final release-preparation rehearsal produced these results:

- Rust 1.92.0 formatting, check, 24 tests, and Clippy with denied warnings:
  passed;
- Bash syntax, shell-integration injection tests, desktop-file validation, and
  AppStream metadata validation: passed;
- dependency advisory and registry-source policy: passed with the two exact,
  build-time-only Wayland XML exceptions documented in `deny.toml`;
- third-party notice generation from the locked graph: passed;
- optimized amd64 Debian, x86_64 AppImage, and x86_64 tarball build, checksum,
  extraction, dynamic-library, desktop/AppStream, icon, CLI, license, notice,
  and changelog checks: passed;
- two consecutive package builds with the same source epoch: identical
  `SHA256SUMS`;
- user-local 1.0.0 installation: installed binary matched the verified release
  binary byte-for-byte, the legacy launcher/icon were removed, and launching
  desktop ID `ai.clairos.termi` opened the expected executable and shut down
  without a remaining Termi process;
- GitHub workflow syntax/action lint and repository Markdown lint: passed;
- X11 window smoke: launched, reported class `ai.clairos.termi`, accepted tab
  shortcuts and terminal input, closed with exit code zero, and left no Termi
  processes behind;
- publishable-file scan for common credential patterns and local absolute home
  paths: no matches.

Native Wayland interaction and the remainder of the manual application matrix
remain per-release sign-off items. This does not block publishing the source to
GitHub, but a version 1.0 release tag should wait for that target-specific manual
pass.
