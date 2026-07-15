use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc, Weak,
        mpsc::{Receiver, Sender, channel},
    },
    thread,
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use slint::{
    Color, ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel,
    winit_030::WinitWindowAccessor,
};
use tracing::{debug, error, warn};

use crate::{
    AppWindow, TabData, TerminalCell,
    config::{AppConfig, MAX_COLUMNS, MAX_GRID_CELLS, MAX_ROWS},
    terminal::{
        MouseButton, MouseModifiers, MousePhase, PreparedPaste, SearchResult, TerminalSession,
        encode_key, prepare_paste,
    },
};

const MAX_TABS: usize = 32;
const XDG_APP_ID: &str = "ai.clairos.termi";

struct PendingPaste {
    session_id: u64,
    prepared: PreparedPaste,
}

struct SearchWorkerResult {
    generation: u64,
    session_id: u64,
    query_empty: bool,
    result: Option<SearchResult>,
}

struct SearchRequest {
    generation: u64,
    session: Weak<TerminalSession>,
    query: String,
    backwards: bool,
}

struct Controller {
    config: AppConfig,
    sessions: Vec<Arc<TerminalSession>>,
    active: usize,
    next_id: u64,
    clipboard: Option<arboard::Clipboard>,
    pending_paste: Option<PendingPaste>,
    search_generation: u64,
    search_request_sender: Sender<SearchRequest>,
    search_receiver: Receiver<SearchWorkerResult>,
    last_resize_error: Option<String>,
}

