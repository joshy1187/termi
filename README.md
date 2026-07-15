# Termi

Termi is a native Linux terminal workbench written in Rust with a Slint interface. This package replaces the visual prototype with a real pseudo-terminal, terminal parser, multi-tab controller, keyboard input, scrollback, selection, clipboard support, dynamic resizing, shell title/current-directory tracking, and a frameless galactic interface.

## What this package delivers

- A real Linux PTY for every tab.
- Bash, Zsh, Fish, or another configured shell.
- ANSI 16-color, 256-color, and true-color rendering.
- Unicode and wide-character cell rendering.
- Alternate screen support through the terminal parser.
- Application cursor mode and bracketed paste.
- 10,000-line configurable scrollback.
- Mouse text selection and `Ctrl+Shift+C` / `Ctrl+Shift+V`.
- Multiple independent tabs with process cleanup.
- PTY resize propagation when the Termi window changes size.
- Shell-provided tab titles and OSC 7 current-directory tracking.
- Frameless window dragging, double-click maximize, resize borders, and left-side traffic-light controls.
- User-controlled font, sizing, background dimming, and terminal translucency through TOML.
- Desktop launcher and local installation scripts.

## Important boundary

This is the first production-oriented vertical slice, not a claim of complete parity with mature terminals such as Alacritty or WezTerm. The architecture is real and usable, but these still belong in later milestones: split panes, a settings surface, searchable scrollback, hyperlink activation, image protocols, complete xterm mouse reporting, ligatures, GPU-batched glyph rendering, session persistence, and exhaustive terminal conformance testing.

The PTY/parser boundary is isolated under `src/terminal`, so the engine can later be replaced or expanded without rewriting the Slint application shell.

## Install over the current prototype

The installer preserves your existing file at:

```text
/home/josh/termi/assets/backgrounds/galactic.png
```

Extract this package somewhere other than `/home/josh/termi`, then run:

```bash
cd termi-production-foundation
chmod +x install-into-termi.sh
./install-into-termi.sh /home/josh/termi
```

A timestamped backup of the current project is created beside it before files are replaced.

Then build:

```bash
cd /home/josh/termi
cargo fmt
cargo check
cargo run
```

## Ubuntu dependencies

```bash
sudo apt update
sudo apt install -y \
  build-essential \
  pkg-config \
  cmake \
  libfontconfig1-dev \
  libfreetype6-dev \
  libxkbcommon-dev \
  libwayland-dev \
  libx11-xcb-dev \
  libxcb1-dev \
  libxcb-render0-dev \
  libxcb-shape0-dev \
  libxcb-xfixes0-dev \
  libssl-dev \
  desktop-file-utils
```

Termi requires Rust 1.85 or newer because the project uses the Rust 2024 edition.

## Configuration

On first launch, Termi writes a configuration file under the normal Linux user configuration directory. On a typical Ubuntu installation this resolves to:

```text
~/.config/termi/config.toml
```

Default configuration:

```toml
shell = "/bin/bash"
font_family = "monospace"
font_size = 14.0
cell_width = 8.6
cell_height = 18.0
scrollback_lines = 10000
initial_columns = 100
initial_rows = 32
background_dim = 0.48
terminal_opacity = 0.78
```

Restart Termi after changing the file.

For a crisper terminal grid, install a monospace font such as JetBrains Mono, Iosevka, or Cascadia Mono and set both the font family and matching cell dimensions. Cell dimensions are explicit because the current renderer lays terminal cells out deterministically rather than guessing font metrics.

## Shell integration

The optional Bash integration updates the tab title and current directory using standard OSC escape sequences:

```bash
mkdir -p ~/.config/termi
cp assets/shell/termi.bash ~/.config/termi/termi.bash
printf '\n# Termi shell integration\n[[ -f ~/.config/termi/termi.bash ]] && source ~/.config/termi/termi.bash\n' >> ~/.bashrc
```

Open a new Termi tab after enabling it.

## Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+Shift+T` | New tab |
| `Ctrl+Shift+W` | Close active tab |
| `Ctrl+Shift+C` | Copy selection; falls back to visible terminal text |
| `Ctrl+Shift+V` | Paste with bracketed-paste support |
| Mouse wheel / touchpad | Scroll terminal history |
| Drag title bar | Move window |
| Double-click title bar | Maximize or restore |

Normal control sequences such as `Ctrl+C`, `Ctrl+D`, `Ctrl+L`, arrows, Home, End, Insert, Delete, Page Up/Down, and F1–F12 are forwarded to the active PTY.

## Build and install locally

```bash
./scripts/check.sh
./packaging/install-local.sh
```

The local installer places the release binary in `~/.local/bin/termi` and the desktop entry in `~/.local/share/applications`.

## Project structure

```text
assets/backgrounds/    Galactic background and fallback
assets/shell/          Optional shell integration
packaging/             Desktop launcher and local installer
scripts/               Development checks
src/app.rs             Window/controller lifecycle and Slint callbacks
src/config.rs          Persistent TOML configuration
src/terminal/          PTY, parser, palette, keyboard translation
ui/app-window.slint    Complete application surface
```

## Debugging

```bash
RUST_LOG=termi=debug cargo run
```

Wayland is preferred on modern Ubuntu. To compare X11 behavior:

```bash
WINIT_UNIX_BACKEND=x11 cargo run
```

## Current implementation notes

- Each terminal tab owns an independent shell process and PTY.
- PTY reads occur on named worker threads; Slint remains on its UI thread.
- Shared terminal state is protected with `parking_lot` locks.
- The 16 ms UI timer only rebuilds cell models when a session marks itself dirty.
- Blank cells are omitted unless they carry a background, cursor, or selection, reducing Slint item count.
- Closing a tab terminates its child process through a cloned PTY child killer.
- The current renderer is a Slint cell renderer. A later performance milestone should move glyph batching to a custom renderer once functionality is stable.
