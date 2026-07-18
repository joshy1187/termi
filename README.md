# Termi

Termi 1.0 is a native Linux terminal emulator written in Rust with a Slint
interface. It targets Ubuntu 24.04 on x86_64 and provides a real PTY per tab,
ANSI/256-color/true-color rendering, Unicode and wide cells, alternate-screen
applications, scrollback search, clipboard integration, and xterm-style mouse
reporting.

> **Status:** 1.0 release candidate. The automated source and package gates are
> in place; the Wayland/X11 manual matrix in [docs/TESTING.md](docs/TESTING.md)
> remains the final release sign-off for each tagged build.

## Supported production target

- Ubuntu 24.04 LTS
- x86_64 (`amd64`)
- Wayland or X11 desktop session
- Bash, Zsh, Fish, or another executable configured by absolute path

ARM, Flatpak, split panes, session restoration, terminal image protocols,
ligatures, and clickable hyperlinks are outside the 1.0 support boundary.

## Install a release

GitHub Releases publish three artifacts plus `SHA256SUMS`:

- `termi_1.0.0_amd64.deb`
- `termi-1.0.0-x86_64.AppImage`
- `termi-1.0.0-x86_64-unknown-linux-gnu.tar.gz`

Verify a download, then install the Debian package:

```bash
sha256sum --ignore-missing --check SHA256SUMS
sudo apt install ./termi_1.0.0_amd64.deb
termi --version
```

The AppImage runs without a system package installation:

```bash
chmod +x termi-1.0.0-x86_64.AppImage
./termi-1.0.0-x86_64.AppImage --version
./termi-1.0.0-x86_64.AppImage
```

The tarball is portable within the supported Ubuntu baseline. Its executable
is under `bin/termi`; desktop and shell-integration files are under `share/`.

## Build from source

Termi declares Rust 1.92 as its minimum toolchain. Install the Ubuntu build
dependencies:

```bash
sudo apt update
sudo apt install -y --no-install-recommends \
  appstream build-essential cmake curl desktop-file-utils file jq \
  libfontconfig1-dev libfreetype6-dev libssl-dev libwayland-dev libx11-xcb-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxcb1-dev libxkbcommon-dev patchelf pkg-config
```

Then validate and run:

```bash
rustup toolchain install 1.92.0 --component rustfmt,clippy
rustup override set 1.92.0
./scripts/check.sh
cargo run --locked
```

Dependency policy is checked separately because `cargo-deny` is a development
tool rather than an application dependency:

```bash
cargo install --locked cargo-deny
cargo deny --locked check advisories sources
```

For a user-local installation:

```bash
./packaging/install-local.sh
```

The installer also updates an existing Termi shortcut on the XDG Desktop in
place, preserving its desktop position while replacing legacy executable,
window-class, and icon paths.

To build and verify the exact release artifacts:

```bash
cargo install --locked --version 0.9.1 --features cli cargo-about
cargo about generate --locked --fail \
  --output-file THIRD_PARTY_NOTICES.html about.hbs
./packaging/build-packages.sh 1.0.0
./scripts/verify-packages.sh
```

The first package build downloads checksum-pinned linuxdeploy, AppImage output
plugin, and x86-64 runtime files into ignored `target/` storage. Later builds
reuse them only while their SHA-256 digests still match the pinned values.

## Configuration

Termi creates `~/.config/termi/config.toml` on first launch (or the equivalent
path under `XDG_CONFIG_HOME`). Useful non-UI diagnostics are:

```bash
termi --print-config-path
termi --check-config
termi --version
```

Default configuration:

The first `shell` value follows an executable absolute `$SHELL` when available,
then falls back to `/bin/bash` or `/bin/sh`. A typical Ubuntu file is:

```toml
config_version = 1
shell = "/bin/bash"
font_family = "DejaVu Sans Mono"
font_size = 16.0
cell_width = 9.64
cell_height = 20.0
scrollback_lines = 10000
initial_columns = 100
initial_rows = 32
background_dim = 0.34
terminal_opacity = 0.72
```

