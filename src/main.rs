use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{error::Error, io, process::Command};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Alignment},
    text::{Spans, Text},
    widgets::{Block, Paragraph, BorderType},
    Frame, Terminal,
};

type AnyError = Box<dyn Error>;

// Constants
const APP_TITLE: &str = "Nightride FM - The Home of Synthwave - Synthwave Radio - Free 24/7 Live Streaming | Nightride FM";
const PLAYER_EXE: &str = "mpv";
const PLAYER_ARGS: [&str; 1] = ["http://stream.nightride.fm/nightride.ogg"];
const PLAYER_PID_FILE_PATH: &str = "/tmp/nightride.fm.pid";

/// Get the PID of the player
/// This will check if the process and arguments match
fn get_player_pid() -> Result<String, AnyError> {
    let pid = std::fs::read_to_string(PLAYER_PID_FILE_PATH)?;
    // Check if this process exists and is the correct one
    let cmdline_expected = format!("{}\0{}\0", PLAYER_EXE, PLAYER_ARGS.join("\0"));
    let cmdline = std::fs::read_to_string(format!("/proc/{}/cmdline", pid))?;
    match cmdline == cmdline_expected {
        true => Ok(pid),
        false => Err(format!("/proc/{}/cmdline was not as expected", pid).into()), //anyhow::bail!("/proc/{}/cmdline was not as expected", pid),
    }
}

/// Check if the player is running
/// This will return false if the PID file does not exist or is invalid
fn is_player_running() -> bool {
    if !std::path::Path::new(PLAYER_PID_FILE_PATH).exists() {
        return false;
    }
    match get_player_pid() {
        std::result::Result::Ok(_) => true,
        Err(_) => false,
    }
}

/// Start the player
/// This will write the PID of the spawned process to the PID file
fn start_player() -> Result<(), AnyError> {
    let mut player_builder = Command::new(PLAYER_EXE);
    for arg in PLAYER_ARGS.iter() {
        player_builder.arg(arg);
    }
    player_builder.stdout(std::process::Stdio::null());
    player_builder.stderr(std::process::Stdio::null());
    let player = player_builder.spawn()?;
    std::fs::write(PLAYER_PID_FILE_PATH, player.id().to_string())?;
    Ok(())
}

/// Stop the player
/// This will always remove the PID file
/// If the player is running, the corresponding process will be stopped
fn stop_player() -> Result<(), AnyError> {
    let pid = get_player_pid();
    let remove_pid_file = || Ok(std::fs::remove_file(PLAYER_PID_FILE_PATH)?);
    match pid {
        std::result::Result::Ok(pid) => {
            Command::new("kill").arg(pid).spawn()?;
            remove_pid_file()
        }
        Err(_) => remove_pid_file(),
    }
}

struct App {
    is_player_running: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            is_player_running: is_player_running(),
        }
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &App) {
    let size = f.size();
    let block = Block::default()
        .title(format!(" {} ", APP_TITLE))
        .title_alignment(Alignment::Center)
        .borders(tui::widgets::Borders::ALL)
        .border_type(BorderType::Rounded);
    f.render_widget(block, size);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(4)
        .constraints([Constraint::Min(3)].as_ref())
        .split(f.size());

    let player_state = Text::from(Spans::from(match app.is_player_running {
        true => "Player is running",
        false => "Player is not running",
    }));

    f.render_widget(Paragraph::new(player_state), chunks[0]);
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> Result<(), AnyError> {
    loop {
        terminal.draw(|f| ui(f, &app))?;

        if let Event::Key(key) = event::read()? {
            if key.code == KeyCode::Char('q') {
                return Ok(());
            }
            if key.code == KeyCode::Char('p') {
                if app.is_player_running {
                    stop_player()?;
                } else {
                    start_player()?;
                }
                app.is_player_running = !app.is_player_running;
            }
        }
    }
}

fn main() -> Result<(), AnyError> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let app = App::default();
    let res = run_app(&mut terminal, app);
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    if let Err(e) = res {
        eprintln!("Error: {}", e);
    }
    Ok(())
}
