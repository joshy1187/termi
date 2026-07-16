# Changelog

All notable changes to Termi are documented here. The project follows
[Semantic Versioning](https://semver.org/) once public release tags begin.

## Unreleased

### Added

- A terminal right-click Copy/Paste menu that reuses the guarded clipboard
  paste flow.
- Native Rust and Slint terminal window for Ubuntu 24.04 x86_64.
- Independent PTY-backed tabs with lifecycle cleanup and a 32-tab limit.
- ANSI, 16/256/true-color, Unicode, wide-cell, alternate-screen, bracketed
  paste, application-cursor, status-query, and xterm mouse-mode handling.
- Configurable scrollback, scrollback search, text selection, clipboard copy
  and guarded paste, OSC title/directory metadata, and shell integration.
- Strict, versioned, atomically created user configuration.
- Debian, AppImage, and tarball release packaging with checksums, license
  notices, and GitHub build-provenance attestations.
- A repository-owned scalable desktop icon and AppStream application metadata.
- CI for the Rust 1.92 minimum toolchain, dependency advisories, dependency
  sources, desktop metadata, release builds, and non-UI smoke tests.
- Public-project documentation, contribution guidance, issue forms, security
  policy, dependency updates, and release/testing runbooks.

### Changed

- Stopped modifier-key state events from being written into terminal input,
  fixing password and other text containing uppercase characters or symbols.
- Left-button drags now always select and highlight terminal text, including
  when a full-screen application has enabled mouse reporting.
- Consolidated the bundled galactic background into one tracked PNG.
- Matched the Wayland application ID, X11 class, and desktop launcher ID.
- Updated the local installer to replace the legacy Termi launcher and icon,
  and to migrate an existing XDG Desktop shortcut in place, using the stable
  `ai.clairos.termi` desktop identity.
- Moved PTY writes to a bounded worker queue so UI callbacks never block on a
  slow terminal consumer.

### Security

- Limited configuration, grid dimensions, tab count, paste size, queued PTY
  input, metadata strings, and OSC payloads.
- Added confirmation for multiline or sanitized paste input.
- Prevented the PTY writer from retaining its own channel sender after session
  shutdown.
- Kept oversized OSC payloads discarded through their real terminator, even
  when they contain embedded escape sequences.
- Percent-encoded unsafe shell-integration path bytes and sanitized generated
  titles before emitting OSC metadata.
