mod auth;
mod system;
mod tui;
mod users;

use std::io::{self, stdout, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::SetBackgroundColor,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use zeroize::Zeroizing;

/// Active screen in the application.
#[derive(Debug, Clone, PartialEq)]
pub enum Screen {
    UserSelect,
    Login,
}

/// Application state.
pub struct App {
    pub screen: Screen,
    pub running: bool,
    pub users: Vec<String>,
    pub selected_index: usize,
    pub search_active: bool,
    pub search_query: String,
    pub password: Zeroizing<String>,
    pub auth_error: bool,
    /// None = focus on main content (list or password input).
    /// Some(n) = focus on button n (0 = left, 1 = right).
    pub button_focus: Option<usize>,
    /// For mask delay: shows the last typed character until this instant.
    pub unmasked_until: Option<Instant>,
    /// For backspace flash: hides cursor briefly to show deletion.
    pub deleted_until: Option<Instant>,
    /// Whether PAM is currently running in the background.
    pub authenticating: bool,
    /// Channel to receive the PAM result.
    pub auth_rx: Option<Receiver<bool>>,
    /// Current spinner frame index.
    pub spinner_frame: usize,
    /// When Some, a confirmation dialog is shown for this system action.
    pub confirm_action: Option<system::SystemAction>,
    /// Focus within the confirmation dialog: 0 = No, 1 = Yes.
    /// Focus within the confirmation dialog: 0 = No, 1 = Yes.
    pub confirm_focus: usize,
    /// Channel to tell the background thread to execute uwsm.
    pub exec_tx: Option<Sender<()>>,
    /// True if authentication succeeded.
    pub auth_success: bool,
    /// Icon style to display
    pub icon_style: tui::IconStyle,
    /// If true, the renderer will issue a full terminal clear before drawing.
    pub needs_clear: bool,
}

impl App {
    pub fn new() -> Self {
        let users = users::list_real_users();
        
        let mut icon_style = tui::IconStyle::None;
        for arg in std::env::args().skip(1) {
            if arg == "--icon=filled" {
                icon_style = tui::IconStyle::Filled;
            } else if arg == "--icon=outline" {
                icon_style = tui::IconStyle::Outline;
            }
        }

        Self {
            screen: Screen::UserSelect,
            running: true,
            users,
            selected_index: 0,
            search_active: false,
            search_query: String::new(),
            password: Zeroizing::new(String::new()),
            auth_error: false,
            button_focus: None,
            unmasked_until: None,
            deleted_until: None,
            authenticating: false,
            auth_rx: None,
            spinner_frame: 0,
            confirm_action: None,
            confirm_focus: 0,
            exec_tx: None,
            auth_success: false,
            icon_style,
            needs_clear: true,
        }
    }

    /// Clear expired timeouts.
    pub fn tick_timeouts(&mut self) {
        let now = Instant::now();
        if let Some(t) = self.unmasked_until {
            if now >= t {
                self.unmasked_until = None;
            }
        }
        if let Some(t) = self.deleted_until {
            if now >= t {
                self.deleted_until = None;
            }
        }
    }

    /// Get duration until the next timeout expires.
    pub fn timeout_duration(&self) -> Option<Duration> {
        let now = Instant::now();
        let mut dur = None;
        if let Some(t) = self.unmasked_until {
            if t > now {
                dur = Some(t.duration_since(now));
            }
        }
        if let Some(t) = self.deleted_until {
            if t > now {
                let d = t.duration_since(now);
                dur = Some(dur.map_or(d, |min_d| d.min(min_d)));
            }
        }
        dur
    }

    /// Return the filtered user list based on the current search query.
    pub fn filtered_users(&self) -> Vec<&str> {
        if self.search_query.is_empty() {
            self.users.iter().map(|s| s.as_str()).collect()
        } else {
            let query = self.search_query.to_lowercase();
            self.users
                .iter()
                .filter(|u| u.to_lowercase().contains(&query))
                .map(|s| s.as_str())
                .collect()
        }
    }

    /// Get the currently selected username (from the filtered view).
    pub fn selected_username(&self) -> Option<&str> {
        self.filtered_users().get(self.selected_index).copied()
    }
}

fn main() -> io::Result<()> {
    terminal::enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        cursor::Hide,
        SetBackgroundColor(tui::BG),
    )?;

    // Restore terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, cursor::Show);
        original_hook(info);
    }));

    let result = run(&mut stdout);

    execute!(stdout, LeaveAlternateScreen, cursor::Show)?;
    terminal::disable_raw_mode()?;

    if let Ok(Some(tx)) = result {
        // Clear the screen so TTY logs don't bleed into the next application
        let _ = execute!(
            std::io::stdout(),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
            crossterm::cursor::MoveTo(0, 0)
        );

        // Signal background thread to proceed with uwsm select and exec
        let _ = tx.send(());
        
        // Wait forever. The background thread is now running `uwsm select` (which waits for user input)
        // and will eventually `exec` into the graphical session. When `exec` happens, the entire
        // process is replaced by the OS, so this infinite sleep will automatically be killed.
        loop {
            std::thread::park();
        }
    }

    Ok(())
}

