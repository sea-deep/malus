<p align="center">
  <img src="assets/malus-logo.png" alt="Malus Logo" width="128" height="128">
</p>

<h1 align="center">Malus</h1>

<p align="center">
  <strong>A native Apple Music desktop client for Linux built with Rust, GTK4, Libadwaita, and WPE WebKit.</strong>
</p>

<p align="center">
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.80%2B-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust 1.80+"></a>
  <a href="https://gtk.org/"><img src="https://img.shields.io/badge/GTK-4.14%2B-4A90E2?style=flat-square&logo=gtk&logoColor=white" alt="GTK4"></a>
  <a href="https://gnome.pages.gitlab.gnome.org/libadwaita/"><img src="https://img.shields.io/badge/Libadwaita-1.5%2B-2A76D2?style=flat-square&logo=gnome&logoColor=white" alt="Libadwaita"></a>
  <a href="https://wpewebkit.org/"><img src="https://img.shields.io/badge/WPE%20WebKit-2.52-506B82?style=flat-square&logo=webkit&logoColor=white" alt="WPE WebKit"></a>
  <img src="https://img.shields.io/badge/Platform-Linux-FCC624?style=flat-square&logo=linux&logoColor=black" alt="Linux">
  <img src="https://img.shields.io/badge/Audio-Widevine%20AAC--LC-FA243C?style=flat-square&logo=applemusic&logoColor=white" alt="Widevine DRM">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-GPLv3-blue?style=flat-square&logo=gnu&logoColor=white" alt="License: GPLv3"></a>
</p>

<p align="center">
  <a href="#overview">Overview</a> •
  <a href="#features">Features</a> •
  <a href="#architecture">Architecture</a> •
  <a href="#quick-start">Quick Start</a> •
  <a href="#cli-controller-malusctl">CLI Controller</a> •
  <a href="#keyboard-shortcuts">Keyboard Shortcuts</a> •
  <a href="#development--testing">Development</a>
</p>

---

## Overview

**Malus** is a desktop player designed specifically for Apple Music on Linux. It combines a native GTK4/Libadwaita user interface with a decoupled background daemon architecture:

- **Direct HTTP Metadata**: Queries the official Apple Music HTTP API (`api.music.apple.com`) directly via native Rust HTTP requests for low-latency feed generation, search, and library management.
- **Isolated DRM Playback Engine**: Runs an isolated headless WPE WebKit process with Widevine CDM support (`malus-wpe-host`) to decrypt and stream protected Apple Music audio via MusicKit JS.
- **Multi-Process Daemon Architecture**: A central daemon (`malusd`) manages service sessions, audio playback state, and process lifecycles, serving both graphical frontends and terminal controllers via Unix domain sockets.

---

## Features

- **Native Linux Desktop Experience**:
  - Built with GTK4, Libadwaita, and Relm4.
  - Automatically respects system light/dark theme preferences and accent colors.
  - MPRIS v2 media key support and desktop notification integration.
  - Fluid responsive layouts for desktop, split-screen, and narrow-window tiling.
- **Comprehensive Discovery & Feeds**:
  - **Home (Listen Now)**: Personalized recommendations, heavy rotation shelves, and "Made for You" mixes.
  - **New / Browse**: Global and regional top charts, editorial releases, and curated playlists.
  - **Radio**: Live broadcasts (Apple Music 1, Apple Music Hits, Apple Music Country) and continuous algorithmic stations.
  - **Search**: Catalog and library search with instant suggestions and category filters.
- **Personal Library**:
  - Complete access to your Cloud Music Library: Songs, Albums, Artists, Playlists, and Recently Added tracks.
  - Add to and remove from your library directly from album, artist, and track views.
  - Favorite tracks and albums with synchronization to your Apple Music profile.
- **Rich Media Presentation**:
  - Time-synced lyrics with active line tracking.
  - Full album and song credits.
  - Dynamic high-resolution artwork pipeline preserving native Apple CDN crop flags (`cc`, `sr`, `SC.DN01`).
- **Full Playback & Queue Management**:
  - Authoritative playback state driven by MusicKit JS.
  - Play, shuffle, seek, and loop (repeat one / repeat all).
  - Inspect, reorder, play next, and append items to the active queue.
- **CLI Power Tools**:
  - Complete command-line control and inspection via `malusctl`.

---

## Architecture

Malus separates user interfaces, business logic, and browser playback runtimes into distinct processes:

```text
┌──────────────────────────────────────────────────────────────┐
│                      Frontends / UIs                         │
│   malus-gtk (GTK4/Libadwaita)        malusctl (CLI tool)     │
└──────────────────────────────┬───────────────────────────────┘
                               │
                               │ Length-Prefixed JSON over Unix Socket
                               │ (/run/user/<uid>/malus.sock)
                               ▼
┌──────────────────────────────────────────────────────────────┐
│                malusd (Background Daemon)                    │
│   • Client connection manager & session supervisor           │
│   • Normalizes playback state mirrors & coordinates requests │
├──────────────────────────────────────────────────────────────┤
│              crates/malus-service (Engine)                   │
│   • Direct API requests (api.music.apple.com)                │
│   • Token manager (Developer + Media User Tokens)            │
│   • Page models, feed mappers, and search aggregator         │
└──────────────────────────────┬───────────────────────────────┘
                               │
                               │ Bidirectional Stdio JSON IPC
                               ▼
┌──────────────────────────────────────────────────────────────┐
│         malus-wpe-host (Headless WPE WebKit Runtime)         │
│   • Isolated profile & credentials storage (0700)            │
│   • Sandboxed web process (Bubblewrap bwrap)                 │
│   • Widevine CDM decryptor via OpenCDM shim                  │
│   • Authoritative MusicKit JS queue and audio output         │
└──────────────────────────────────────────────────────────────┘
```

