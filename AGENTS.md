# Working on Malus

Treat executable code and live behavior as evidence. The previous architecture document included aspirational and inaccurate claims; do not infer capabilities from it.

## Architecture

Malus is an Apple-First native Linux audio client. The multi-provider framework was retired and archived at git tag `provider-framework-final` (see `archive/provider-framework/`).

- `crates/malus-apple`: In-process Apple Music service (`AppleService`), official HTTP API client (`api.music.apple.com`), response normalization, token management, and WPE MusicKit session for DRM/playback.
- `crates/malus-daemon`: Background daemon (`malusd`), client Unix domain socket server, in-process engine coordinator, and MPRIS host.
- `crates/malus-client`: Asynchronous IPC client library connecting to `malus-daemon` over Unix domain sockets.
- `crates/malus-protocol`: Framing (Length-Prefixed JSON), client/daemon wire protocol, and typed Apple page / navigation models.
- `crates/malus-web-runtime`: WPE WebKit process manager, headless/CDP runtime, isolated profile manager, and Widevine CDM supervisor.
- `crates/malus-core`: Domain models (MediaId, Track, Album, Artist, PlaybackState).
- `crates/malus-cli`: CLI interface executable (`malus`).
- `apps/malus-gui` / `apps/malus-gui-next`: Native GTK4 / Libadwaita / Relm4 frontends.

## Invariants

1. Live playback belongs to MusicKit. Do not optimistically claim that a track is playing because a command was queued. Errors and rejected promises must reach the user.
2. Mock content is only for explicit `--demo` and tests. No fabricated stations, lyrics, listening history, audio-quality badges, or subscription claims.
3. Library, catalog search, charts, and separately loaded playlist resources must not overwrite or contaminate each other. Ignore stale search replies.
4. Use Apple's resource IDs and relationships. Song artist text can contain featured artists; it is not a reliable album identity. Keep library IDs and catalog playback IDs distinct.
5. Queue mutations must change MusicKit's actual queue. Check the expected item identity before applying an index-based action. MusicKit queue `splice` takes an **array** as its third argument; reorder a contiguous range atomically.
6. Pass dynamic JS values as `call_function` arguments. Never interpolate track IDs, search queries, or user text into executable JavaScript.
7. Bound external I/O. Request cancellation must remove pending CDP entries. Do not block event delivery or MPRIS updates while awaiting a slow playback command or library request.
8. Keep the browser owned by a task with deterministic cleanup. Preserve parent-death signalling and reap killed children. Closing or failing the UI must stop its engine.
9. Use `ProfileManager` for the isolated profile. Preserve 0700 permissions, reject symlinked profile roots, and refuse logout while that profile is in use. Never print cookies, tokens, passwords, or authentication responses.
10. Modal state and the declared ratcn modal tree must agree, including at tiny terminal sizes. Preserve focus on close and prevent click-through. Capture a pointer only on its matching Down event.
11. Keep the current borderless design and working mouse/keyboard controls. The user is taking over further visual polish; prioritize backend correctness and functional regressions.

## Verification

Use `cargo fmt --all -- --check`, `cargo clippy --lib --bin malus -- -D warnings`, and `cargo test` for deterministic checks. Browser/account tests are marked ignored and must be explicitly run with `cargo test -- --include-ignored --test-threads=1`, with the normal client closed.

Relevant regressions live in `tests/ui_interactions.rs`, `tests/cdp_transport.rs`, and `tests/playback_commands.rs`. Verify live playback, queue behavior, and MPRIS after changing the MusicKit integration. Do not label Widevine, subscription status, synchronized lyrics, universal browser support, or navigation-origin enforcement as verified without runtime evidence.
