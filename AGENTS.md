# Working on Malus

Treat executable code and live behavior as evidence. The previous architecture document included aspirational and inaccurate claims; do not infer capabilities from it.

## Architecture

Malus is an Apple-First native Linux audio client. The multi-provider framework was retired and archived at git tag `provider-framework-final` (see `archive/provider-framework/`).

- `crates/malus-model`: Domain models (`MediaRef`, `Track`, `Album`, `Artist`, `Playlist`, `PlaybackState`). Pure domain logic without serde or external runtime dependencies.
- `crates/malus-ipc`: Framing (Length-Prefixed JSON), client/daemon wire protocol, and typed Apple page / navigation models (`PageWire`, `NavigationWire`, `PageRoute`, `MediaRefWire`).
- `crates/malus-client`: Asynchronous IPC client library connecting to `malusd` over Unix domain sockets.
- `crates/malusd`: Background daemon binary (`malusd`), client Unix domain socket server, and in-process engine coordinator hosting `AppleService`.
- `crates/malus-service`: In-process Apple Music service (`AppleService`), official HTTP API client (`api.music.apple.com`), response normalization, token management, product pages generator, and WPE MusicKit session for DRM/playback.
- `crates/malus-wpe`: WPE WebKit process manager, headless/CDP runtime, isolated profile manager, and Widevine CDM supervisor.
- `crates/malusctl`: CLI controller executable (`malusctl`).
- `apps/malus-gtk`: Native GTK4 / Libadwaita / Relm4 frontend (UI frozen during backend milestone).

## Invariants

1. Live playback belongs to MusicKit. Do not optimistically claim that a track is playing because a command was queued. Errors and rejected promises must reach the user.
2. Mock content is only for explicit `--demo` and tests. No fabricated stations, lyrics, listening history, audio-quality badges, or subscription claims.
3. Library, catalog search, charts, and separately loaded playlist resources must not overwrite or contaminate each other. Ignore stale search replies.
4. Use Apple's resource IDs and relationships. Song artist text can contain featured artists; it is not a reliable album identity. Keep library IDs and catalog playback IDs distinct.
5. Entity ID convention: Strictly `<kind>:<id>` (`song:<id>`, `album:<id>`, `artist:<id>`, `playlist:<id>`, `station:<id>`). No `apple:` prefixes on active IDs.
6. Page Routes: Strictly canonical strings `home`, `new`, `radio`, `library:recently-added`, `library:songs`, `library:albums`, `library:artists`, `library:playlists`, `album:<id>`, `artist:<id>`, `playlist:<id>`, `replay:<year>`.
7. Queue mutations must change MusicKit's actual queue. Check the expected item identity before applying an index-based action. MusicKit queue `splice` takes an **array** as its third argument; reorder a contiguous range atomically.
8. Pass dynamic JS values as `call_function` arguments. Never interpolate track IDs, search queries, or user text into executable JavaScript.
9. Bound external I/O. Request cancellation must remove pending CDP entries. Do not block event delivery or MPRIS updates while awaiting a slow playback command or library request.
10. Keep the browser owned by a task with deterministic cleanup. Preserve parent-death signalling and reap killed children. Closing or failing the UI must stop its engine. WPE runtime must stay dormant until playback or auth is requested.
11. Use `ProfileManager` for the isolated profile. Preserve 0700 permissions, reject symlinked profile roots, and refuse logout while that profile is in use. Never print cookies, tokens, passwords, or authentication responses.

## Verification

Use `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --exclude malus-gtk -- -D warnings`, and `cargo test --workspace --exclude malus-gtk` for deterministic checks. Browser/account tests are marked ignored and must be explicitly run with `cargo test --workspace --exclude malus-gtk -- --include-ignored --test-threads=1`, with the normal client closed.
