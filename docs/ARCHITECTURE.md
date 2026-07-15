# Termi architecture

## UI process

Slint owns the application event loop. `src/app.rs` keeps the controller on the UI thread and installs every generated callback. A repeated timer polls session dirty flags, calculates terminal rows and columns from the viewport, resizes the active PTY, and replaces the Slint cell model only when state changed.

## Session model

Each `TerminalSession` owns:

- one PTY master,
- one writer,
- one cloned child killer,
- one parser and screen,
- one reader thread,
- one child wait thread,
- terminal title/current-directory state,
- resize state,
- selection state,
- atomic dirty, exit, and bell flags.

Dropping the final session handle terminates a still-running child. Closing one tab does not affect any other tab.

## Rendering model

The terminal parser produces a screen grid. `snapshot()` walks the visible cells and emits only cells that contain text, a non-default background, the cursor, or a selection. Rust maps terminal colors to Slint colors and the UI positions each cell using explicit cell dimensions.

The current approach favors correctness and iteration speed over maximum throughput. Once behavior stabilizes, the renderer should batch glyphs into a custom Slint rendering layer rather than creating one visual subtree per emitted cell.

## Lock ordering

- Selection values are copied out before acquiring the parser lock.
- PTY resize acquires the PTY master before the parser.
- Terminal snapshots acquire the parser, then briefly copy selection state.
- UI controller state never crosses worker threads.

Keep this ordering intact when expanding the engine.

## Security boundaries

- Termi launches the user-configured shell directly; it does not invoke `sh -c` around user input.
- The project forbids unsafe Rust in its own crate.
- OSC title and current-directory data are treated as display state, not commands.
- OSC 52 clipboard requests are deliberately not implemented yet.
- The desktop launcher does not start a shell through a wrapper script.

## Next engine milestones

1. Terminal response channel for device status and clipboard query responses.
2. Full xterm mouse reporting and application mouse-mode bypass of selection.
3. Searchable scrollback with match overlays.
4. Hyperlink parsing and guarded URL launching.
5. GPU/custom glyph batching.
6. Split-pane tree and persistent workspace model.
7. Automated vttest and escape-sequence regression fixtures.
