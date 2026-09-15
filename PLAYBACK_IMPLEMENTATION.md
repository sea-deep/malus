# Playback investigation and implementation record

## Scope and working-tree boundary

User takeover request, 2026-09-15: independently reconstruct current playback, refactor wrong internal models without compatibility shims, verify raw runtime clock and queue effects, complete acceptance matrix. Preserve unrelated UI/account work. User explicitly authorizes live playback and focused multi-client tests. MusicKit remains sole queue authority.

Starting HEAD: `0fb772b` on main. Playback changes are uncommitted alongside a large GTK/account rewrite. Initial tracked diff saved locally in `scratch/playback-takeover/before.patch`.

## MusicKit Boundary

Malus treats MusicKit JS as the authoritative playback API.

Production playback logic uses documented MusicKit operations and state:
- `setQueue(...)`
- `play()`
- `pause()`
- `stop()`
- `seekToTime(...)`
- `changeToMediaItem(...)`
- `changeToMediaAtIndex(...)`
- `skipToNextItem()`
- `skipToPreviousItem()`
- `clearQueue()`
- `playNext(...)`
- `playLater(...)`
- State: `playbackState`, `currentPlaybackTime`, `currentPlaybackDuration`, `nowPlayingItem`, `queue`, `repeatMode`, `shuffleMode`

Malus does not manipulate MusicKit's underlying `HTMLMediaElement`, `MediaSource`, `SourceBuffer`, or private implementation details.

Linux-specific compatibility defects belong below the MusicKit boundary in `crates/malus-wpe` / OpenCDM / WebKit integration.

A MusicKit operation is used only for its intended player semantic. For example, `seekToTime(0)` is valid for an explicit "restart current item" operation (`restart_current_item`), but is not a generic stalled-playback recovery mechanism.

## Historical Root Cause and Provenance

During development, collection and track switching (specifically switching from Album A → Album B → Album A) exhibited freezes at 0:00 (`State: Playing`, progress stationary, GStreamer decryptor stopped feeding samples). Symptoms initially appeared to be MusicKit queue/restart/state issues, prompting multiple high-level experiments.

The root cause was proven from runtime telemetry and source inspection to reside below the MusicKit boundary in the OpenCDM / WebKit integration, involving three distinct issues:

### A. OpenCDM Key-Status Compatibility Contract
- **[SOURCE]**: In `open_cdm.h`, the `callbacks.key_update_callback(session, userData, keyId, length)` carries only the Key ID without a usability status. In WebKit's `CDMProxyThunder.cpp`, `isKeyAvailableUnlocked(keyId)` checks `m_keyStore.containsKeyID(keyId)` only.
- **[RUNTIME]**: When switching away from a collection, MusicKit closed the previous media keys session, causing Widevine to emit `OnSessionKeysChange` with status `cdm::kReleased`. The OpenCDM shim previously forwarded this released key via `key_update_callback`, adding it to WebKit's `m_keyStore`. When returning to that collection, GStreamer checked `isKeyAvailable(keyId)`, WebKit returned `true`, skipped `tryWaitForKeyHandle`, discovered the key was not usable, logged `ERROR webkitthunderdecrypt CDMProxyThunder.cpp:65: Key handle has no usable key`, and permanently aborted decryption.
- **[FIX]**: In `crates/malus-wpe/native/opencdm/src/opencdm_shim.cpp`, `OnSessionKeysChange` notifies WebKit via `callbacks.key_update_callback` ONLY when key status is `Usable` (`cdm::kUsable`). Non-usable keys are never propagated into WebKit's key store, allowing WebKit to wait cleanly for the new license.

### B. OpenCDM Session Lifetime
- **[SOURCE]**: `system->sessions` stores raw pointers to active `OpenCDMSession` instances.
- **[RUNTIME]**: In `opencdm_destruct_session`, deleted session pointers were previously retained in `system->sessions`, allowing subsequent `opencdm_get_system_session` lookups under rapid switching to return dangling pointers.
- **[FIX]**: `opencdm_destruct_session` now synchronizes with `session->system->lock`, unregisters the session pointer from `system->sessions`, and deletes the session safely without use-after-free or resurrecting zero-refcount sessions.

### C. MusicKit Queue Pre-emption
- **[SOURCE]**: MusicKit's Player Audio Framework (PAF) throws `Unhandled Promise Rejection: Error: The play() method was called without a previous stop() or pause() call` if `setQueue(..., startPlaying: true)` is called while actively streaming.
- **[FIX]**: In `crates/malus-service/src/web.rs`, `set_queue_at_index` explicitly pauses active playback before setting a new queue.

## Selection vs Transport Semantics

- **Transport Play (`ClientRequest::Play`)**:
  Resumes paused playback at the current position via `mk.play()`. Never rewinds to 0:00.
- **Content Selection (`ClientRequest::PlayMedia`)**:
  - Selecting an unselected item establishes/replaces the queue.
  - Re-selecting the currently active item rewinds to 0:00 via `mk.seekToTime(0)` (`restart_current_item`) and starts playback.
  - Re-selecting a track within an already-loaded pristine collection jumps cleanly using `mk.changeToMediaAtIndex(idx)`.
- **Content Selection While Paused**:
  Explicitly restarts that item from 0:00 and begins playback.

## Queue Context & Mutation Ordering

- `QueueContext` tracks `{ origin: Option<MediaRef>, pristine: bool }` as context metadata. It never overrides MusicKit's authoritative queue.
- Modifying the queue (Play Next, Play Later, Remove, Move, Clear Upcoming, Shuffle) marks `pristine = false`.
- All playback mutations are serialized through `playback_mutex` in `crates/malusd/src/engine.rs`. Read-only requests (`GetStatus`, `GetQueue`, browsing) remain outside the lock.
- Duplicate IPC routes for `PlayNext` and `PlayLater` were removed; requests route canonically through `PageActionWire::PlayNext` / `PlayLater`.

## Live Acceptance Summary (September 16, 2026)

All 18 live acceptance scenarios verified with monotonic raw clock progression on production daemon:
1. `[PASS]` Song A first play (time > 2s)
2. `[PASS]` Song A -> Song B (track switch, time > 2s)
3. `[PASS]` Song B -> Song A (A -> B -> A DRM switch, time > 2s)
4. `[PASS]` Song A same content Play while Playing (in-place restart to 0, time > 2s)
5. `[PASS]` Song A playing -> Pause (position frozen)
6. `[PASS]` Pause -> Transport Play (resumes paused position, does not restart)
7. `[PASS]` Song A paused -> Same Content Play (restarts from 0, time > 2s)
8. `[PASS]` Album A first Play (Raifal Raja JI)
9. `[PASS]` Album A -> Album B (The Beatles 1, 27 tracks)
10. `[PASS]` Album B -> Album A (Raifal Raja JI switchback regression test)
11. `[PASS]` Album track N -> row N+1 (From Me to You, queue preserved)
12. `[PASS]` Album track N -> same row N (in-place restart to 0, queue preserved)
13. `[PASS]` Queue Next (current_index = 2)
14. `[PASS]` Queue Previous (current_index = 1)
15. `[PASS]` QueueJump (current_index = 5)
16. `[PASS]` Queue Clear Upcoming (truncated ahead of cursor)
17. `[PASS]` SetRepeat Track (Repeat-One natural EOF loop verified)
18. `[PASS]` SetRepeat Off
