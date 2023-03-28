# Nightride.FM TUI
A less resource-intensive way to listen to my favorite synth-wave radio station.

This app is Linux-only.

## Installation
1. Install the dependencies (mpv, rust toolchain)
2. Clone the repository
3. Run `cargo install --path .`

## Usage
1. Run `nightride`
2. Press `p` to play/pause
3. Change the volume with `v` and `V`
4. Search the current song on YouTube Music with `y`
5. Switch to the next station with `n`
6. Press `q` to quit

## Roadmap
- [x] Play/pause
- [x] Quit
- [x] Display the current song
- [x] Look up the current song on YouTube Music
- [ ] Nice TUI
- [x] Volume control
- [ ] List of previous songs
- [x] Add more stations
- [x] Remember the last station and volume
- [ ] Seek forward when resuming playback
- [ ] CLI (start/stop, station selection, get YT Music search link, path to mpv socket)
- [ ] Inline code documentation