impl Controller {
    fn new(config: AppConfig) -> Result<Self> {
        let cwd = fallback_working_directory();
        let first = TerminalSession::spawn(
            1,
            &config.shell,
            &cwd,
            config.initial_columns,
            config.initial_rows,
            config.scrollback_lines,
        )?;

        let (search_request_sender, search_request_receiver) = channel::<SearchRequest>();
        let (search_sender, search_receiver) = channel();
        thread::Builder::new()
            .name("termi-scrollback-search".to_owned())
            .spawn(move || {
                while let Ok(mut request) = search_request_receiver.recv() {
                    thread::sleep(Duration::from_millis(45));
                    for newer_request in search_request_receiver.try_iter() {
                        request = newer_request;
                    }
                    let Some(session) = request.session.upgrade() else {
                        continue;
                    };
                    let session_id = session.id();
                    let query_empty = request.query.trim().is_empty();
                    let result = session.search(&request.query, request.backwards);
                    if search_sender
                        .send(SearchWorkerResult {
                            generation: request.generation,
                            session_id,
                            query_empty,
                            result,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .context("failed to start scrollback search worker")?;
        Ok(Self {
            config,
            sessions: vec![first],
            active: 0,
            next_id: 2,
            clipboard: arboard::Clipboard::new().ok(),
            pending_paste: None,
            search_generation: 0,
            search_request_sender,
            search_receiver,
            last_resize_error: None,
        })
    }

    fn active_session(&self) -> Option<&Arc<TerminalSession>> {
        self.sessions.get(self.active)
    }

    fn new_tab(&mut self) -> Result<()> {
        ensure!(
            self.sessions.len() < MAX_TABS,
            "Termi supports at most {MAX_TABS} open tabs"
        );
        let cwd = self
            .active_session()
            .map(|session| session.local_cwd())
            .filter(|path| path.is_dir())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(fallback_working_directory);

        let session = TerminalSession::spawn(
            self.next_id,
            &self.config.shell,
            &cwd,
            self.config.initial_columns,
            self.config.initial_rows,
            self.config.scrollback_lines,
        )?;
        if let Some(active_session) = self.active_session() {
            active_session.clear_search();
        }
        self.next_id += 1;
        self.sessions.push(session);
        self.active = self.sessions.len() - 1;
        Ok(())
    }

    fn close_tab(&mut self, index: usize) -> bool {
        if index >= self.sessions.len() {
            return false;
        }

        let session = self.sessions.remove(index);
        if let Err(error) = session.terminate() {
            debug!(%error, session_id = session.id(), "failed to terminate closed terminal tab");
        }
        if self.sessions.is_empty() {
            self.active = 0;
            self.pending_paste = None;
            return true;
        } else if self.active >= self.sessions.len() {
            self.active = self.sessions.len() - 1;
        } else if index < self.active {
            self.active -= 1;
        }
        false
    }

    fn activate_tab(&mut self, index: usize) {
        if index < self.sessions.len() {
            if let Some(session) = self.active_session() {
                session.clear_search();
            }
            self.active = index;
            if let Some(session) = self.active_session() {
                session.take_dirty();
            }
        }
    }

    fn copy_visible(&mut self) -> Result<()> {
        let Some(text) = self.active_session().map(|session| {
            session
                .selected_text()
                .unwrap_or_else(|| session.visible_text())
        }) else {
            return Ok(());
        };
        self.clipboard
            .as_mut()
            .context("the desktop clipboard is unavailable")?
            .set_text(text)
            .context("failed to copy terminal text")
    }

    fn request_paste(&mut self) -> Result<Option<String>> {
        let text = self
            .clipboard
            .as_mut()
            .context("the desktop clipboard is unavailable")?
            .get_text()
            .context("failed to read clipboard text")?;
        let prepared = prepare_paste(&text)?;
        if prepared.text.is_empty() {
            return Ok(None);
        }
        let session_id = self
            .active_session()
            .map(|session| session.id())
            .context("there is no active terminal")?;
        if prepared.requires_confirmation {
            let summary = paste_summary(&prepared);
            self.pending_paste = Some(PendingPaste {
                session_id,
                prepared,
            });
            Ok(Some(summary))
        } else {
            self.active_session()
                .context("there is no active terminal")?
                .paste(&prepared)?;
            Ok(None)
        }
    }

    fn confirm_paste(&mut self) -> Result<()> {
        let pending = self
            .pending_paste
            .take()
            .context("there is no pending paste")?;
        let session = self
            .sessions
            .iter()
            .find(|session| session.id() == pending.session_id)
            .context("the destination tab was closed")?;
        session.paste(&pending.prepared)
    }

    fn start_search(&mut self, query: String, backwards: bool) -> Result<()> {
        ensure!(
            query.chars().count() <= 1024,
            "search text is limited to 1024 characters"
        );
        self.search_generation = self.search_generation.wrapping_add(1);
        let generation = self.search_generation;
        let Some(session) = self.active_session().cloned() else {
            return Ok(());
        };
        self.search_request_sender
            .send(SearchRequest {
                generation,
                session: Arc::downgrade(&session),
                query,
                backwards,
            })
            .context("scrollback search worker stopped")
    }
}

impl Drop for Controller {
    fn drop(&mut self) {
        for session in &self.sessions {
            if let Err(error) = session.terminate() {
                debug!(%error, session_id = session.id(), "failed to terminate terminal during shutdown");
            }
        }
    }
}

pub fn run(config: AppConfig) -> Result<()> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .select()
        .context("failed to select Slint winit backend")?;
    slint::set_xdg_app_id(XDG_APP_ID).context("failed to set the desktop application ID")?;

    let app = AppWindow::new().context("failed to create Termi window")?;
    apply_config(&app, &config);

    let controller = Rc::new(RefCell::new(Controller::new(config.clone())?));
    install_callbacks(&app, Rc::clone(&controller));
    refresh_ui(&app, &controller.borrow());

    let app_weak = app.as_weak();
    let timer_controller = Rc::clone(&controller);
    let update_timer = Timer::default();
    update_timer.start(TimerMode::Repeated, Duration::from_millis(16), move || {
        let Some(app) = app_weak.upgrade() else {
            return;
        };

        let mut controller = timer_controller.borrow_mut();
        while let Ok(search_result) = controller.search_receiver.try_recv() {
            let active_id = controller.active_session().map(|session| session.id());
            if search_result.generation == controller.search_generation
                && active_id == Some(search_result.session_id)
            {
                let status = if search_result.query_empty {
                    String::new()
                } else {
                    search_result
                        .result
                        .map(|result| format!("{} of {}", result.current, result.total))
                        .unwrap_or_else(|| "No matches".to_owned())
                };
                app.set_search_status(status.into());
            }
        }
        let columns = cells_for_extent(
            app.get_terminal_viewport_width(),
            controller.config.cell_width,
            20,
            MAX_COLUMNS,
        );
        let mut rows = cells_for_extent(
            app.get_terminal_viewport_height(),
            controller.config.cell_height,
            6,
            MAX_ROWS,
        );
        let maximum_rows_for_columns = MAX_GRID_CELLS / usize::from(columns);
        rows = rows.min(u16::try_from(maximum_rows_for_columns).unwrap_or(MAX_ROWS));

        if let Some(session) = controller.active_session().cloned() {
            if let Err(error) = session.resize(columns, rows) {
                warn!(%error, "terminal resize failed");
                let message = format!("Could not resize the terminal: {error:#}");
                if controller.last_resize_error.as_ref() != Some(&message) {
                    show_error(&app, message.clone());
                    controller.last_resize_error = Some(message);
                }
            } else {
                controller.last_resize_error = None;
            }
        }

        if let Some(error) = controller
            .sessions
            .iter()
            .find_map(|session| session.take_input_error())
        {
            show_error(&app, format!("Terminal input stopped: {error}"));
        }

        let should_refresh = controller
            .sessions
            .iter()
            .any(|session| session.take_dirty());
        if should_refresh {
            refresh_ui(&app, &controller);
        }
    });

    app.run().context("Termi event loop failed")?;
    drop(update_timer);
    Ok(())
}

fn apply_config(app: &AppWindow, config: &AppConfig) {
    app.set_app_version(env!("CARGO_PKG_VERSION").into());
    app.set_terminal_font_family(config.font_family.clone().into());
    app.set_terminal_font_size(config.font_size);
    app.set_terminal_cell_width(config.cell_width);
    app.set_terminal_cell_height(config.cell_height);
    app.set_shell_name(shell_name(&config.shell).into());
    app.set_background_overlay(Color::from_argb_u8(alpha(config.background_dim), 3, 7, 14));
    app.set_terminal_surface(Color::from_argb_u8(
        alpha(config.terminal_opacity),
        10,
        11,
        16,
    ));
}

fn install_callbacks(app: &AppWindow, controller: Rc<RefCell<Controller>>) {
    let app_weak = app.as_weak();
    app.on_start_window_drag(move || {
        if let Some(app) = app_weak.upgrade() {
            app.window().with_winit_window(|window| {
                if let Err(error) = window.drag_window() {
                    debug!(%error, "window manager rejected drag request");
                }
            });
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_new_tab(move || {
        let result = callback_controller.borrow_mut().new_tab();
        if let Some(app) = app_weak.upgrade() {
            match result {
                Ok(()) => {
                    {
                        let controller = callback_controller.borrow();
                        refresh_ui(&app, &controller);
                    }
                    if app.get_search_open() {
                        close_search_ui(&app, &mut callback_controller.borrow_mut());
                    } else {
                        app.invoke_focus_terminal();
                    }
                }
                Err(error) => {
                    error!(%error, "failed to open terminal tab");
                    show_error(&app, format!("Could not open a new tab: {error:#}"));
                }
            }
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_close_tab(move |index| {
        if index < 0 {
            return;
        }

        let should_close = callback_controller.borrow_mut().close_tab(index as usize);
        if let Some(app) = app_weak.upgrade() {
            if should_close {
                close_application(&app);
            } else {
                {
                    let controller = callback_controller.borrow();
                    refresh_ui(&app, &controller);
                }
                close_search_ui(&app, &mut callback_controller.borrow_mut());
            }
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_close_active_tab(move || {
        let active = callback_controller.borrow().active;
        let should_close = callback_controller.borrow_mut().close_tab(active);
        if let Some(app) = app_weak.upgrade() {
            if should_close {
                close_application(&app);
            } else {
                {
                    refresh_ui(&app, &callback_controller.borrow());
                }
                close_search_ui(&app, &mut callback_controller.borrow_mut());
            }
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_activate_tab(move |index| {
        if index < 0 {
            return;
        }
        callback_controller
            .borrow_mut()
            .activate_tab(index as usize);
        if let Some(app) = app_weak.upgrade() {
            refresh_ui(&app, &callback_controller.borrow());
            close_search_ui(&app, &mut callback_controller.borrow_mut());
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_next_tab(move |backwards| {
        let mut controller = callback_controller.borrow_mut();
        if controller.sessions.is_empty() {
            return;
        }
        if let Some(session) = controller.active_session() {
            session.clear_search();
        }
        controller.active = if backwards {
            controller
                .active
                .checked_sub(1)
                .unwrap_or(controller.sessions.len() - 1)
        } else {
            (controller.active + 1) % controller.sessions.len()
        };
        if let Some(app) = app_weak.upgrade() {
            refresh_ui(&app, &controller);
            close_search_ui(&app, &mut controller);
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_terminal_key(move |text, control, alt, shift, _meta| {
        let controller = callback_controller.borrow();
        let Some(session) = controller.active_session() else {
            return;
        };
        session.clear_selection();
        let application_cursor = session.application_cursor();
        if let Some(bytes) = encode_key(text.as_str(), control, alt, shift, application_cursor)
            && let Err(error) = session.write(&bytes)
        {
            warn!(%error, "failed to send keyboard input to terminal");
            if let Some(app) = app_weak.upgrade() {
                show_error(&app, format!("Could not send terminal input: {error:#}"));
            }
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_terminal_scroll(move |x, y, delta, shift, alt, control| {
        let lines = if delta > 0.0 {
            3
        } else if delta < 0.0 {
            -3
        } else {
            0
        };
        if lines != 0 {
            let controller = callback_controller.borrow();
            if let Some(session) = controller.active_session() {
                if shift || !session.mouse_reporting() {
                    session.scroll(lines);
                } else {
                    let button = if delta > 0.0 {
                        MouseButton::WheelUp
                    } else {
                        MouseButton::WheelDown
                    };
                    let result = session.mouse_event(
                        button,
                        MousePhase::Press,
                        coordinate_to_cell(x, controller.config.cell_width),
                        coordinate_to_cell(y, controller.config.cell_height),
                        MouseModifiers {
                            shift,
                            alt,
                            control,
                        },
                    );
                    if let Err(error) = result {
                        warn!(%error, "failed to report terminal mouse wheel");
                        if let Some(app) = app_weak.upgrade() {
                            show_error(&app, format!("Could not send mouse input: {error:#}"));
                        }
                    }
                }
            }
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_terminal_pointer(move |x, y, button, phase, shift, alt, control| {
        let mut controller = callback_controller.borrow_mut();
        let Some(session) = controller.active_session() else {
            return;
        };
        let session = Arc::clone(session);
        let column = coordinate_to_cell(x, controller.config.cell_width);
        let row = coordinate_to_cell(y, controller.config.cell_height);
        let button = match button {
            0 => MouseButton::Left,
            1 => MouseButton::Middle,
            2 => MouseButton::Right,
            _ => return,
        };
        let phase = match phase {
            0 => MousePhase::Press,
            1 => MousePhase::Move,
            2 => MousePhase::Release,
            _ => return,
        };

        let selecting = shift || !session.mouse_reporting();
        if selecting && phase == MousePhase::Release {
            session.cancel_mouse_tracking();
        }
        if selecting && button == MouseButton::Left {
            match phase {
                MousePhase::Press => session.begin_selection(column, row),
                MousePhase::Move => session.update_selection(column, row),
                MousePhase::Release => session.finish_selection(column, row),
            }
        } else if selecting && button == MouseButton::Middle && phase == MousePhase::Press {
            handle_paste_request(&mut controller, app_weak.upgrade().as_ref());
        } else if !selecting {
            let result = session.mouse_event(
                button,
                phase,
                column,
                row,
                MouseModifiers {
                    shift,
                    alt,
                    control,
                },
            );
            if let Err(error) = result {
                warn!(%error, "failed to report terminal pointer");
                if let Some(app) = app_weak.upgrade() {
                    show_error(&app, format!("Could not send mouse input: {error:#}"));
                }
            }
        }
        if let Some(app) = app_weak.upgrade() {
            refresh_ui(&app, &controller);
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_copy_request(move || {
        if let Err(error) = callback_controller.borrow_mut().copy_visible() {
            warn!(%error, "failed to copy terminal text");
            if let Some(app) = app_weak.upgrade() {
                show_error(&app, format!("Could not copy terminal text: {error:#}"));
            }
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_paste_request(move || {
        let mut controller = callback_controller.borrow_mut();
        handle_paste_request(&mut controller, app_weak.upgrade().as_ref());
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_confirm_paste(move || {
        let result = callback_controller.borrow_mut().confirm_paste();
        if let Some(app) = app_weak.upgrade() {
            app.set_paste_confirmation_open(false);
            if let Err(error) = result {
                show_error(&app, format!("Could not paste terminal text: {error:#}"));
            }
        }
    });

    let callback_controller = Rc::clone(&controller);
    app.on_cancel_paste(move || {
        callback_controller.borrow_mut().pending_paste = None;
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_search_changed(move |query| {
        let query = query.to_string();
        if let Some(app) = app_weak.upgrade() {
            app.set_search_status(if query.trim().is_empty() {
                "".into()
            } else {
                "Searching…".into()
            });
        }
        if let Err(error) = callback_controller.borrow_mut().start_search(query, false)
            && let Some(app) = app_weak.upgrade()
        {
            show_error(&app, format!("Could not search scrollback: {error:#}"));
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_search_next(move || {
        if let Some(app) = app_weak.upgrade() {
            app.set_search_status("Searching…".into());
            if let Err(error) = callback_controller
                .borrow_mut()
                .start_search(app.get_search_query().to_string(), false)
            {
                show_error(&app, format!("Could not search scrollback: {error:#}"));
            }
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_search_previous(move || {
        if let Some(app) = app_weak.upgrade() {
            app.set_search_status("Searching…".into());
            if let Err(error) = callback_controller
                .borrow_mut()
                .start_search(app.get_search_query().to_string(), true)
            {
                show_error(&app, format!("Could not search scrollback: {error:#}"));
            }
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_close_search(move || {
        if let Some(app) = app_weak.upgrade() {
            close_search_ui(&app, &mut callback_controller.borrow_mut());
        }
    });

    app.on_dismiss_error(|| {});
    app.on_close_about(|| {});

    let app_weak = app.as_weak();
    app.on_open_menu(move || {
        if let Some(app) = app_weak.upgrade() {
            app.set_about_open(true);
        }
    });
}

fn refresh_ui(app: &AppWindow, controller: &Controller) {
    let tabs = controller
        .sessions
        .iter()
        .enumerate()
        .map(|(index, session)| TabData {
            id: session.id().try_into().unwrap_or(i32::MAX),
            title: SharedString::from(session.title()),
            active: index == controller.active,
            exited: session.exited(),
        })
        .collect::<Vec<_>>();
    app.set_tabs(model(tabs));
    app.set_active_tab_index(controller.active.try_into().unwrap_or(0));

    let Some(session) = controller.active_session() else {
        return;
    };
    let snapshot = session.snapshot();
    let cells = snapshot
        .cells
        .into_iter()
        .map(|cell| TerminalCell {
            row: cell.row,
            column: cell.column,
            text: cell.text.into(),
            foreground: cell.foreground,
            background: cell.background,
            bold: cell.bold,
            italic: cell.italic,
            underline: cell.underline,
            cursor: cell.cursor,
            selected: cell.selected,
            search_match: cell.search_match,
            wide: cell.wide,
        })
        .collect::<Vec<_>>();

    app.set_terminal_cells(model(cells));
    app.set_terminal_rows(i32::from(snapshot.rows));
    app.set_terminal_columns(i32::from(snapshot.columns));
    app.set_current_directory(snapshot.cwd.into());
    if snapshot.bell {
        app.set_bell_active(true);
    }
}

fn handle_paste_request(controller: &mut Controller, app: Option<&AppWindow>) {
    match controller.request_paste() {
        Ok(Some(summary)) => {
            if let Some(app) = app {
                app.set_paste_summary(summary.into());
                app.set_paste_confirmation_open(true);
            }
        }
        Ok(None) => {}
        Err(error) => {
            warn!(%error, "failed to prepare terminal paste");
            if let Some(app) = app {
                show_error(app, format!("Could not paste terminal text: {error:#}"));
            }
        }
    }
}

fn paste_summary(prepared: &PreparedPaste) -> String {
    let mut summary = format!(
        "Paste {} lines ({} bytes) into this terminal?",
        prepared.lines, prepared.bytes
    );
    if prepared.removed_control_characters > 0 {
        summary.push_str(&format!(
            " {} control characters were removed.",
            prepared.removed_control_characters
        ));
    }
    summary
}

fn close_search_ui(app: &AppWindow, controller: &mut Controller) {
    if let Some(session) = controller.active_session() {
        session.clear_search();
    }
    if let Err(error) = controller.start_search(String::new(), false) {
        debug!(%error, "failed to cancel scrollback search");
    }
    app.set_search_query("".into());
    app.set_search_status("".into());
    app.set_search_open(false);
    app.invoke_focus_terminal();
}

fn show_error(app: &AppWindow, message: String) {
    app.set_error_message(message.into());
    app.set_error_open(true);
}

fn close_application(app: &AppWindow) {
    if let Err(error) = app.hide() {
        debug!(%error, "failed to hide Termi window during shutdown");
    }
    if let Err(error) = slint::quit_event_loop() {
        debug!(%error, "failed to stop Termi event loop");
    }
}

fn fallback_working_directory() -> PathBuf {
    if let Some(directory) = std::env::current_dir().ok().filter(|path| path.is_dir()) {
        return directory;
    }
    directories::BaseDirs::new()
        .map(|directories| directories.home_dir().to_path_buf())
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items))
}

fn cells_for_extent(extent: f32, cell_extent: f32, minimum: u16, maximum: u16) -> u16 {
    if !extent.is_finite() || !cell_extent.is_finite() || cell_extent <= 0.0 {
        return minimum;
    }
    (extent / cell_extent)
        .floor()
        .clamp(f32::from(minimum), f32::from(maximum)) as u16
}

fn shell_name(shell: &str) -> String {
    Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("shell")
        .to_owned()
}

fn alpha(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn coordinate_to_cell(coordinate: f32, cell_extent: f32) -> u16 {
    if !coordinate.is_finite() || !cell_extent.is_finite() || cell_extent <= 0.0 {
        return 0;
    }
    (coordinate.max(0.0) / cell_extent)
        .floor()
        .clamp(0.0, f32::from(u16::MAX)) as u16
}
