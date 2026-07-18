# Termi architecture

## Process and UI ownership

Slint owns the main thread and event loop. `src/app.rs` keeps the tab controller
inside an `Rc<RefCell<_>>`, installs generated UI callbacks, calculates grid
dimensions, and only replaces the rendered cell model when a session marks
itself dirty.

Closing a tab removes that session and explicitly terminates its child process.
Closing the last tab quits the event loop; Termi does not silently create a
replacement shell. Tab creation is transactional and capped at 32 sessions.

Termi registers `ai.clairos.termi` as its XDG application ID before creating
the window, so Wayland app IDs, X11 `WM_CLASS`, and the desktop file agree.

## Session workers

Each `TerminalSession` owns:

- one PTY master and cloned child killer;
- a `vt100` parser and visible/scrollback screen;
- a named PTY reader thread;
- a named, bounded PTY writer thread;
- a child-wait thread;
- title, displayed directory, local initial directory, size, selection, search,
  and mouse state;
- atomic dirty, exit, bell, and writer-status flags.

The UI never writes directly to the PTY. Input is enqueued without blocking;
the writer thread owns the writer and flushes messages in order. Queue capacity
is bounded both by message count and pending bytes. Terminal-generated replies
use the same ordered channel. The writer owns the receiver and shared queue/error
state but deliberately does not retain a sender clone, so dropping a session
disconnects the channel and lets the worker exit.

The reader applies an incremental OSC length guard before bytes reach the
parser. Once an OSC exceeds the cap, its remaining bytes—including embedded
escape sequences—stay discarded through the real string terminator. The reader
then processes parser state under a mutex and marks the session dirty. The UI
timer snapshots only dirty sessions.

## Terminal protocol support

The parser provides ANSI styling, 16/256/true color, Unicode cells, scrollback,
alternate screen, application cursor mode, bracketed paste, and xterm mouse
modes. Termi adds replies for primary/secondary device attributes, operating
status, cursor position, and text-area size queries.

Mouse events use the active X10/VT200/button-motion/any-motion mode and default,
UTF-8, or SGR encoding. Wheel and touchpad gestures always scroll local history;
other pointer input remains available to applications that capture the mouse.

OSC title and directory values are bounded and normalized as display metadata.
OSC 7 never selects a local directory for a new process. New tabs resolve the
active shell's actual directory from `/proc/<pid>/cwd`, with local fallbacks.

## Search and rendering

Scrollback search runs on one coalescing worker, so rapid edits do not create an
unbounded thread set. Queued work holds weak session references, closing a tab
explicitly terminates its child, and results carry generation and session
identifiers so stale UI results are ignored. A selected result scrolls into
view and contributes a highlight range to the next snapshot.

`snapshot()` walks visible cells and emits only cells that contain text, a
non-default background, cursor, selection, or search highlight. Rust converts
terminal colors to Slint colors; Slint positions each emitted cell with explicit
configured dimensions.

This cell-item renderer favors predictable behavior over peak throughput. A
future renderer can batch glyphs behind the same snapshot boundary without
changing PTY or controller ownership.

## Lock ordering

- Parser state is acquired before search highlight state.
- Selection values are copied while the parser is already held for snapshots.
- PTY resize acquires the master before parser state.
- The Slint controller never crosses worker threads.
- Worker results cross into the UI through channels and are polled by the UI
  timer.

Keep this ordering when extending the engine.

## Configuration and release boundary

Configuration version 1 is strict, validated before session creation, bounded
to 64 KiB, and atomically created with user-only permissions. Runtime geometry
is capped as well as initial geometry.

The supported release target is Ubuntu 24.04 x86_64. GitHub Actions validates
with Rust 1.92, produces Debian, AppImage, and tarball artifacts, checks
extracted binaries and desktop metadata, generates dependency license texts,
hashes artifacts, and creates build provenance attestations.

## Deferred capabilities

Split panes, workspace persistence, clickable hyperlinks, image protocols,
ligatures, ARM packages, Flatpak, and automatic updates are intentionally not
part of the 1.0 contract.