fn run(stdout: &mut impl Write) -> io::Result<Option<Sender<()>>> {
    let mut app = App::new();

    loop {
        app.tick_timeouts();

        // Poll PAM result when auth is running in background
        if app.authenticating {
            if let Some(rx) = &app.auth_rx {
                use std::sync::mpsc::TryRecvError;
                match rx.try_recv() {
                    Ok(true) => {
                        app.running = false;
                        app.auth_success = true;
                    }
                    Ok(false) | Err(TryRecvError::Disconnected) => {
                        app.authenticating = false;
                        app.auth_rx = None;
                        app.auth_error = true;
                        app.needs_clear = true;
                        app.password.clear();
                    }
                    Err(TryRecvError::Empty) => {
                        // Still waiting — advance spinner
                        app.spinner_frame = (app.spinner_frame + 1) % 8;
                    }
                }
            }
        }

        tui::render(stdout, &app)?;
        app.needs_clear = false;

        let poll_timeout = if app.authenticating {
            Duration::from_millis(100) // ~10fps for spinner
        } else if let Some(dur) = app.timeout_duration() {
            dur
        } else {
            Duration::from_secs(60 * 60)
        };

        if event::poll(poll_timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    if !app.authenticating {
                        let old_len = app.filtered_users().len();
                        let old_auth_error = app.auth_error;
                        
                        handle_input(&mut app, key);
                        
                        if app.filtered_users().len() != old_len || app.auth_error != old_auth_error {
                            app.needs_clear = true;
                        }
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        if !app.running {
            break;
        }
    }

    if app.auth_success {
        Ok(app.exec_tx.take())
    } else {
        Ok(None)
    }
}

/// Spawn a background thread to run PAM. Sets authenticating = true.
fn start_auth(username: &str, password: &str, app: &mut App) {
    let username = username.to_string();
    let password = password.to_string();
    let (tx, rx) = mpsc::channel();
    let (exec_tx, exec_rx) = mpsc::channel();

    std::thread::spawn(move || {
        auth::authenticate_and_launch(&username, &password, tx, exec_rx);
    });

    app.authenticating = true;
    app.spinner_frame = 0;
    app.auth_rx = Some(rx);
    app.exec_tx = Some(exec_tx);
}

fn handle_input(app: &mut App, key: KeyEvent) {
    // Dev-only exit: Ctrl+C
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.running = false;
        return;
    }

    // Confirmation dialog intercepts all input when active
    if app.confirm_action.is_some() {
        handle_confirm(app, key);
        return;
    }

    match &app.screen {
        Screen::UserSelect => {
            if app.search_active {
                handle_search(app, key);
            } else {
                handle_user_select(app, key);
            }
        }
        Screen::Login => handle_login(app, key),
    }
}

/// Handle input while the confirmation dialog is open.
fn handle_confirm(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Left => {
            app.confirm_focus = 0;
        }
        KeyCode::Right => {
            app.confirm_focus = 1;
        }
        KeyCode::Esc => {
            app.confirm_action = None;
        }
        KeyCode::Enter => {
            if app.confirm_focus == 1 {
                // Confirmed — execute the action
                if let Some(action) = app.confirm_action.take() {
                    match action {
                        system::SystemAction::Shutdown => system::shutdown(),
                        system::SystemAction::Reboot => system::reboot(),
                    }
                }
            } else {
                // Cancelled
                app.confirm_action = None;
            }
        }
        _ => {}
    }
}

// ── UserSelect screen ───────────────────────────────────────────

fn handle_user_select(app: &mut App, key: KeyEvent) {
    if app.button_focus.is_some() {
        handle_user_buttons(app, key);
        return;
    }

    let len = app.filtered_users().len();
    if len == 0 {
        match key.code {
            KeyCode::Char('/') => {
                app.search_active = true;
                app.search_query.clear();
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.confirm_action = Some(system::SystemAction::Shutdown);
                app.confirm_focus = 0;
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.confirm_action = Some(system::SystemAction::Reboot);
                app.confirm_focus = 0;
            }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::Up => {
            if app.selected_index > 0 {
                app.selected_index -= 1;
            }
        }
        KeyCode::Down => {
            if app.selected_index < len - 1 {
                app.selected_index += 1;
            } else {
                // Past last user → focus buttons
                app.button_focus = Some(0);
            }
        }
        KeyCode::Left => {
            app.button_focus = Some(0);
        }
        KeyCode::Right => {
            app.button_focus = Some(1);
        }
        KeyCode::Enter => {
            app.password.clear();
            app.auth_error = false;
            app.button_focus = None;
            app.screen = Screen::Login;
            app.needs_clear = true;
        }
        KeyCode::Char('/') => {
            app.search_active = true;
            app.search_query.clear();
            app.selected_index = 0;
        }
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.confirm_action = Some(system::SystemAction::Shutdown);
            app.confirm_focus = 0;
        }
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.confirm_action = Some(system::SystemAction::Reboot);
            app.confirm_focus = 0;
        }
        _ => {}
    }
}