### Workspace Structure

| Path | Description |
| :--- | :--- |
| `apps/malus-gtk` | Native desktop application frontend using GTK4, Libadwaita, and Relm4. |
| `crates/malusctl` | CLI controller for playback, system diagnostics, and feed inspection. |
| `crates/malusd` | Background daemon binary, Unix socket server, and engine supervisor. |
| `crates/malus-service` | Apple Music HTTP client, token lifecycle, page models, and WPE driver. |
| `crates/malus-wpe` | WPE WebKit process manager, Bubblewrap sandbox, and OpenCDM shim. |
| `crates/malus-client` | Asynchronous client library for communicating with `malusd`. |
| `crates/malus-ipc` | Length-prefixed JSON wire framing and typed DTOs. |
| `crates/malus-model` | Pure domain models (`Track`, `Album`, `Artist`, `Playlist`, `PlaybackState`). |

---

## Quick Start

### 1. Prerequisites

Ensure your system has the required development libraries installed:

**Fedora:**
```sh
sudo dnf install gtk4-devel libadwaita-devel gstreamer1-devel \
    gstreamer1-plugins-base-devel gstreamer1-plugins-good \
    gstreamer1-plugins-bad-free gstreamer1-plugin-libav bubblewrap
```

**Arch Linux:**
```sh
sudo pacman -S gtk4 libadwaita gst-plugins-base gst-plugins-good \
    gst-plugins-bad gst-libav bubblewrap
```

**Ubuntu / Debian:**
```sh
sudo apt install libgtk-4-dev libadwaita-1-dev libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad gstreamer1.0-libav bubblewrap
```

### 2. Widevine CDM Setup

Malus requires Google's Widevine Content Decryption Module (CDM) to decrypt Apple Music streams. The CLI tool can install or symlink Widevine from your local Google Chrome installation:

```sh
# Run diagnostics to check your environment
cargo run -p malusctl -- doctor

# Install Widevine CDM
cargo run -p malusctl -- setup-widevine --install
```

### 3. Running Malus

**Start the Daemon:**
In a terminal, launch the background daemon:
```sh
cargo run -p malusd
```

**Launch the Desktop App:**
In another terminal, launch the GTK4 application:
```sh
cargo run -p malus-gtk
```

**Authenticate:**
1. In the app, navigate to **Settings** in the sidebar.
2. Click **Sign In to Apple Music**.
3. Complete the official Apple Music web sign-in dialog. Credentials and session cookies are saved securely to an isolated local profile (`~/.local/share/malus/profiles/apple`).

---

## CLI Controller (`malusctl`)

`malusctl` can control playback and inspect live feeds independently of the GUI:

```sh
# Playback controls
cargo run -p malusctl -- play song:<id>       # Play a specific track
cargo run -p malusctl -- play album:<id>      # Queue and play an album
cargo run -p malusctl -- play playlist:<id>   # Queue and play a playlist
cargo run -p malusctl -- play station:<id>    # Stream a radio station
cargo run -p malusctl -- toggle               # Toggle play/pause
cargo run -p malusctl -- pause                # Pause playback
cargo run -p malusctl -- next                 # Skip to next track
cargo run -p malusctl -- prev                 # Return to previous track
cargo run -p malusctl -- seek 45              # Seek to 45 seconds
cargo run -p malusctl -- volume 75            # Set volume to 75%

# State inspection
cargo run -p malusctl -- status               # Show active track and player state
cargo run -p malusctl -- watch                # Stream real-time player event stream

# Feed and catalog inspection
cargo run -p malusctl -- nav                  # Print top-level navigation routes
cargo run -p malusctl -- page home            # Fetch and display Home feed
cargo run -p malusctl -- page new             # Fetch Browse/New charts
cargo run -p malusctl -- page radio           # Fetch Radio stations
cargo run -p malusctl -- page library:songs   # List library tracks
```

---

## Keyboard Shortcuts

| Shortcut | Action |
| :--- | :--- |
| `Space` | Toggle Play / Pause |
| `Ctrl + Right` | Next Track |
| `Ctrl + Left` | Previous Track |
| `Ctrl + Up` / `Ctrl + Down` | Adjust Volume |
| `Ctrl + F` / `/` | Focus Search Bar |
| `Ctrl + Q` | Toggle Up Next / Queue Drawer |
| `Ctrl + L` | Toggle Lyrics View |
| `Escape` | Close Modal / Return to Previous View |

---

## Development & Testing

Malus enforces deterministic code quality checks across the workspace:

```sh
# Verify code formatting
cargo fmt --all -- --check

# Run Clippy checks across all crates
cargo clippy --workspace --all-targets --exclude malus-gtk -- -D warnings

# Run backend unit and integration test suite
cargo test --workspace --exclude malus-gtk

# Run GTK frontend unit tests
cargo test -p malus-gtk --lib
```

For information on building the relocatable WPE WebKit runtime from source, refer to [Building the WPE WebKit Runtime](docs/building-wpe-runtime.md).

---

## License

This project is licensed under the **GNU General Public License v3.0 or later** ([GPL-3.0-or-later](LICENSE)).

Copyright © 2026 sea-deep and Malus Contributors.

---

## Disclaimer

Malus is an independent open-source project and is not affiliated with, endorsed by, or sponsored by Apple Inc. Apple Music is a registered trademark of Apple Inc. An active Apple Music subscription is required for full playback.
