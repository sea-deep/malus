# Malus

An Apple Music client for Linux, built with Rust, WPE WebKit, and GTK4 / Libadwaita.

Malus is an Apple-First native Linux audio player. It connects directly to the official Apple Music HTTP API for metadata and feed generation, while using an isolated WPE WebKit runtime for Widevine DRM audio decryption and playback.

## Architecture

- `crates/malus-model`: Pure domain models (`MediaRef`, `Track`, `Album`, `Artist`, `Playlist`, `PlaybackState`).
- `crates/malus-ipc`: Framing (Length-Prefixed JSON), client/daemon wire protocol, and typed product page models.
- `crates/malus-client`: Asynchronous IPC client library connecting to `malusd` over Unix domain sockets.
- `crates/malusd`: Background daemon (`malusd`), Unix domain socket IPC server, and in-process engine coordinator.
- `crates/malus-service`: In-process Apple Music service (`AppleService`), official HTTP API client, and WPE MusicKit session.
- `crates/malus-wpe`: WPE WebKit process manager, headless/CDP runtime, isolated profile manager, and Widevine supervisor.
- `crates/malusctl`: CLI inspection and player controller (`malusctl`).
- `apps/malus-gtk`: Native GTK4 / Libadwaita frontend.

## Quick Start

### 1. Build and Diagnostics

```sh
cargo build --workspace --exclude malus-gtk
cargo run -p malusctl -- doctor
```

### 2. Widevine Setup

```sh
cargo run -p malusctl -- setup-widevine --install
```

### 3. Start Daemon

```sh
cargo run -p malusd
```

### 4. Interactive Controller / Inspection (`malusctl`)

In another terminal:

```sh
# Browse navigation tree
cargo run -p malusctl -- nav

# Inspect product page feeds
cargo run -p malusctl -- page home
cargo run -p malusctl -- page new
cargo run -p malusctl -- page radio
cargo run -p malusctl -- page library:songs
cargo run -p malusctl -- page album:<id>

# Playback controls
cargo run -p malusctl -- play song:<id>
cargo run -p malusctl -- pause
cargo run -p malusctl -- toggle
cargo run -p malusctl -- status
cargo run -p malusctl -- watch
```

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --exclude malus-gtk -- -D warnings
cargo test --workspace --exclude malus-gtk
```