fn handle_user_buttons(app: &mut App, key: KeyEvent) {
    let focus = app.button_focus.unwrap_or(0);

    match key.code {
        KeyCode::Left => {
            app.button_focus = Some(0);
        }
        KeyCode::Right => {
            app.button_focus = Some(1);
        }
        KeyCode::Up => {
            // Back to user list (last user)
            app.button_focus = None;
        }
        KeyCode::Enter => {
            match focus {
                0 => {
                    app.confirm_action = Some(system::SystemAction::Reboot);
                    app.confirm_focus = 0;
                }
                1 => {
                    app.confirm_action = Some(system::SystemAction::Shutdown);
                    app.confirm_focus = 0;
                }
                _ => {}
            }
        }
        KeyCode::Char('/') => {
            app.button_focus = None;
            app.search_active = true;
            app.search_query.clear();
            app.selected_index = 0;
        }
        _ => {}
    }
}

// ── Search mode ─────────────────────────────────────────────────

fn handle_search(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.search_active = false;
            app.search_query.clear();
            app.selected_index = 0;
        }
        KeyCode::Enter => {
            if !app.filtered_users().is_empty() {
                app.password.clear();
                app.auth_error = false;
                app.button_focus = None;
                app.screen = Screen::Login;
                app.needs_clear = true;
            }
        }
        KeyCode::Backspace => {
            if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
                app.search_query.clear();
            } else {
                app.search_query.pop();
            }
            clamp_selection(app);
        }
        KeyCode::Up => {
            let len = app.filtered_users().len();
            if len > 0 && app.selected_index > 0 {
                app.selected_index -= 1;
            }
        }
        KeyCode::Down => {
            let len = app.filtered_users().len();
            if len > 0 && app.selected_index < len - 1 {
                app.selected_index += 1;
            }
        }
        KeyCode::Char('u' | 'w' | 'h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.search_query.clear();
            clamp_selection(app);
        }
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.confirm_action = Some(system::SystemAction::Shutdown);
            app.confirm_focus = 0;
        }
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.confirm_action = Some(system::SystemAction::Reboot);
            app.confirm_focus = 0;
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) && app.search_query.len() < 32 => {
            app.search_query.push(c);
            app.selected_index = 0;
        }
        _ => {}
    }
}

// ── Login screen ────────────────────────────────────────────────

fn handle_login(app: &mut App, key: KeyEvent) {
    if app.button_focus.is_some() {
        handle_login_buttons(app, key);
        return;
    }

    match key.code {
        KeyCode::Esc => {
            app.password.clear();
            app.auth_error = false;
            app.button_focus = None;
            app.screen = Screen::UserSelect;
            app.needs_clear = true;
        }
        KeyCode::Enter => {
            let username = app.selected_username().unwrap_or("").to_string();
            let password = app.password.clone();
            start_auth(&username, &password, app);
        }
        KeyCode::Down => {
            app.button_focus = Some(0);
        }
        KeyCode::Left => {
            app.button_focus = Some(0);
        }
        KeyCode::Right => {
            app.button_focus = Some(1);
        }
        KeyCode::Backspace => {
            if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
                app.password.clear();
            } else {
                app.password.pop();
            }
            app.auth_error = false;
            app.unmasked_until = None;
            app.deleted_until = Some(Instant::now() + Duration::from_millis(150));
        }
        KeyCode::Char('u' | 'w' | 'h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.password.clear();
            app.auth_error = false;
            app.unmasked_until = None;
            app.deleted_until = Some(Instant::now() + Duration::from_millis(150));
        }
        KeyCode::Char(c) if !key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) && app.password.len() < 128 => {
            app.password.push(c);
            app.auth_error = false;
            app.unmasked_until = Some(Instant::now() + Duration::from_millis(500));
        }
        _ => {}
    }
}

fn handle_login_buttons(app: &mut App, key: KeyEvent) {
    let focus = app.button_focus.unwrap_or(0);

    match key.code {
        KeyCode::Left => {
            app.button_focus = Some(0);
        }
        KeyCode::Right => {
            app.button_focus = Some(1);
        }
        KeyCode::Up => {
            app.button_focus = None;
        }
        KeyCode::Esc => {
            app.password.clear();
            app.auth_error = false;
            app.button_focus = None;
            app.screen = Screen::UserSelect;
            app.needs_clear = true;
        }
        KeyCode::Enter => {
            match focus {
                0 => {
                    // Back
                    app.password.clear();
                    app.auth_error = false;
                    app.button_focus = None;
                    app.screen = Screen::UserSelect;
            app.needs_clear = true;
                    app.needs_clear = true;
                }
                1 => {
                    // Confirm
                    let username = app.selected_username().unwrap_or("").to_string();
                    let password = app.password.clone();
                    app.button_focus = None;
                    start_auth(&username, &password, app);
                }
                _ => {}
            }
        }
        _ => {}
    }
}

/// Clamp selected_index to the filtered list bounds.
fn clamp_selection(app: &mut App) {
    let len = app.filtered_users().len();
    if len == 0 {
        app.selected_index = 0;
    } else if app.selected_index >= len {
        app.selected_index = len - 1;
    }
}
