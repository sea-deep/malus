# Malus UI overhaul — implementation and handoff

## Authorization and scope

The user authorized autonomous implementation on 2026-09-14, including the previously frozen GTK frontend. Improve the native visual design, interaction reliability, and maintainability; preserve real Apple data, authoritative MusicKit playback, existing features, and repository invariants. Do not request routine design or implementation approval. Never discard unrelated work or expose account secrets.

## Starting point

- Checkout: `/home/dipak/code/malus`.
- The working tree already contains extensive tracked and untracked changes across GTK, IPC, service, daemon, and WPE. These predate this task and must be preserved. Do not reset/revert them or attribute all existing changes to this task.
- Current frontend: GTK4 / Libadwaita / Relm4, with shared player presentation, feed/search/settings/immersive player, utility panes, artwork service, and custom CSS.
- Initial source hotspots: `src/app.rs` ~1,038 lines; `src/pages/feed.rs` ~1,800 lines; search ~489 lines; sidebar ~500 lines; CSS ~584 lines. Size identifies inspection targets, not proof of a defect.
- Old memory describes a retired TUI. Use current source/runtime as evidence; prior live account and playback results are not current verification.

## Design direction

Create a coherent, artwork-led native music app: spacious editorial headers, disciplined typography, warm accent details, layered surfaces, consistent rounded artwork and controls, a quiet sidebar, and a carefully balanced persistent player. Support light/dark appearance, narrow windows, keyboard interaction, clear focus, and honest empty/loading/error states. Use existing real artwork; no fabricated music or feature claims.

## Execution plan

- [x] Inspect repository instructions and working-tree state; create this file before implementation edits.
- [ ] Establish baseline: compile/run current GTK UI; inspect real rendered layouts and key workflows; record concrete defects and baseline checks.
- [ ] Untangle proven hotspots: separate rendering from request/state coordination where useful; centralize repeated presentation logic without unnecessary abstractions.
- [ ] Repair asynchronous UI state: stale replies, cancellation, loading/error/retry behavior, navigation state, artwork lifecycle, reconnect and player interactions as findings warrant.
- [ ] Add missing album/playlist/artist page action rows: Play, Shuffle, Favorite, Add to Library and More where supported; audit service/header action mapping and verify actual playback/queue behavior. User explicitly highlighted the missing header controls in a screenshot during this task.
- [ ] Apply a cohesive visual system across shell, feed, search, cards/rows, player and utility panes, respecting GTK accessibility and native behavior.
- [ ] Verify representative workflows and multiple window sizes in the running app; iterate on observed problems.
- [ ] Run formatting, backend Clippy/tests, GTK checks/tests, and appropriate live checks. Record exact results and limitations.
- [ ] Finish handoff: changed files and rationale, remaining issues, verification evidence, exact resume steps.

## Verification strategy