Configuration is size-limited, strictly parsed, validated before the window
opens, and created atomically. Unknown keys are errors so misspellings do not
silently change behavior. Restart Termi after editing it.

## Keyboard and pointer behavior

| Input | Action |
| --- | --- |
| `Ctrl+Shift+T` | New tab (up to 32) |
| `Ctrl+Shift+W` | Close active tab; closing the final tab exits |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / previous tab |
| `Ctrl+Shift+C` | Copy the selection, or the visible screen if none |
| `Ctrl+Shift+V` | Paste; multiline or cleaned input requires confirmation |
| `Ctrl+Shift+F` | Search scrollback |
| Mouse wheel / touchpad | Always scroll local terminal history |
| Left-button drag | Select and highlight text, including when an application has mouse capture |
| Middle click | Paste when application mouse capture is off |
| Right click | Open the Copy/Paste menu; paste retains its safety confirmation when needed |

Arrows, Home/End, Insert/Delete, Page Up/Down, F1–F12, modified function
keys, normal text, Alt-prefixed text, and standard control characters are
forwarded to the active PTY. The left button is reserved for local selection
and right click never sends a mouse event or pastes directly.

## Terminal behavior and limits

- PTY writes run through a bounded background queue, keeping the UI thread
  responsive under slow consumers.
- Clipboard access supports native Wayland data-control compositors and X11,
  including XWayland fallback where available.
- Paste input is limited to 1 MiB and pending input to 2 MiB.
- OSC metadata strings are bounded before parsing.
- Device attributes, status, cursor-position, and text-area-size queries receive
  standard responses needed by common full-screen applications.
- OSC 7 is display metadata only. New tabs use the child process's local
  `/proc/<pid>/cwd`, never a remote shell-provided path.
- Scrollback is configurable up to 100,000 lines; visible grid dimensions and
  cell count are capped.

## Optional Bash integration

```bash
mkdir -p ~/.config/termi
cp assets/shell/termi.bash ~/.config/termi/termi.bash
printf '%s\n' \
  '[[ -f ~/.config/termi/termi.bash ]] && source ~/.config/termi/termi.bash' \
  >> ~/.bashrc
```

This emits standard title and current-directory metadata for display.

## Release process

Pull requests and pushes run format, check, tests, Clippy, a release build, CLI
smoke tests, desktop-file validation, dependency advisory checks, and dependency
source policy on Ubuntu 24.04 with Rust 1.92. GitHub Actions are pinned to
immutable commits and Dependabot proposes dependency and action updates. Tags
of the form `v1.0.0` must match `Cargo.toml`; the release workflow generates
full license notices, builds the `.deb`, AppImage, and tarball, verifies all
three, publishes SHA-256 checksums, and records GitHub build provenance
attestations.

The application includes Slint's required `AboutSlint` attribution. Release
artifacts include full third-party notices generated from `Cargo.lock`.

## Debugging

```bash
RUST_LOG=termi=debug cargo run --locked
WINIT_UNIX_BACKEND=x11 cargo run --locked
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for ownership, worker, and
lifecycle details.

## Project documentation

- [Product specification](product.md) — supported contract, capabilities,
  limits, performance design, and deferred scope.
- [Architecture](docs/ARCHITECTURE.md) — process ownership, workers, protocol
  support, rendering, and lock ordering.
- [Testing](docs/TESTING.md) — automated gates plus the Wayland/X11 release
  matrix.
- [Releasing](docs/RELEASING.md) — versioning, artifact, tag, and publication
  runbook.
- [Changelog](CHANGELOG.md) — user-visible and security-relevant changes.

## Contributing and security

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request and follow
[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) in project spaces. Report suspected
vulnerabilities privately as described in [SECURITY.md](SECURITY.md); do not put
security details or private terminal output in a public issue.

## License

Termi is available under the [MIT License](LICENSE). Release artifacts also
include the third-party notices generated from the locked dependency graph; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) for regeneration details.
