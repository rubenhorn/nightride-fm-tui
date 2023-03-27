use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use serde::Deserialize;
use std::{error::Error, io, process::Command};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout},
    text::{Spans, Text},
    widgets::{Block, BorderType, Paragraph},
    Frame, Terminal,
};

type AnyError = Box<dyn Error>;

// Constants
const APP_TITLE: &str = "Nightride FM - The Home of Synthwave - Synthwave Radio - Free 24/7 Live Streaming | Nightride FM";
const STATION_URL: &str = "http://stream.nightride.fm/nightride.ogg";
const PLAYER_PID_FILE_PATH: &str = "/tmp/nightride.fm.pid";
const INPUT_IPC_SERVER_FILE_PATH: &str = "/tmp/nightride.fm.sock";

/// Get the PID of the player
/// This will check if the process and arguments match
fn get_player_pid() -> Result<String, AnyError> {
    let pid = std::fs::read_to_string(PLAYER_PID_FILE_PATH)?;
    // Check if this process exists and is the correct one
    /*
    let cmdline_expected = format!("{}\0{}\0", "mpv", PLAYER_ARGS.join("\0"));
    let cmdline = std::fs::read_to_string(format!("/proc/{}/cmdline", pid))?;
    match cmdline == cmdline_expected {
        true => Ok(pid),
        false => Err(format!("/proc/{}/cmdline was not as expected", pid).into()),
    }
    */
    Ok(pid)
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
    let player = Command::new("mpv")
        .args([
            STATION_URL.into(),
            format!("--input-ipc-server={}", INPUT_IPC_SERVER_FILE_PATH),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
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

#[derive(Deserialize)]
struct MpvProperty<T> {
    data: Option<T>,
    error: String,
}

#[derive(Debug, Deserialize)]
struct Track {
    title: String,
    artist: String,
    album: String,
}

fn mpv_get_property<T: for<'a> serde::de::Deserialize<'a>>(property: &str) -> Result<T, AnyError> {
    let shell_cmd = format!(
        "echo '{{\"command\":[\"get_property\",\"{}\"]}}' | socat - {}",
        property, INPUT_IPC_SERVER_FILE_PATH
    );
    let shell_output = Command::new("sh").arg("-c").arg(shell_cmd).output()?;
    let result_json = String::from_utf8(shell_output.stdout)?;
    let result: MpvProperty<T> = serde_json::from_str(result_json.as_str())?;
    if result.error != "success" || result.data.is_none() {
        Err(result.error.into())
    } else {
        Ok(result.data.unwrap())
    }
}

fn mpv_set_property<T: serde::Serialize>(property: &str, value: T) -> Result<(), AnyError> {
    let value_json = serde_json::to_string(&value)?;
    let shell_cmd = format!(
        "echo '{{\"command\":[\"set_property\",\"{}\",{}]}}' | socat - {}",
        property, value_json, INPUT_IPC_SERVER_FILE_PATH
    );
    let shell_output = Command::new("sh").arg("-c").arg(shell_cmd).output()?;
    let result_json = String::from_utf8(shell_output.stdout)?;
    let result: MpvProperty<()> = serde_json::from_str(result_json.as_str())?;
    if result.error == "success" {
        Ok(())
    } else {
        Err(result.error.into())
    }
}

fn get_track_info() -> Result<Track, AnyError> {
    return Ok(mpv_get_property::<Track>("metadata")?);
}

struct App {
    is_player_running: bool,
    current_track: Option<Track>,
    volume: f32,
}

impl Default for App {
    fn default() -> Self {
        Self {
            is_player_running: is_player_running(),
            current_track: None,
            volume: mpv_get_property("volume").unwrap_or(100.0),
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
        .constraints([Constraint::Min(1), Constraint::Min(1), Constraint::Min(1)].as_ref())
        .split(f.size());
    f.render_widget(
        Paragraph::new(Text::from(Spans::from(format!(
            "Player State: {}",
            match app.is_player_running {
                true => "playing",
                false => " paused",
            }
        )))),
        chunks[0],
    );
    f.render_widget(
        Paragraph::new(Text::from(Spans::from(format!(
            "Current Track: {}",
            match &app.current_track {
                Some(track) => format!("{:?}", track),
                None => "???".to_string(),
            }
        )))),
        chunks[1],
    );
    f.render_widget(
        Paragraph::new(Text::from(Spans::from(format!("Volume: {}", app.volume)))),
        chunks[2],
    );
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> Result<(), AnyError> {
    loop {
        // Update the UI
        terminal.draw(|f| ui(f, &app))?;

        // Handle events
        app.current_track = get_track_info().ok();
        let mut update_volume = |change: f32| -> Result<(), AnyError> {
            let volume = mpv_get_property::<f32>("volume")?;
            let volume = (volume + change).max(0.0).min(150.0);
            mpv_set_property("volume", volume)?;
            app.volume = volume;
            Ok(())
        };
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('p') => {
                    if app.is_player_running {
                        stop_player()?;
                    } else {
                        start_player()?;
                    }
                    app.is_player_running = !app.is_player_running;
                }
                KeyCode::Char('V') => {
                    update_volume(5.0)?;
                }
                KeyCode::Char('v') => {
                    update_volume(-5.0)?;
                }
                _ => {}
            }
        }
    }
    Ok(())
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