Required repository checks:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --exclude malus-gtk -- -D warnings
cargo test --workspace --exclude malus-gtk
```

Also compile/lint/test the GTK crate. Use regression tests for meaningful state/race fixes, not cosmetic implementation mirrors. Inspect actual GTK rendering and interactions at desktop and narrow sizes. Run ignored account/browser tests only with the normal client closed, using the documented serialized command where appropriate; do not treat startup, compilation, or prior-run screenshots as current product proof.

## Findings, decisions, and progress

### Confirmed findings

- Baseline `cargo check -p malus-gtk` passed. Inspected the existing running GTK window and its AT-SPI tree. Screenshot: `scratch/ui-overhaul/baseline.png`.
- Search replies matched only query text; page and continuation replies matched only route. Repeating A→B→A could accept an old response. Stale responses could also rebuild the visible view even when data was ignored.
- Search errors were stored but never rendered. Pagination errors only logged and could leave a spinner running. Repeated whitespace input could invalidate a debounce without scheduling its replacement.
- Sidebar rebuilt all playlist widgets on every input, disabled keyboard focus, hardcoded an account name and a private favorite-playlist ID. Search pushed every query edit into history; narrow navigation dismissed the search entry while typing.
- Settings swallowed sign-in/out errors and equated authentication with subscription. Appearance wasn't persisted and startup forced dark despite a system appearance selection.
- CSS changed lyric font size on active-line transitions, forcing relayout. Several interaction colors ignored the selected accent.

### Implemented so far (not yet acceptance-tested)

- Added feed/artist/search request generations and view-level stale-reply guards. Added page/search/continuation error feedback and retry, typed library-artist route identity, route scroll reset, and page-level continuation discovery.
- Separated feed rendering into `pages/feed/rendering.rs`; request coordination remains in `feed.rs`.
- Removed synthetic `song:0` action fallbacks and the unused media-card selection event. Added card keyboard activation and sidebar keyboard focus.
- Retained sidebar playlist buttons between navigation/search updates; removed hardcoded account/playlist identity; coalesced search history; fixed hidden-sidebar search focus and narrow search dismissal.
- Appearance saved to `$XDG_CONFIG_HOME/malus/appearance.json` (fallback `~/.config`). Settings display errors and honest authentication text. Introduced artwork-led typography, rose accent, layered surfaces and stable lyric font geometry.

### Runtime tooling

Native CUA APIs are unavailable. Local GTK verification uses the existing scoped AT-SPI helper and compositor screenshots. This Hyprland uses Lua dispatch objects: execute `hyprctl eval 'hl.dispatch(hl.dsp.focus({ workspace = 7 }))'`; merely evaluating `hl.dsp.focus(...)` does nothing. Never treat a screenshot of Codex as Malus evidence. Capture only the verified Malus window geometry.

### Current next steps

Finish artwork lifecycle fixes; improve hero/compact layout; run rebuild and live acceptance; add meaningful race-regression coverage; run all required checks. Preserve original pre-task changes. A baseline source copy and pre-existing tracked patch are in `scratch/ui-overhaul/` for comparison, not for wholesale restoration.

## Resume instructions

1. Read this file and `AGENTS.md`; inspect current `git status` without discarding changes.
2. Review the findings and latest verification below before repeating work.
3. Complete the first unchecked item, updating this file after meaningful milestones.
4. Preserve pre-existing changes; report blockers precisely and do not claim unperformed verification.

## Verification log

Not yet run.


## User follow-ups and audit reconciliation

- User screenshot: album detail header has no actions. Confirmed root cause: service detail builders omitted `header.actions` even though card mapping already provided them. Added shared detail action mapping, explicit Play/Shuffle RPC and MusicKit queue setup before playback, artist top-song resolution, and header Favorite/Add/More controls. Needs backend restart and live verification.
- User screenshot: `Playlists` heading scrolls away. Keep the heading outside the playlist list's scrolling container. Navigation above it can shrink/scroll independently to support short windows; preserve fixed brand/search and account controls.
- Pasted 21-item audit reviewed against current executable source. Confirmed compact-control, volume endpoint, crossfade lifetime, accessibility-label, lyrics-spacing/manual-scroll, and compact-player-mode defects. The report's optimistic player-favorite handler is absent in current source; track-row optimistic updates were present and removed. Buffering is not represented by the playback model: do not invent a playing/buffering state.
- New memory/artwork changes and memory/stress tests appeared in the shared checkout during this task. Preserve and integrate those current changes; do not overwrite from the initial snapshot. In particular artwork now uses sized/compressed caching and on-demand backdrop generation.
- Implemented audit fixes: compact Lyrics/Queue callbacks; Queue access from immersive view; large artwork in compact Player/no-lyrics mode; compact transport CSS; fixed volume endpoints; accurate Play/Pause/Favorite/Repeat accessible names; WindowHandle for immersive dragging; retain outgoing backdrop until GTK transition completion and clear after browsing transition; synced-only lyrics padding; pause animations on manual input; static lyric-distance classes. Verification pending.

### Verification updates

- Initial required backend deterministic suite passed (before page-action backend changes).
- Initial GTK Clippy passed (before latest changes).
- Added and ran isolated real-GTK IPC race test: same-query and same-route older replies cannot overwrite newer results, whitespace cannot strand debounce, errors show retry, clearing shows initial search state. Passed.
- Current next step: finish compile/fmt after audit patches; test all latest changes; restart updated app/daemon for action-row live verification; check narrow/sidebar/Now Playing screenshots. Record exact final results here.

## 2026-09-15: playback takeover supersedes final UI acceptance

The user supplied a new explicit playback takeover request at `/home/dipak/.codex/attachments/55f3df92-6eab-4ab5-9950-44b31bfa2cd4/pasted-text.txt`. Continue with `PLAYBACK_IMPLEMENTATION.md`. Keep this UI work and its evidence intact.

UI checkpoint: missing header actions, pinned playlist heading, compact Player/Lyrics/Queue, appearance persistence, request generations/retry, and pasted audit fixes are implemented. Album/playlist/artist playback was observed on the September 14 build (13/16/10 items respectively); these checks DO NOT validate subsequent playback changes. Real screenshots exist in `scratch/ui-overhaul/`. The heading stayed at y=479 before/after scrolling to the final playlist. Compact 560x740 and regular 764x822 / 1180x740 layouts were visually checked. Native Adwaita breakpoints removed wide-layout minimum-width feedback. Latest wide-player change sizes artwork around measured controls to keep volume visible; needs final visual confirmation. Minimum supported window is now 560x560. Latest isolated header-action/minimum-size test passed (`layout-actions-test.log`). Earlier GTK race, widget, and memory tests and backend deterministic suite passed; rerun final gates against the playback refactor. Light theme and detailed pointer-drag acceptance remain unperformed. No September 14 Malus test processes survived to this takeover.
