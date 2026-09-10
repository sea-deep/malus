# Malus

An Apple Music terminal client for Linux, built with Rust, ratatui, and ratcn.

Malus opens Apple's web player in an isolated Chromium profile and controls its existing MusicKit instance over local CDP. Sign-in happens in Apple's browser window. No Apple developer account or private key is needed for this integration.

## Run

Install Rust and a Chromium browser with working Widevine (Google Chrome is the browser tested here). A graphical desktop is needed for sign-in; normal playback runs headless. Audio uses your system's default output.

```sh
cargo run -- --login
cargo run
```

Other options:

```sh
cargo run -- --status
cargo run -- --detect-browsers
cargo run -- --browser-path /path/to/chrome
cargo run -- --demo
cargo run -- --logout
```

The session lives in `$XDG_DATA_HOME/malus/browser-profile` (normally `~/.local/share/malus/browser-profile`). Close the running client before login, status inspection, or logout; Chromium profiles cannot be shared by two processes.

## Controls

| Action | Keyboard | Mouse |
|---|---|---|
| Navigate | `1` Home, `2` Browse, `3` Radio, `4` Library, `5` Playing | Header tabs |
| Move through songs | Arrows or `j` / `k`, Page Up/Down, Home/End | Wheel |
| Play selection | Enter | Click song |
| Play/pause | Space | Player button |
| Next/previous | `n` / `p` | Player buttons |
| Seek | Focus timeline, then Left/Right | Click or drag timeline |
| Volume | `+` / `-` | Click, drag, or wheel over volume slider |
| Shuffle/repeat | `s` / `r` | Player buttons |
| Song actions | `m` on selected song | Right-click or `···` |
| Queue | `q` | Queue button |
| Remove queued song | Delete | Its `···` button |
| Reorder queue | Alt+Up/Down | Drag a queued song |
| Search | `/`; type or paste; Up/Down and Enter | Search button |
| Command palette | Ctrl+K | Available through Help |
| Refresh/reconnect | Ctrl+R | Refresh button |
| Help/settings | `?` / `,` | Header buttons |
| Dismiss/back | Esc | Close/Back; click outside a popup |
| Quit | Ctrl+C or Shift+Q | Quit action in Help |

Tab and Shift+Tab traverse controls. Search captures text without triggering player shortcuts. Set `MALUS_REDUCED_MOTION=1` to disable motion.

## What is connected

- Paginated library songs and playlists; album/artist relationships come from Apple.
- Catalog song search and storefront charts; real radio station discovery.
- Track, album, playlist, and station playback through MusicKit.
- Native playback queue: play next/later, jump, remove, reorder, clear, shuffle and repeat.
- Playback state, elapsed time, real cover images, and MPRIS metadata/property notifications.
- Browser crash detection and reconnect attempts, bounded CDP requests, cancellation cleanup, and explicit error reporting.

`--demo` is explicitly simulated and never starts a browser. Live mode starts empty and never substitutes sample songs. Synchronized lyrics are not supplied by this bridge yet. Artwork uses terminal half-block pixels; no Kitty/Sixel protocol is claimed. Apple can change its web player, and an authorized session alone does not prove subscription playback or a working CDM.

## Verify

```sh
cargo fmt --all -- --check
cargo clippy --lib --bin malus -- -D warnings
cargo test
```

Tests requiring Chromium or an authorized Apple session are opt-in. Close Malus first, then run them serially:

```sh
cargo test -- --include-ignored --test-threads=1
```

The suite covers keyboard/mouse routing and small terminals, real-vs-demo state, search debounce/stale responses, album grouping, queue mutations, rejected JS promises, CDP cancellation/timeouts/disconnects, profile permissions, and browser lifetime. Live verification must also check MusicKit playback time and desktop MPRIS state; a successful build alone is insufficient.
