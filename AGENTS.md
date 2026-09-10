# Working on Malus

Treat executable code and live behavior as evidence. The previous architecture document included aspirational and inaccurate claims; do not infer capabilities from it.

## Architecture

- `src/main.rs`: terminal lifetime, asynchronous engine startup, reconnect scheduling, artwork requests, CLI operations.
- `src/cli.rs`: side-effect-free option parsing. Parse all arguments before launching a browser or deleting a session.
- `src/controller.rs`: input priority, global shortcuts, mouse routing, and ratcn rendering. This controller is also used by interaction tests.
- `src/app.rs`: reducer, navigation, modal/focus state, library/catalog/resource caches, and engine event handling.
- `src/ui/`: shared list and slider components, screen routing, overlays, header/player bar, and half-block artwork.
- `src/engine/mod.rs`: browser owner, serial playback commands, concurrent catalog/library queries, event relay, MPRIS lifetime, heartbeat, and shutdown.
- `src/engine/musickit.rs` and `bridge.js`: structured MusicKit calls, paginated API reads, metadata parsing, and change-deduplicated snapshots. The bridge reinstalls across page navigation.
- `src/engine/cdp.rs`: request/response correlation, deadlines, cancellation cleanup, events, and disconnection handling.
- `src/engine/process.rs`, `profile.rs`, `discovery.rs`: Chromium discovery, isolated session directory, locks, and process cleanup.
- `src/engine/mpris.rs`: D-Bus controls, typed track object paths, metadata and property-change signals.

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
