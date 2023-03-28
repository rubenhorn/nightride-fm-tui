use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use home::home_dir;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt::Display,
    io,
    process::Command,
    time::{Duration, Instant},
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Direction, Layout},
    text::{Spans, Text},
    widgets::{Block, BorderType, Paragraph},
    Frame, Terminal,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

// Constants
const APP_TITLE: &str = "Nightride FM - The Home of Synthwave";
const STATION_BASE_URL: &str = "http://stream.nightride.fm/";
const STATIONS: [&str; 7] = [
    "nightride",
    "chillsynth",
    "datawave",
    "spacesynth",
    "darksynth",
    "horrorsynth",
    "ebsm",
];
const INPUT_IPC_SERVER_FILE_PATH: &str = "/tmp/nightride.sock";
const POLLING_RATE: Duration = Duration::from_secs(1);
const YT_MUSIC_SEARCH_URL: &str = "https://music.youtube.com/search?q=";
const USER_SERIALIZED_APP_FILE_PATH: &str = ".local/share/nightride/app.json"; // relative to home dir

/// Start the player
fn mpv_start(station: usize) -> Result<()> {
    let station_url = format!("{}{}.ogg", STATION_BASE_URL, STATIONS[station]);
    // Use nohup to avoid the process being killed when the terminal is closed
    Command::new("nohup")
        .args([
            "mpv",
            station_url.as_str(),
            format!("--input-ipc-server={}", INPUT_IPC_SERVER_FILE_PATH).as_str(),
            ">/dev/null", // Do not create nohup.out
            "2>&1",       // Redirect stderr to stdout
            "&",          // Run in background
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

/// Stop the player
/// This will query the socket for the PID of the running process and kill it
fn mpv_stop() -> Result<()> {
    if let Ok(pid) = mpv_get_property::<u32>("pid") {
        // Ignore errors (MPV might not have been running)
        Command::new("kill").arg(pid.to_string()).output()?;
    }
    Ok(())
}

/// Ensure that the player is running and playing the station
fn ensure_playing_station(station: usize) -> Result<()> {
    let is_running_station = mpv_get_property::<String>("filename")
        .unwrap_or("".into())
        .split(".")
        .nth(0)
        .unwrap_or("")
        == STATIONS[station];
    if !is_running_station {
        mpv_stop()?;
        mpv_start(station)?;
    }
    Ok(())
}

#[derive(Deserialize)]
struct MpvProperty<T> {
    data: Option<T>,
    error: String,
}

fn mpv_get_property<T: for<'a> serde::de::Deserialize<'a>>(property: &str) -> Result<T> {
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

fn mpv_set_property<T: serde::Serialize>(property: &str, value: T) -> Result<()> {
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

#[derive(Debug, Serialize, Deserialize)]
struct Track {
    title: String,
    artist: String,
    album: String,
}

impl Display for Track {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} by {} ({})", self.title, self.artist, self.album)
    }
}

impl Track {
    fn search_yt_music(&self) {
        let search_url =
            format!("{}{} {}", YT_MUSIC_SEARCH_URL, self.title, self.artist).replace(" ", "+");
        Command::new("xdg-open").arg(search_url).spawn().ok();
    }
}

#[derive(Serialize, Deserialize)]
struct App {
    is_paused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_track: Option<Track>,
    volume: f32,
    station: usize,
}

impl Default for App {
    fn default() -> Self {
        Self {
            is_paused: true,
            current_track: None,
            volume: 100.0,
            station: 0,
        }
    }
}

impl App {
    fn update(&mut self) {
        if let Ok(is_paused) = mpv_get_property("pause") {
            self.is_paused = is_paused;
        }
        if let Ok(volume) = mpv_get_property("volume") {
            self.volume = volume;
        }
        self.current_track = get_track_info().ok();
        if let Some(station) = STATIONS
            .iter()
            .position(|&s| s == mpv_get_property::<String>("filename").unwrap_or_default())
        {
            self.station = station;
        }
    }

    fn load() -> Self {
        let app = match serde_json::from_str(
            std::fs::read_to_string(
                home_dir()
                    .unwrap_or_default()
                    .join(USER_SERIALIZED_APP_FILE_PATH),
            )
            .unwrap_or("".into())
            .as_str(),
        ) {
            Ok(app) => app,
            Err(_) => Self::default(),
        };
        ensure_playing_station(app.station).ok();
        app
    }

    fn store(&self) -> Result<()> {
        // Make path if it doesn't exist
        let path = home_dir()
            .ok_or("Could not get home directory")?
            .join(USER_SERIALIZED_APP_FILE_PATH);
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).ok();
            }
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

fn get_track_info() -> Result<Track> {
    let track_info = mpv_get_property::<Track>("metadata")?;
    // MPV appends successive metadata to the end of the string, separated by semicolons
    let get_last = |s: String| s.split(";").last().unwrap().to_string();
    Ok(Track {
        title: get_last(track_info.title),
        artist: get_last(track_info.artist),
        album: get_last(track_info.album),
    })
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
        .constraints(vec![Constraint::Min(1); 4])
        .split(f.size());
    f.render_widget(
        Paragraph::new(Text::from(Spans::from(format!(
            "Station: {}",
            STATIONS[app.station]
        )))),
        chunks[0],
    );
    f.render_widget(
        Paragraph::new(Text::from(Spans::from(format!(
            "State:   {}",
            match app.is_paused {
                true => "paused",
                false => "playing",
            }
        )))),
        chunks[1],
    );
    f.render_widget(
        Paragraph::new(Text::from(Spans::from(format!(
            "Track:   {}",
            match &app.current_track {
                Some(track) => format!("{}", track),
                None => "...".to_string(),
            }
        )))),
        chunks[2],
    );
    f.render_widget(
        Paragraph::new(Text::from(Spans::from(format!("Volume:  {}", app.volume)))),
        chunks[3],
    );
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let mut next_poll = Instant::now();
    loop {
        // Debounce updates and be easy on the IO
        if next_poll < Instant::now() {
            // Synchronize app state with mpv (and perhaps start mpv if it's not running)
            app.update();
            next_poll = Instant::now() + POLLING_RATE;
        }

        // Update the UI
        terminal.draw(|f| ui(f, &app))?;

        // Handle events
        let mut update_volume = |change: f32| -> Result<()> {
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
                    mpv_set_property("pause", !app.is_paused)?;
                    app.is_paused = !app.is_paused;
                }
                KeyCode::Char('V') => {
                    update_volume(5.0)?;
                }
                KeyCode::Char('v') => {
                    update_volume(-5.0)?;
                }
                KeyCode::Char('y') => {
                    app.current_track = get_track_info().ok();
                    if let Some(track) = &app.current_track {
                        track.search_yt_music();
                    }
                }
                KeyCode::Char('n') => {
                    app.station = (app.station + 1) % STATIONS.len();
                    ensure_playing_station(app.station)?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::load();
    let res = run_app(&mut terminal, &mut app);
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
    app.store()?;
    Ok(())
}
