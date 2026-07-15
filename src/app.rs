use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use slint::{
    Color, ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel,
    winit_030::WinitWindowAccessor,
};
use tracing::{debug, error, warn};

use crate::{
    AppWindow, TabData, TerminalCell,
    config::AppConfig,
    terminal::{TerminalSession, encode_key},
};

struct Controller {
    config: AppConfig,
    sessions: Vec<Arc<TerminalSession>>,
    active: usize,
    next_id: u64,
    clipboard: Option<arboard::Clipboard>,
}

impl Controller {
    fn new(config: AppConfig) -> Result<Self> {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/home/josh"));
        let first = TerminalSession::spawn(
            1,
            &config.shell,
            &cwd,
            config.initial_columns,
            config.initial_rows,
            config.scrollback_lines,
        )?;

        Ok(Self {
            config,
            sessions: vec![first],
            active: 0,
            next_id: 2,
            clipboard: arboard::Clipboard::new().ok(),
        })
    }

    fn active_session(&self) -> Option<&Arc<TerminalSession>> {
        self.sessions.get(self.active)
    }

    fn new_tab(&mut self) -> Result<()> {
        let cwd = self
            .active_session()
            .map(|session| PathBuf::from(session.cwd()))
            .filter(|path| path.is_dir())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/home/josh"));

        let session = TerminalSession::spawn(
            self.next_id,
            &self.config.shell,
            &cwd,
            self.config.initial_columns,
            self.config.initial_rows,
            self.config.scrollback_lines,
        )?;
        self.next_id += 1;
        self.sessions.push(session);
        self.active = self.sessions.len() - 1;
        Ok(())
    }

    fn close_tab(&mut self, index: usize) -> Result<()> {
        if index >= self.sessions.len() {
            return Ok(());
        }

        self.sessions.remove(index);
        if self.sessions.is_empty() {
            self.active = 0;
            self.new_tab()?;
        } else if self.active >= self.sessions.len() {
            self.active = self.sessions.len() - 1;
        } else if index < self.active {
            self.active -= 1;
        }
        Ok(())
    }

    fn activate_tab(&mut self, index: usize) {
        if index < self.sessions.len() {
            self.active = index;
            if let Some(session) = self.active_session() {
                session.take_dirty();
            }
        }
    }

    fn copy_visible(&mut self) {
        let Some(text) = self.active_session().map(|session| {
            session
                .selected_text()
                .unwrap_or_else(|| session.visible_text())
        }) else {
            return;
        };
        if let Some(clipboard) = self.clipboard.as_mut() {
            if let Err(error) = clipboard.set_text(text) {
                warn!(%error, "failed to copy terminal text");
            }
        }
    }

    fn paste(&mut self) {
        let text = self
            .clipboard
            .as_mut()
            .and_then(|clipboard| clipboard.get_text().ok());
        let Some(text) = text else {
            return;
        };
        if let Some(session) = self.active_session() {
            if let Err(error) = session.paste(&text) {
                warn!(%error, "failed to paste into terminal");
            }
        }
    }
}

pub fn run(config: AppConfig) -> Result<()> {
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .select()
        .context("failed to select Slint winit backend")?;

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

        let controller = timer_controller.borrow();
        let columns = cells_for_extent(
            app.get_terminal_viewport_width(),
            controller.config.cell_width,
            20,
        );
        let rows = cells_for_extent(
            app.get_terminal_viewport_height(),
            controller.config.cell_height,
            6,
        );

        if let Some(session) = controller.active_session() {
            if let Err(error) = session.resize(columns, rows) {
                warn!(%error, "terminal resize failed");
            }
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
        // Keep the mutable RefCell borrow in its own scope. A temporary
        // borrow created directly in a match expression remains alive
        // through the selected match arm.
        let result = {
            let mut controller = callback_controller.borrow_mut();
            controller.new_tab()
        };

        match result {
            Ok(()) => {
                if let Some(app) = app_weak.upgrade() {
                    let controller = callback_controller.borrow();
                    refresh_ui(&app, &controller);
                }
            }
            Err(error) => error!(%error, "failed to open terminal tab"),
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_close_tab(move |index| {
        if index < 0 {
            return;
        }

        let result = {
            let mut controller = callback_controller.borrow_mut();
            controller.close_tab(index as usize)
        };

        match result {
            Ok(()) => {
                if let Some(app) = app_weak.upgrade() {
                    let controller = callback_controller.borrow();
                    refresh_ui(&app, &controller);
                }
            }
            Err(error) => error!(%error, "failed to close terminal tab"),
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_close_active_tab(move || {
        let active = callback_controller.borrow().active;
        match callback_controller.borrow_mut().close_tab(active) {
            Ok(()) => {
                if let Some(app) = app_weak.upgrade() {
                    refresh_ui(&app, &callback_controller.borrow());
                }
            }
            Err(error) => error!(%error, "failed to close active terminal tab"),
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
        }
    });

    let callback_controller = Rc::clone(&controller);
    app.on_terminal_key(move |text, control, alt, shift, _meta| {
        let controller = callback_controller.borrow();
        let Some(session) = controller.active_session() else {
            return;
        };
        session.clear_selection();
        let application_cursor = session.application_cursor();
        if let Some(bytes) = encode_key(text.as_str(), control, alt, shift, application_cursor) {
            if let Err(error) = session.write(&bytes) {
                warn!(%error, "failed to send keyboard input to terminal");
            }
        }
    });

    let callback_controller = Rc::clone(&controller);
    app.on_terminal_scroll(move |delta| {
        let lines = if delta > 0.0 {
            3
        } else if delta < 0.0 {
            -3
        } else {
            0
        };
        if lines != 0 {
            if let Some(session) = callback_controller.borrow().active_session() {
                session.scroll(lines);
            }
        }
    });

    let app_weak = app.as_weak();
    let callback_controller = Rc::clone(&controller);
    app.on_terminal_pointer(move |x, y, phase| {
        let controller = callback_controller.borrow();
        let Some(session) = controller.active_session() else {
            return;
        };
        let column = coordinate_to_cell(x, controller.config.cell_width);
        let row = coordinate_to_cell(y, controller.config.cell_height);
        match phase {
            0 => session.begin_selection(column, row),
            1 => session.update_selection(column, row),
            2 => session.finish_selection(column, row),
            _ => {}
        }
        if let Some(app) = app_weak.upgrade() {
            refresh_ui(&app, &controller);
        }
    });

    let callback_controller = Rc::clone(&controller);
    app.on_copy_request(move || callback_controller.borrow_mut().copy_visible());

    let callback_controller = Rc::clone(&controller);
    app.on_paste_request(move || callback_controller.borrow_mut().paste());

    app.on_open_menu(|| {
        debug!("Termi menu requested; settings surface is the next UI slice");
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

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items))
}

fn cells_for_extent(extent: f32, cell_extent: f32, minimum: u16) -> u16 {
    if !extent.is_finite() || !cell_extent.is_finite() || cell_extent <= 0.0 {
        return minimum;
    }
    (extent / cell_extent)
        .floor()
        .clamp(f32::from(minimum), f32::from(u16::MAX)) as u16
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
