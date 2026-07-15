use std::{
    io::{Read, Write},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use anyhow::{Context, Result, anyhow};
use parking_lot::{Mutex, RwLock};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use slint::Color;
use tracing::{debug, error, warn};
use vt100::{Callbacks, Parser, Screen};

use super::palette;

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
}

impl Callbacks for CallbackState {
    fn audible_bell(&mut self, _screen: &mut Screen) {
        self.bell.store(true, Ordering::Release);
    }

    fn visual_bell(&mut self, _screen: &mut Screen) {
        self.bell.store(true, Ordering::Release);
    }

    fn set_window_title(&mut self, _screen: &mut Screen, title: &[u8]) {
        if let Ok(title) = std::str::from_utf8(title) {
            let title = title.trim();
            if !title.is_empty() {
                *self.title.write() = title.to_owned();
            }
        }
    }

    fn unhandled_osc(&mut self, _screen: &mut Screen, params: &[&[u8]]) {
        if params.len() < 2 || params[0] != b"7" {
            return;
        }

        if let Ok(uri) = std::str::from_utf8(params[1]) {
            if let Some(path) = file_uri_path(uri) {
                *self.cwd.write() = path;
            }
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
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    title: Arc<RwLock<String>>,
    cwd: Arc<RwLock<String>>,
    size: RwLock<(u16, u16)>,
    dirty: Arc<AtomicBool>,
    exited: Arc<AtomicBool>,
    bell: Arc<AtomicBool>,
    selection: Mutex<Option<Selection>>,
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

        let title = Arc::new(RwLock::new(default_title(shell, cwd)));
        let cwd_text = Arc::new(RwLock::new(cwd.display().to_string()));
        let dirty = Arc::new(AtomicBool::new(true));
        let exited = Arc::new(AtomicBool::new(false));
        let bell = Arc::new(AtomicBool::new(false));

        let callbacks = CallbackState {
            title: Arc::clone(&title),
            cwd: Arc::clone(&cwd_text),
            bell: Arc::clone(&bell),
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
                    loop {
                        match reader.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(count) => {
                                parser.lock().process(&buffer[..count]);
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
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            killer: Mutex::new(killer),
            title,
            cwd: cwd_text,
            size: RwLock::new((rows, columns)),
            dirty,
            exited,
            bell,
            selection: Mutex::new(None),
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

    pub fn exited(&self) -> bool {
        self.exited.load(Ordering::Acquire)
    }

    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
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
        if let Some(selection) = self.selection.lock().as_mut() {
            if selection.dragging {
                selection.head = clamp_point(column, row, columns, rows);
                self.dirty.store(true, Ordering::Release);
            }
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
                if let Some(cell) = screen.cell(row, column) {
                    if !cell.is_wide_continuation() {
                        line.push_str(cell.contents());
                    }
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
        let mut writer = self.writer.lock();
        writer.write_all(bytes).context("failed writing to PTY")?;
        writer.flush().context("failed flushing PTY")?;
        Ok(())
    }

    pub fn paste(&self, text: &str) -> Result<()> {
        let bracketed = self.parser.lock().screen().bracketed_paste();
        if bracketed {
            let mut payload = Vec::with_capacity(text.len() + 12);
            payload.extend_from_slice(b"\x1b[200~");
            payload.extend_from_slice(text.as_bytes());
            payload.extend_from_slice(b"\x1b[201~");
            self.write(&payload)
        } else {
            self.write(text.as_bytes())
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
                let has_background = !matches!(cell.bgcolor(), vt100::Color::Default);
                if !cell.has_contents() && !has_background && !cursor && !selected {
                    continue;
                }

                let (foreground, background) = if cell.inverse() {
                    (
                        palette::opaque_background(cell.bgcolor()),
                        palette::opaque_background(cell.fgcolor()),
                    )
                } else {
                    (
                        palette::foreground(cell.fgcolor(), cell.bold(), cell.dim()),
                        palette::background(cell.bgcolor()),
                    )
                };

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
        if !self.exited.load(Ordering::Acquire) {
            if let Err(error) = self.killer.get_mut().kill() {
                debug!(%error, "terminal child was already gone during cleanup");
            }
        }
    }
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

fn file_uri_path(uri: &str) -> Option<String> {
    let remainder = uri.strip_prefix("file://")?;
    let slash = remainder.find('/')?;
    let path = &remainder[slash..];
    Some(percent_decode(path))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                output.push((high << 4) | low);
                index += 3;
                continue;
            }
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
