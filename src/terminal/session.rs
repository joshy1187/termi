use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread,
};

use anyhow::{Context, Result, anyhow};
use parking_lot::{Mutex, RwLock};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use slint::Color;
use tracing::{debug, error, warn};
use vt100::{Callbacks, MouseProtocolEncoding, MouseProtocolMode, Parser, Screen};

use super::palette;

const INPUT_CHANNEL_CAPACITY: usize = 256;
const MAX_PENDING_INPUT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_PASTE_BYTES: usize = 1024 * 1024;
const MAX_OSC_BYTES: usize = 4096;
const MAX_TITLE_CHARS: usize = 256;

#[derive(Debug, Clone)]
pub struct PreparedPaste {
    pub text: String,
    pub bytes: usize,
    pub lines: usize,
    pub removed_control_characters: usize,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MousePhase {
    Press,
    Release,
    Move,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct MouseModifiers {
    pub shift: bool,
    pub alt: bool,
    pub control: bool,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub current: usize,
    pub total: usize,
}

#[derive(Clone)]
struct InputSender {
    sender: SyncSender<Vec<u8>>,
    queued_bytes: Arc<AtomicUsize>,
    last_error: Arc<RwLock<Option<String>>>,
    error_reported: Arc<AtomicBool>,
}

impl InputSender {
    fn send(&self, bytes: &[u8]) -> Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        if bytes.len() > MAX_PENDING_INPUT_BYTES {
            return Err(anyhow!(
                "terminal input is larger than the {} MiB queue limit",
                MAX_PENDING_INPUT_BYTES / (1024 * 1024)
            ));
        }
        if let Some(error) = self.last_error.read().as_ref() {
            return Err(anyhow!("PTY writer stopped: {error}"));
        }

        self.queued_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |queued| {
                queued
                    .checked_add(bytes.len())
                    .filter(|next| *next <= MAX_PENDING_INPUT_BYTES)
            })
            .map_err(|_| anyhow!("terminal input queue is full; try again"))?;

        match self.sender.try_send(bytes.to_vec()) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => {
                self.queued_bytes.fetch_sub(bytes.len(), Ordering::AcqRel);
                Err(anyhow!("terminal input queue is busy; try again"))
            }
            Err(TrySendError::Disconnected(_)) => {
                self.queued_bytes.fetch_sub(bytes.len(), Ordering::AcqRel);
                Err(anyhow!("PTY writer is no longer available"))
            }
        }
    }

    fn take_error(&self) -> Option<String> {
        if self.error_reported.swap(true, Ordering::AcqRel) {
            None
        } else {
            self.last_error.read().clone()
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct SearchHighlight {
    row: u16,
    start_column: u16,
    end_column: u16,
}

#[derive(Debug, Clone, Copy)]
struct SearchLocation {
    logical_row: usize,
    start_column: u16,
    end_column: u16,
}

#[derive(Debug, Default)]
struct SearchState {
    query: String,
    matches: Vec<SearchLocation>,
    current: usize,
    highlight: Option<SearchHighlight>,
}

#[derive(Debug, Clone)]
pub struct RenderCell {
    pub row: i32,
    pub column: i32,
    pub text: String,
    pub foreground: Color,
    pub background: Color,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub cursor: bool,
    pub selected: bool,
    pub search_match: bool,
    pub wide: bool,
}

#[derive(Debug, Clone)]
pub struct TerminalSnapshot {
    pub cells: Vec<RenderCell>,
    pub rows: u16,
    pub columns: u16,
    pub cwd: String,
    pub bell: bool,
}

#[derive(Clone)]
struct CallbackState {
    title: Arc<RwLock<String>>,
    cwd: Arc<RwLock<String>>,
    bell: Arc<AtomicBool>,
    input: InputSender,
}

impl Callbacks for CallbackState {
    fn audible_bell(&mut self, _screen: &mut Screen) {
        self.bell.store(true, Ordering::Release);
    }

    fn visual_bell(&mut self, _screen: &mut Screen) {
        self.bell.store(true, Ordering::Release);
    }

    fn set_window_title(&mut self, _screen: &mut Screen, title: &[u8]) {
        if let Some(title) = sanitize_metadata(title, MAX_TITLE_CHARS) {
            *self.title.write() = title;
        }
    }

    fn unhandled_csi(
        &mut self,
        screen: &mut Screen,
        intermediate_1: Option<u8>,
        intermediate_2: Option<u8>,
        params: &[&[u16]],
        final_character: char,
    ) {
        if intermediate_2.is_some() {
            return;
        }

        let first = csi_param(params, 0);
        let response = match (intermediate_1, final_character, first) {
            (None, 'c', 0) => Some(b"\x1b[?1;2c".to_vec()),
            (Some(b'>'), 'c', 0) => {
                Some(format!("\x1b[>0;{};0c", env!("CARGO_PKG_VERSION_MAJOR")).into_bytes())
            }
            (None, 'n', 5) => Some(b"\x1b[0n".to_vec()),
            (None, 'n', 6) | (Some(b'?'), 'n', 6) => {
                let (row, column) = screen.cursor_position();
                let private = if intermediate_1 == Some(b'?') {
                    "?"
                } else {
                    ""
                };
                Some(format!("\x1b[{private}{};{}R", row + 1, column + 1).into_bytes())
            }
            (None, 't', 18) => {
                let (rows, columns) = screen.size();
                Some(format!("\x1b[8;{rows};{columns}t").into_bytes())
            }
            _ => None,
        };

        if let Some(response) = response
            && let Err(error) = self.input.send(&response)
        {
            debug!(%error, "failed to answer terminal status request");
        }
    }

    fn unhandled_osc(&mut self, _screen: &mut Screen, params: &[&[u8]]) {
        if params.len() < 2 || params[0] != b"7" {
            return;
        }

        if params[1].len() > MAX_OSC_BYTES {
            return;
        }
        if let Ok(uri) = std::str::from_utf8(params[1])
            && let Some(path) = file_uri_display(uri)
        {
            *self.cwd.write() = path;
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Selection {
    anchor: (u16, u16),
    head: (u16, u16),
    dragging: bool,
}

pub struct TerminalSession {
    id: u64,
    parser: Arc<Mutex<Parser<CallbackState>>>,
    input: InputSender,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    title: Arc<RwLock<String>>,
    cwd: Arc<RwLock<String>>,
    initial_cwd: PathBuf,
    process_id: Option<u32>,
    size: RwLock<(u16, u16)>,
    dirty: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
    bell: Arc<AtomicBool>,
    selection: Mutex<Option<Selection>>,
    pressed_mouse_button: Mutex<Option<MouseButton>>,
    search: Mutex<SearchState>,
}

impl TerminalSession {
    pub fn spawn(
        id: u64,
        shell: &str,
        cwd: &Path,
        columns: u16,
        rows: u16,
        scrollback_lines: usize,
    ) -> Result<Arc<Self>> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols: columns,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open pseudo terminal")?;

        let mut command = CommandBuilder::new(shell);
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env("TERM_PROGRAM", "Termi");
        command.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));

        let mut child = pair
            .slave
            .spawn_command(command)
            .with_context(|| format!("failed to spawn shell {shell}"))?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone PTY reader")?;
        let writer = pair
            .master
            .take_writer()
            .context("failed to acquire PTY writer")?;
        let killer = child.clone_killer();
        let process_id = child.process_id();

        let title = Arc::new(RwLock::new(default_title(shell, cwd)));
        let cwd_text = Arc::new(RwLock::new(cwd.display().to_string()));
        let dirty = Arc::new(AtomicBool::new(true));
        let exited = Arc::new(AtomicBool::new(false));
        let bell = Arc::new(AtomicBool::new(false));

        let (input_tx, input_rx) = sync_channel::<Vec<u8>>(INPUT_CHANNEL_CAPACITY);
        let input = InputSender {
            sender: input_tx,
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            last_error: Arc::new(RwLock::new(None)),
            error_reported: Arc::new(AtomicBool::new(false)),
        };
        {
            // The writer must not retain a SyncSender clone. If it did, dropping
            // the session could never disconnect the channel and the worker would
            // remain blocked in recv() forever.
            let queued_bytes = Arc::clone(&input.queued_bytes);
            let last_error = Arc::clone(&input.last_error);
            let error_reported = Arc::clone(&input.error_reported);
            thread::Builder::new()
                .name(format!("termi-pty-writer-{id}"))
                .spawn(move || {
                    run_input_writer(writer, input_rx, queued_bytes, last_error, error_reported);
                })
                .context("failed to spawn PTY writer thread")?;
        }

        let callbacks = CallbackState {
            title: Arc::clone(&title),
            cwd: Arc::clone(&cwd_text),
            bell: Arc::clone(&bell),
            input: input.clone(),
        };
        let parser = Arc::new(Mutex::new(Parser::new_with_callbacks(
            rows,
            columns,
            scrollback_lines,
            callbacks,
        )));

        {
            let parser = Arc::clone(&parser);
            let dirty = Arc::clone(&dirty);
            let exited = Arc::clone(&exited);
            thread::Builder::new()
                .name(format!("termi-pty-reader-{id}"))
                .spawn(move || {
                    let mut buffer = [0_u8; 16 * 1024];
                    let mut filtered = Vec::with_capacity(buffer.len());
                    let mut limiter = OscLimiter::default();
                    loop {
                        match reader.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(count) => {
                                filtered.clear();
                                limiter.filter(&buffer[..count], &mut filtered);
                                parser.lock().process(&filtered);
                                dirty.store(true, Ordering::Release);
                            }
                            Err(error) => {
                                if !exited.load(Ordering::Acquire) {
                                    warn!(%error, "PTY reader stopped");
                                }
                                break;
                            }
                        }
                    }
                    dirty.store(true, Ordering::Release);
                })
                .context("failed to spawn PTY reader thread")?;
        }

        {
            let exited = Arc::clone(&exited);
            let dirty = Arc::clone(&dirty);
            thread::Builder::new()
                .name(format!("termi-child-wait-{id}"))
                .spawn(move || {
                    match child.wait() {
                        Ok(status) => debug!(?status, "terminal child exited"),
                        Err(error) => error!(%error, "failed waiting for terminal child"),
                    }
                    exited.store(true, Ordering::Release);
                    dirty.store(true, Ordering::Release);
                })
                .context("failed to spawn child wait thread")?;
        }

        Ok(Arc::new(Self {
            id,
            parser,
            input,
            master: Mutex::new(pair.master),
            killer: Mutex::new(killer),
            title,
            cwd: cwd_text,
            initial_cwd: cwd.to_path_buf(),
            process_id,
            size: RwLock::new((rows, columns)),
            dirty,
            exited,
            bell,
            selection: Mutex::new(None),
            pressed_mouse_button: Mutex::new(None),
            search: Mutex::new(SearchState::default()),
        }))
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn title(&self) -> String {
        self.title.read().clone()
    }

    pub fn cwd(&self) -> String {
        self.cwd.read().clone()
    }

    pub fn local_cwd(&self) -> PathBuf {
        (!self.exited())
            .then_some(self.process_id)
            .flatten()
            .and_then(|process_id| std::fs::read_link(format!("/proc/{process_id}/cwd")).ok())
            .filter(|path| path.is_dir())
            .unwrap_or_else(|| self.initial_cwd.clone())
    }

    pub fn exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    pub fn terminate(&self) -> Result<()> {
        if self.exited() {
            return Ok(());
        }

        match self.killer.lock().kill() {
            Ok(()) => {
                self.exited.store(true, Ordering::Release);
                self.dirty.store(true, Ordering::Release);
                Ok(())
            }
            Err(_) if self.exited() => Ok(()),
            Err(error) => Err(error).context("failed to terminate terminal child"),
        }
    }

    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    pub fn take_input_error(&self) -> Option<String> {
        self.input.take_error()
    }

    pub fn application_cursor(&self) -> bool {
        self.parser.lock().screen().application_cursor()
    }

    pub fn begin_selection(&self, column: u16, row: u16) {
        let (rows, columns) = *self.size.read();
        let point = clamp_point(column, row, columns, rows);
        *self.selection.lock() = Some(Selection {
            anchor: point,
            head: point,
            dragging: true,
        });
        self.dirty.store(true, Ordering::Release);
    }

    pub fn update_selection(&self, column: u16, row: u16) {
        let (rows, columns) = *self.size.read();
        if let Some(selection) = self.selection.lock().as_mut()
            && selection.dragging
        {
            selection.head = clamp_point(column, row, columns, rows);
            self.dirty.store(true, Ordering::Release);
        }
    }

    pub fn finish_selection(&self, column: u16, row: u16) {
        let (rows, columns) = *self.size.read();
        let mut selection_guard = self.selection.lock();

        if let Some(selection) = selection_guard.as_mut() {
            selection.head = clamp_point(column, row, columns, rows);
            selection.dragging = false;

            // Clicking focuses the terminal. Only an actual drag creates
            // a persistent text selection.
            if selection.anchor == selection.head {
                *selection_guard = None;
            }

            self.dirty.store(true, Ordering::Release);
        }
    }

    pub fn clear_selection(&self) {
        if self.selection.lock().take().is_some() {
            self.dirty.store(true, Ordering::Release);
        }
    }

    pub fn selected_text(&self) -> Option<String> {
        let selection = (*self.selection.lock())?;
        let parser = self.parser.lock();
        let screen = parser.screen();
        let (rows, columns) = screen.size();
        let (start, end) = ordered_points(selection.anchor, selection.head, columns);
        let mut output = String::new();

        for row in start.1..=end.1.min(rows.saturating_sub(1)) {
            let first_column = if row == start.1 { start.0 } else { 0 };
            let last_column = if row == end.1 {
                end.0.min(columns.saturating_sub(1))
            } else {
                columns.saturating_sub(1)
            };
            let mut line = String::new();
            for column in first_column..=last_column {
                if let Some(cell) = screen.cell(row, column)
                    && !cell.is_wide_continuation()
                {
                    line.push_str(cell.contents());
                }
            }
            output.push_str(line.trim_end_matches(' '));
            if row != end.1 {
                output.push('\n');
            }
        }

        (!output.is_empty()).then_some(output)
    }

    pub fn write(&self, bytes: &[u8]) -> Result<()> {
        if self.exited() {
            return Err(anyhow!("terminal process has exited"));
        }

        self.clear_selection();
        self.parser.lock().screen_mut().set_scrollback(0);
        self.clear_search();
        self.input.send(bytes)
    }

    pub fn paste(&self, prepared: &PreparedPaste) -> Result<()> {
        let bracketed = self.parser.lock().screen().bracketed_paste();
        if bracketed {
            let mut payload = Vec::with_capacity(prepared.text.len() + 12);
            payload.extend_from_slice(b"\x1b[200~");
            payload.extend_from_slice(prepared.text.as_bytes());
            payload.extend_from_slice(b"\x1b[201~");
            self.write(&payload)
        } else {
            self.write(prepared.text.as_bytes())
        }
    }

    pub fn resize(&self, columns: u16, rows: u16) -> Result<()> {
        let columns = columns.max(20);
        let rows = rows.max(6);
        let mut size = self.size.write();
        if *size == (rows, columns) {
            return Ok(());
        }

        self.master
            .lock()
            .resize(PtySize {
                rows,
                cols: columns,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to resize PTY")?;
        self.parser.lock().screen_mut().set_size(rows, columns);
        *size = (rows, columns);
        self.dirty.store(true, Ordering::Release);
        Ok(())
    }

    pub fn scroll(&self, lines: i32) {
        let mut parser = self.parser.lock();
        let screen = parser.screen_mut();
        let current = screen.scrollback();
        let next = if lines > 0 {
            current.saturating_add(lines as usize)
        } else {
            current.saturating_sub(lines.unsigned_abs() as usize)
        };
        screen.set_scrollback(next);
        self.dirty.store(true, Ordering::Release);
    }

    pub fn mouse_reporting(&self) -> bool {
        self.parser.lock().screen().mouse_protocol_mode() != MouseProtocolMode::None
    }

    pub fn cancel_mouse_tracking(&self) {
        *self.pressed_mouse_button.lock() = None;
    }

    pub fn mouse_event(
        &self,
        button: MouseButton,
        phase: MousePhase,
        column: u16,
        row: u16,
        modifiers: MouseModifiers,
    ) -> Result<bool> {
        let (mode, encoding) = {
            let parser = self.parser.lock();
            let screen = parser.screen();
            (
                screen.mouse_protocol_mode(),
                screen.mouse_protocol_encoding(),
            )
        };
        if mode == MouseProtocolMode::None {
            return Ok(false);
        }

        let mut pressed = self.pressed_mouse_button.lock();
        let effective_button = match phase {
            MousePhase::Press => {
                *pressed = Some(button);
                Some(button)
            }
            MousePhase::Release => {
                let effective = Some(pressed.unwrap_or(button));
                *pressed = None;
                effective
            }
            MousePhase::Move => *pressed,
        };

        let should_report = match mode {
            MouseProtocolMode::None => false,
            MouseProtocolMode::Press => phase == MousePhase::Press,
            MouseProtocolMode::PressRelease => phase != MousePhase::Move,
            MouseProtocolMode::ButtonMotion => phase != MousePhase::Move || pressed.is_some(),
            MouseProtocolMode::AnyMotion => true,
        };
        if !should_report {
            return Ok(false);
        }

        let encoded =
            encode_mouse_event(effective_button, phase, column, row, modifiers, encoding)?;
        drop(pressed);
        self.write(&encoded)?;
        Ok(true)
    }

    pub fn search(&self, query: &str, backwards: bool) -> Option<SearchResult> {
        let query = query.trim();
        if query.is_empty() {
            self.clear_search();
            return None;
        }

        let mut parser = self.parser.lock();
        let screen = parser.screen_mut();
        let mut search = self.search.lock();
        if search.query != query {
            search.query = query.to_owned();
            search.matches = collect_search_matches(screen, query);
            search.highlight = None;
            search.current = if backwards {
                search.matches.len().saturating_sub(1)
            } else {
                0
            };
        } else if !search.matches.is_empty() {
            search.current = if backwards {
                search
                    .current
                    .checked_sub(1)
                    .unwrap_or(search.matches.len() - 1)
            } else {
                (search.current + 1) % search.matches.len()
            };
        }

        let Some(location) = search.matches.get(search.current).copied() else {
            self.dirty.store(true, Ordering::Release);
            return None;
        };
        search.highlight = Some(position_search_match(screen, location));
        self.dirty.store(true, Ordering::Release);
        Some(SearchResult {
            current: search.current + 1,
            total: search.matches.len(),
        })
    }

    pub fn clear_search(&self) {
        let mut search = self.search.lock();
        if !search.query.is_empty() || search.highlight.is_some() {
            *search = SearchState::default();
            self.dirty.store(true, Ordering::Release);
        }
    }

    pub fn visible_text(&self) -> String {
        self.parser.lock().screen().contents()
    }

    pub fn snapshot(&self) -> TerminalSnapshot {
        let parser = self.parser.lock();
        let screen = parser.screen();
        let (rows, columns) = screen.size();
        let (cursor_row, cursor_column) = screen.cursor_position();
        let cursor_visible = !screen.hide_cursor() && screen.scrollback() == 0;
        let mut cells = Vec::with_capacity(usize::from(rows) * usize::from(columns) / 3);
        let selection = *self.selection.lock();
        let search_highlight = self.search.lock().highlight;

        for row in 0..rows {
            for column in 0..columns {
                let Some(cell) = screen.cell(row, column) else {
                    continue;
                };
                if cell.is_wide_continuation() {
                    continue;
                }

                let cursor = cursor_visible && row == cursor_row && column == cursor_column;
                // A simple click should only focus the terminal. Do not render
                // a selection until the pointer has actually moved to another cell.
                let selected = selection
                    .filter(|selection| selection.anchor != selection.head)
                    .map(|selection| point_in_selection((column, row), selection, columns))
                    .unwrap_or(false);
                let search_match = search_highlight
                    .map(|highlight| {
                        row == highlight.row
                            && (highlight.start_column..highlight.end_column).contains(&column)
                    })
                    .unwrap_or(false);
                let has_background = !matches!(cell.bgcolor(), vt100::Color::Default);
                if !cell.has_contents() && !has_background && !cursor && !selected && !search_match
                {
                    continue;
                }

                let (foreground, background) = render_colors(cell);

                cells.push(RenderCell {
                    row: i32::from(row),
                    column: i32::from(column),
                    text: cell.contents().to_owned(),
                    foreground,
                    background,
                    bold: cell.bold(),
                    italic: cell.italic(),
                    underline: cell.underline(),
                    cursor,
                    selected,
                    search_match,
                    wide: cell.is_wide(),
                });
            }
        }

        TerminalSnapshot {
            cells,
            rows,
            columns,
            cwd: self.cwd(),
            bell: self.bell.swap(false, Ordering::AcqRel),
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if let Err(error) = self.terminate() {
            debug!(%error, "terminal child was already gone during cleanup");
        }
    }
}

fn run_input_writer<W: Write>(
    mut writer: W,
    input_rx: Receiver<Vec<u8>>,
    queued_bytes: Arc<AtomicUsize>,
    last_error: Arc<RwLock<Option<String>>>,
    error_reported: Arc<AtomicBool>,
) {
    while let Ok(payload) = input_rx.recv() {
        let payload_len = payload.len();
        let result = writer.write_all(&payload).and_then(|()| writer.flush());
        queued_bytes.fetch_sub(payload_len, Ordering::AcqRel);
        if let Err(error) = result {
            *last_error.write() = Some(error.to_string());
            error_reported.store(false, Ordering::Release);
            break;
        }
    }
}

fn render_colors(cell: &vt100::Cell) -> (Color, Color) {
    if cell.inverse() {
        (
            palette::opaque_background(cell.bgcolor()),
            palette::foreground(cell.fgcolor(), cell.bold(), cell.dim()),
        )
    } else {
        (
            palette::foreground(cell.fgcolor(), cell.bold(), cell.dim()),
            palette::background(cell.bgcolor()),
        )
    }
}

pub fn prepare_paste(text: &str) -> Result<PreparedPaste> {
    if text.len() > MAX_PASTE_BYTES {
        return Err(anyhow!(
            "clipboard text is larger than the {} MiB paste limit",
            MAX_PASTE_BYTES / (1024 * 1024)
        ));
    }

    let mut sanitized = String::with_capacity(text.len());
    let mut removed_control_characters = 0;
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                sanitized.push('\n');
            }
            '\n' | '\t' => sanitized.push(character),
            character if character.is_control() || is_bidi_control(character) => {
                removed_control_characters += 1;
            }
            character => sanitized.push(character),
        }
    }

    let lines = if sanitized.is_empty() {
        0
    } else {
        sanitized.bytes().filter(|byte| *byte == b'\n').count() + 1
    };
    Ok(PreparedPaste {
        bytes: sanitized.len(),
        lines,
        requires_confirmation: lines > 1 || removed_control_characters > 0,
        removed_control_characters,
        text: sanitized,
    })
}

#[derive(Debug, Clone, Copy, Default)]
enum OscState {
    #[default]
    Ground,
    Escape,
    Osc(usize),
    DiscardOsc,
    DiscardEscape,
}

#[derive(Debug, Default)]
struct OscLimiter {
    state: OscState,
}

impl OscLimiter {
    fn filter(&mut self, input: &[u8], output: &mut Vec<u8>) {
        for byte in input.iter().copied() {
            match self.state {
                OscState::Ground => {
                    output.push(byte);
                    if byte == 0x1b {
                        self.state = OscState::Escape;
                    }
                }
                OscState::Escape => {
                    output.push(byte);
                    self.state = if byte == b']' {
                        OscState::Osc(0)
                    } else if byte == 0x1b {
                        OscState::Escape
                    } else {
                        OscState::Ground
                    };
                }
                OscState::Osc(length) => match byte {
                    0x07 | 0x18 | 0x1a => {
                        output.push(byte);
                        self.state = OscState::Ground;
                    }
                    0x1b => {
                        output.push(byte);
                        self.state = OscState::Escape;
                    }
                    _ if length < MAX_OSC_BYTES => {
                        output.push(byte);
                        self.state = OscState::Osc(length + 1);
                    }
                    _ => {
                        // End the parser's in-progress OSC immediately, then
                        // consume the rest of this oversized sequence.
                        output.push(0x07);
                        self.state = OscState::DiscardOsc;
                    }
                },
                OscState::DiscardOsc => match byte {
                    0x07 | 0x18 | 0x1a => self.state = OscState::Ground,
                    0x1b => self.state = OscState::DiscardEscape,
                    _ => {}
                },
                OscState::DiscardEscape => {
                    self.state = match byte {
                        b'\\' | 0x07 | 0x18 | 0x1a => OscState::Ground,
                        0x1b => OscState::DiscardEscape,
                        _ => OscState::DiscardOsc,
                    };
                }
            }
        }
    }
}

fn encode_mouse_event(
    button: Option<MouseButton>,
    phase: MousePhase,
    column: u16,
    row: u16,
    modifiers: MouseModifiers,
    encoding: MouseProtocolEncoding,
) -> Result<Vec<u8>> {
    let mut code = match button {
        Some(MouseButton::Left) => 0_u16,
        Some(MouseButton::Middle) => 1,
        Some(MouseButton::Right) => 2,
        None => 3,
    };
    if phase == MousePhase::Release && encoding != MouseProtocolEncoding::Sgr {
        code = 3;
    }
    if modifiers.shift {
        code += 4;
    }
    if modifiers.alt {
        code += 8;
    }
    if modifiers.control {
        code += 16;
    }
    if phase == MousePhase::Move {
        code += 32;
    }

    let x = column.saturating_add(1);
    let y = row.saturating_add(1);
    match encoding {
        MouseProtocolEncoding::Sgr => {
            let terminator = if phase == MousePhase::Release {
                'm'
            } else {
                'M'
            };
            Ok(format!("\x1b[<{code};{x};{y}{terminator}").into_bytes())
        }
        MouseProtocolEncoding::Default => Ok(vec![
            0x1b,
            b'[',
            b'M',
            u8::try_from(code + 32).context("mouse button code is out of range")?,
            u8::try_from(x.min(223) + 32).expect("clamped mouse x coordinate"),
            u8::try_from(y.min(223) + 32).expect("clamped mouse y coordinate"),
        ]),
        MouseProtocolEncoding::Utf8 => {
            let mut output = b"\x1b[M".to_vec();
            for value in [u32::from(code) + 32, u32::from(x) + 32, u32::from(y) + 32] {
                let character = char::from_u32(value)
                    .ok_or_else(|| anyhow!("mouse coordinate is out of range"))?;
                let mut bytes = [0_u8; 4];
                output.extend_from_slice(character.encode_utf8(&mut bytes).as_bytes());
            }
            Ok(output)
        }
    }
}

fn collect_search_matches(screen: &mut Screen, query: &str) -> Vec<SearchLocation> {
    let original_scrollback = screen.scrollback();
    screen.set_scrollback(usize::MAX);
    let maximum_scrollback = screen.scrollback();
    let (rows, columns) = screen.size();
    let lowercase_query = query.to_lowercase();
    let mut matches = Vec::new();

    for logical_row in 0..maximum_scrollback + usize::from(rows) {
        let (offset, visible_row) = if logical_row <= maximum_scrollback {
            (maximum_scrollback - logical_row, 0)
        } else {
            (
                0,
                u16::try_from(logical_row - maximum_scrollback).unwrap_or(rows),
            )
        };
        screen.set_scrollback(offset);
        let Some(line) = screen.rows(0, columns).nth(usize::from(visible_row)) else {
            continue;
        };
        let lowercase_line = line.to_lowercase();
        for (byte_offset, _) in lowercase_line.match_indices(&lowercase_query) {
            let start_column = lowercase_line[..byte_offset].chars().count();
            let match_width = lowercase_query.chars().count().max(1);
            matches.push(SearchLocation {
                logical_row,
                start_column: u16::try_from(start_column).unwrap_or(u16::MAX),
                end_column: u16::try_from(start_column.saturating_add(match_width))
                    .unwrap_or(u16::MAX),
            });
        }
    }

    screen.set_scrollback(original_scrollback);
    matches
}

fn position_search_match(screen: &mut Screen, location: SearchLocation) -> SearchHighlight {
    screen.set_scrollback(usize::MAX);
    let maximum_scrollback = screen.scrollback();
    let (rows, _) = screen.size();
    let top_row = location.logical_row.saturating_sub(usize::from(rows / 2));
    let (offset, visible_row) = if top_row <= maximum_scrollback {
        (
            maximum_scrollback - top_row,
            location.logical_row.saturating_sub(top_row),
        )
    } else {
        (0, location.logical_row.saturating_sub(maximum_scrollback))
    };
    screen.set_scrollback(offset);
    SearchHighlight {
        row: u16::try_from(visible_row).unwrap_or(rows.saturating_sub(1)),
        start_column: location.start_column,
        end_column: location.end_column,
    }
}

fn csi_param(params: &[&[u16]], index: usize) -> u16 {
    params
        .get(index)
        .and_then(|parameter| parameter.first())
        .copied()
        .unwrap_or(0)
}

fn default_title(shell: &str, cwd: &Path) -> String {
    let shell = Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("shell")
        .to_owned();
    let directory = cwd
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("~");
    format!("{shell} — {directory}")
}

fn file_uri_display(uri: &str) -> Option<String> {
    let remainder = uri.strip_prefix("file://")?;
    let slash = remainder.find('/')?;
    let host = &remainder[..slash];
    let path = &remainder[slash..];
    let path = sanitize_string(&percent_decode(path), MAX_OSC_BYTES)?;
    if host.is_empty() || host.eq_ignore_ascii_case("localhost") {
        Some(path)
    } else {
        let host = sanitize_string(host, 255)?;
        Some(format!("{host}:{path}"))
    }
}

fn sanitize_metadata(value: &[u8], max_characters: usize) -> Option<String> {
    sanitize_string(&String::from_utf8_lossy(value), max_characters)
}

fn sanitize_string(value: &str, max_characters: usize) -> Option<String> {
    let sanitized = value
        .chars()
        .filter(|character| !character.is_control() && !is_bidi_control(*character))
        .take(max_characters)
        .collect::<String>();
    let sanitized = sanitized.trim();
    (!sanitized.is_empty()).then(|| sanitized.to_owned())
}

fn is_bidi_control(character: char) -> bool {
    matches!(
        character,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2]))
        {
            output.push((high << 4) | low);
            index += 3;
            continue;
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn clamp_point(column: u16, row: u16, columns: u16, rows: u16) -> (u16, u16) {
    (
        column.min(columns.saturating_sub(1)),
        row.min(rows.saturating_sub(1)),
    )
}

fn ordered_points(first: (u16, u16), second: (u16, u16), columns: u16) -> ((u16, u16), (u16, u16)) {
    let first_offset = usize::from(first.1) * usize::from(columns) + usize::from(first.0);
    let second_offset = usize::from(second.1) * usize::from(columns) + usize::from(second.0);
    if first_offset <= second_offset {
        (first, second)
    } else {
        (second, first)
    }
}

fn point_in_selection(point: (u16, u16), selection: Selection, columns: u16) -> bool {
    let (start, end) = ordered_points(selection.anchor, selection.head, columns);
    let offset = usize::from(point.1) * usize::from(columns) + usize::from(point.0);
    let start_offset = usize::from(start.1) * usize::from(columns) + usize::from(start.0);
    let end_offset = usize::from(end.1) * usize::from(columns) + usize::from(end.0);
    (start_offset..=end_offset).contains(&offset)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicUsize, Ordering},
            mpsc::sync_channel,
        },
        thread,
        time::{Duration, Instant},
    };

    use parking_lot::RwLock;

    use super::{
        CallbackState, InputSender, MAX_OSC_BYTES, MouseButton, MouseModifiers, MousePhase,
        OscLimiter, TerminalSession, collect_search_matches, encode_mouse_event, file_uri_display,
        prepare_paste, render_colors, run_input_writer, sanitize_metadata,
    };
    use vt100::{MouseProtocolEncoding, Parser};

    #[test]
    fn paste_normalizes_lines_and_removes_terminal_controls() {
        let paste = prepare_paste("first\r\nsecond\x1b[31m\0").expect("valid paste");
        assert_eq!(paste.text, "first\nsecond[31m");
        assert_eq!(paste.lines, 2);
        assert_eq!(paste.removed_control_characters, 2);
        assert!(paste.requires_confirmation);
    }

    #[test]
    fn single_line_plain_paste_does_not_require_confirmation() {
        let paste = prepare_paste("hello, world").expect("valid paste");
        assert!(!paste.requires_confirmation);
    }

    #[test]
    fn oversized_osc_is_terminated_and_discarded() {
        let mut input = b"\x1b]2;".to_vec();
        input.extend(std::iter::repeat_n(b'x', MAX_OSC_BYTES + 100));
        input.extend_from_slice(b"\x07visible");
        let mut output = Vec::new();
        OscLimiter::default().filter(&input, &mut output);

        assert!(output.ends_with(b"visible"));
        assert!(output.len() <= MAX_OSC_BYTES + 16);
    }

    #[test]
    fn oversized_osc_discards_embedded_escape_sequences() {
        let mut input = b"\x1b]2;".to_vec();
        input.extend(std::iter::repeat_n(b'x', MAX_OSC_BYTES + 1));
        input.extend_from_slice(b"\x1b[31mhidden\x07visible");
        let mut output = Vec::new();
        OscLimiter::default().filter(&input, &mut output);

        assert!(
            !output
                .windows(b"hidden".len())
                .any(|part| part == b"hidden")
        );
        assert!(!output.windows(b"[31m".len()).any(|part| part == b"[31m"));
        assert!(output.ends_with(b"visible"));
    }

    #[test]
    fn input_writer_stops_after_the_last_sender_is_dropped() {
        let (sender, receiver) = sync_channel(1);
        let queued_bytes = Arc::new(AtomicUsize::new(4));
        let last_error = Arc::new(RwLock::new(None));
        let error_reported = Arc::new(AtomicBool::new(false));
        let writer_queued_bytes = Arc::clone(&queued_bytes);

        let worker = thread::spawn(move || {
            run_input_writer(
                Vec::new(),
                receiver,
                writer_queued_bytes,
                last_error,
                error_reported,
            );
        });
        sender.send(b"test".to_vec()).unwrap();
        drop(sender);

        worker.join().expect("input writer should stop");
        assert_eq!(queued_bytes.load(Ordering::Acquire), 0);
    }

    #[test]
    fn metadata_removes_controls_and_bidi_overrides() {
        assert_eq!(
            sanitize_metadata("safe\n\u{202e}title".as_bytes(), 256),
            Some("safetitle".to_owned())
        );
    }

    #[test]
    fn osc_seven_keeps_remote_host_visibly_distinct() {
        assert_eq!(
            file_uri_display("file://example.test/home/me/My%20Files"),
            Some("example.test:/home/me/My Files".to_owned())
        );
        assert_eq!(
            file_uri_display("file://localhost/home/me"),
            Some("/home/me".to_owned())
        );
    }

    #[test]
    fn sgr_mouse_press_is_encoded() {
        let encoded = encode_mouse_event(
            Some(MouseButton::Left),
            MousePhase::Press,
            4,
            2,
            MouseModifiers::default(),
            MouseProtocolEncoding::Sgr,
        )
        .expect("mouse event");
        assert_eq!(encoded, b"\x1b[<0;5;3M");
    }

    #[test]
    fn answers_cursor_and_device_status_queries() {
        let (sender, receiver) = sync_channel(8);
        let input = InputSender {
            sender,
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            last_error: Arc::new(RwLock::new(None)),
            error_reported: Arc::new(AtomicBool::new(false)),
        };
        let callbacks = CallbackState {
            title: Arc::new(RwLock::new(String::new())),
            cwd: Arc::new(RwLock::new(String::new())),
            bell: Arc::new(AtomicBool::new(false)),
            input,
        };
        let mut parser = Parser::new_with_callbacks(24, 80, 0, callbacks);

        parser.process(b"\x1b[5n\x1b[6n\x1b[c");
        assert_eq!(receiver.recv().unwrap(), b"\x1b[0n");
        assert_eq!(receiver.recv().unwrap(), b"\x1b[1;1R");
        assert_eq!(receiver.recv().unwrap(), b"\x1b[?1;2c");
    }

    #[test]
    fn search_counts_each_matching_row_once() {
        let mut parser = Parser::new(3, 40, 20);
        parser.process(b"alpha\r\nbeta\r\nalpha");
        let matches = collect_search_matches(parser.screen_mut(), "alpha");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn inverse_default_colors_swap_foreground_and_background() {
        let mut parser = Parser::new(1, 8, 0);
        parser.process(b"\x1b[7mX");
        let cell = parser.screen().cell(0, 0).expect("inverse cell");
        let (foreground, background) = render_colors(cell);

        assert_eq!(foreground, slint::Color::from_argb_u8(255, 8, 10, 17));
        assert_eq!(background, slint::Color::from_argb_u8(255, 223, 226, 237));
    }

    #[test]
    fn pty_input_round_trips_through_the_shell() {
        let session = TerminalSession::spawn(9_999, "/bin/sh", &std::env::temp_dir(), 80, 8, 20)
            .expect("terminal session");
        session
            .write(
                b"printf '\\124\\105\\122\\115\\111\\137\\120\\124\\131\\137\\122\\117\\125\\116\\104\\137\\124\\122\\111\\120\\012'\n",
            )
            .expect("shell input");

        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if session.visible_text().contains("TERMI_PTY_ROUND_TRIP") {
                session.terminate().expect("terminal shutdown");
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }

        panic!("shell output did not reach the parser before the timeout");
    }
}
