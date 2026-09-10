# Malus TUI: Architecture, Wireframes & Viability Blueprint

This document specifies the terminal user interface (TUI) architecture, responsive screen wireframes, artwork strategy, modular input design, and feature viability for **Malus**, powered by **Ratatui** and **ratcn**.

---

## 1. Design Philosophy & Shell Architecture

Malus adheres to a strict, borderless, no-fluff design system inspired by `ratcn`:
1. **Zero Gimmicks**: No decorative visualizers, bouncing ASCII meters, or pixelated half-block noise. Every screen element conveys musical content or state.
2. **Borderless Structural Rules**: Views are separated by clean single-cell rules (`─`) rather than heavy nested box borders. Whitespace, typography, and contrast establish visual hierarchy.
3. **In-Place Status**: Volume percentages, connection states (`● Connected` / `○ Offline`), queue counts, and loading indicators update quietly in-line with zero intrusive toast popups.
4. **Three Horizontal Bands**:
   * **Header (1 row + rule)**: Brand, numbered screen buttons, connection status, search shortcut.
   * **Body (Flexible height)**: Active view or split panel.
   * **Player Bar (2 rows + rule)**: Scrub slider, track metadata, audio traits, and transport controls.

### 1.1. Wide Viewport (≥ 110 Columns)
```text
+----------------------------------------------------------------------------------------------------------+
| malus      [1] Listen Now   [2] Browse   [3] Radio   [4] Library            ● Connected     [/] Search   |
+----------------------+-----------------------------------------------------------------------------------+
| DISCOVERY            | My Library — Songs (215 tracks)                            Sort: [Artist ▾]  [/]  |
|  • Listen Now        |-----------------------------------------------------------------------------------|
|  • Browse            | #   TITLE                        ARTIST             ALBUM                   TIME  |
|  • Radio             | 01  Blinding Lights        [E]   The Weeknd         After Hours            03:20 |
|                      | 02  Starboy                [E]   The Weeknd         Starboy                03:50 |
| LIBRARY              | 03  Save Your Tears              The Weeknd         After Hours            03:35 |
|  • Songs (215)       | 04  Midnight City                M83                Hurry Up, We're ...    04:03 |
|  • Albums            | 05  Instant Crush                Daft Punk          Random Access M...     05:37 |
|  • Artists           | 06  Get Lucky                    Daft Punk          Random Access M...     04:08 |
|  • Playlists         | 07  The Less I Know The Better   Tame Impala        Currents               03:36 |
|                      | 08  Borderline                   Tame Impala        The Slow Rush          03:57 |
| PLAYLISTS            | 09  Lost in Yesterday            Tame Impala        The Slow Rush          04:09 |
|  • Heavy Rotation    | 10  Breathe Deeper               Tame Impala        The Slow Rush          06:12 |
|  • Chill Mix         |                                                                                   |
|  • Favorites Mix     | [j/k] Navigate  [Enter] Play  [Space] Pause  [l] Lyrics  [q] Queue  [r] Radio        |
+----------------------+-----------------------------------------------------------------------------------+
| The Weeknd — Blinding Lights   [E] [AAC 256]                                  01:24 ━━━━━●━━━━━ 03:20    |
| [ ‹ ] [ Play ] [ › ]   [ Shuffle ] [ Repeat ]   [ Autoplay ]        [ 80% ]   [ Lyrics ]   [ Queue 14 ]  |
+----------------------------------------------------------------------------------------------------------+
```

### 1.2. Compact Viewport (80–109 Columns)
* Sidebar collapses into top horizontal navigation tabs (`ratcn::Tabs`).
* Table columns gracefully truncate (`Album` column drops first, preserving `Title`, `Artist`, and `Time`).

### 1.3. Minimal Viewport (< 80 Columns)
* High-density single-column list with inline badges.
* Player bar collapses to compact single-row transport (`[‹] [▶] [›]  01:24/03:20  [80%]`).

---

## 2. Screen-by-Screen Layouts

### 2.1. Screen: Listen Now (Curation & Recommendations)
Clean card containers without decorative ASCII borders:
```text
+-----------------------------------------------------------------------------------+
| LISTEN NOW                                                                        |
|                                                                                   |
| TOP PICKS                                                                         |
|   ┌─────────────────────┐   ┌─────────────────────┐   ┌─────────────────────┐     |
|   │ After Hours         │   │ Currents            │   │ Random Access Mem.  │     |
|   │ The Weeknd          │   │ Tame Impala         │   │ Daft Punk           │     |
|   │ Album · 2020        │   │ Album · 2015        │   │ Album · 2013        │     |
|   └─────────────────────┘   └─────────────────────┘   └─────────────────────┘     |
|                                                                                   |
| MADE FOR YOU                                                                      |
|   [ Favorites Mix · 25 tracks ]   [ Get Up! Mix · Upbeat ]   [ Chill Mix ]        |
|                                                                                   |
| RECENTLY PLAYED                                                                   |
|   1. Starboy (Album) — The Weeknd                  20 mins ago                    |
|   2. The Slow Rush — Tame Impala                   Yesterday                      |
|   3. Discovery Station                             2 days ago                     |
+-----------------------------------------------------------------------------------+
```

---

### 2.2. Screen: Browse & Charts
```text
+-----------------------------------------------------------------------------------+
| BROWSE & CHARTS                                             Storefront: [India ▾] |
|                                                                                   |
| TOP 100 SONGS                                                                     |
|   1. ▶ Espresso                      Sabrina Carpenter       Short n' Sweet 02:55 |
|   2.   BIRDS OF A FEATHER            Billie Eilish           HIT ME HARD... 03:03 |
|   3.   A Bar Song (Tipsy)       [E]  Shaboozey               Where I've ... 02:51 |
|   4.   Not Like Us              [E]  Kendrick Lamar          Not Like Us    04:34 |
|   5.   Good Luck, Babe!              Chappell Roan           Good Luck,...  03:38 |
|                                                                                   |
| GENRES & MOODS                                                                    |
|   [ Pop ]  [ Hip-Hop ]  [ Electronic ]  [ Indie/Alt ]  [ Rock ]  [ R&B ]  [ Chill]|
+-----------------------------------------------------------------------------------+
```

---

### 2.3. Screen: Now Playing & Lyrics
A focused split-pane view pairing artwork with live karaoke lyrics. **No audio visualizers or spectrum bars.**
```text
+-----------------------------------------------------------------------------------+
| NOW PLAYING                                                                       |
+-----------------------------------+-----------------------------------------------+
|                                   | LYRICS                                        |
|   ┌───────────────────────────┐   |                                               |
|   │                           │   | (Past lines - dimmed)                         |
|   │                           │   | I've been on my own for long enough           |
|   │     [ NATIVE ARTWORK ]    │   | You don't have to say too much                |
|   │        Kitty / Sixel      │   |                                               |
|   │             or            │   | ▶ I said, ooh, I'm blinded by the lights      |
|   │     DEFAULT PLACEHOLDER   │   |                                               |
|   │                           │   | (Upcoming lines - standard)                   |
|   │                           │   | No, I can't sleep until I feel your touch     |
|   └───────────────────────────┘   | I said, ooh, I'm drowning in the night        |
|                                   | Oh, when I'm like this, you're the one...     |
|   Blinding Lights             [E] |                                               |
|   The Weeknd                      |                                               |
|   After Hours · 2020 · AAC 256    | [Click line to seek]  [c] Re-center lyrics    |
+-----------------------------------+-----------------------------------------------+
| [ ‹ ] [ Pause ] [ › ]   01:00 ━━━━━━━━━━●━━━━ 03:20    [ 80% ]    [Esc] Back      |
+-----------------------------------------------------------------------------------+
```

---

### 2.4. Screen: Up Next Queue & Autoplay (`∞`)
```text
+-----------------------------------------------------------------------------------+
| UP NEXT                                               Autoplay: [ ON (∞) ]        |
|                                                       Clear Queue: [c]            |
| NOW PLAYING                                                                       |
|   ▶ Blinding Lights             The Weeknd           After Hours            03:20 |
|                                                                                   |
| QUEUED                                                                            |
|   1. In Your Eyes          [E]  The Weeknd           After Hours            03:57 |
|   2. Save Your Tears            The Weeknd           After Hours            03:35 |
|   3. After Hours                The Weeknd           After Hours            06:01 |
|   4. Starboy               [E]  The Weeknd           Starboy                03:50 |
|                                                                                   |
| AUTOPLAY (Continuously generated from current song)                               |
|   ∞  Midnight City              M83                  Hurry Up, We're Dr...  04:03 |
|   ∞  The Less I Know The Better Tame Impala          Currents               03:36 |
|                                                                                   |
| [Alt+▲/▼] Reorder item in queue   [d] Remove   [Enter] Jump   [Esc] Close         |
+-----------------------------------------------------------------------------------+
```

---

## 3. Artwork Architecture: Image Protocol vs. Clean Placeholder

Malus rejects muddy half-block (`▀`) pixelation. Half-block downsampling at terminal character cell sizes yields low-contrast visual noise.

Artwork uses a strict **Two-Tier Engine**:

```
                  ┌───────────────────────────────┐
                  │ Does terminal support Kitty   │
                  │ Graphics Protocol or Sixel?   │
                  └───────────────┬───────────────┘
                                  │
                 ┌────────────────┴────────────────┐
                 ▼ YES                             ▼ NO
  ┌───────────────────────────────┐ ┌───────────────────────────────┐
  │ Tier 1: Native Image Protocol │ │ Tier 2: Typography Placeholder│
  │ • Crisp, full-res raster      │ │ • Crisp structured card       │
  │ • Rendered directly in cell   │ │ • Track & Album metadata      │
  │ • Ghostty, Kitty, WezTerm     │ │ • Format badge: [AAC 256]     │
  │ • Zero pixelation artifacts   │ │ • Zero pixelated muddy blocks │
  └───────────────────────────────┘ └───────────────────────────────┘
```

1. **Tier 1: Native Graphic Protocols**:
   * Query terminal capabilities via device attributes (`CSI ? c` or `$TERM`).
   * For terminals supporting **Kitty Graphics Protocol** or **Sixel** (Ghostty, Kitty, WezTerm, Foot), render high-resolution raster images directly into the designated cell rectangle.
2. **Tier 2: Clean Default Placeholder**:
   * If graphic protocols are unsupported or disabled, render a clean, high-contrast typography placeholder.
   * Consists of an elegant, single-cell bordered card showing the track title, album name, artist, release year, and format badge (`[AAC 256]`).
   * **Never pixelate half-blocks.**

---

## 4. Modular Keymap Architecture

Keybindings must be **fully modular, decoupled, and user-configurable**—never hardcoded in a monolithic `match` statement.

### 4.1. Core Components

1. **`Action` Enum**: Typed representation of all executable user intentions:
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
   pub enum Action {
       // Playback
       TogglePlay,
       NextTrack,
       PrevTrack,
       SeekForward(f64),
       SeekBackward(f64),
       VolumeUp(i8),
       VolumeDown(i8),
       ToggleShuffle,
       CycleRepeat,

       // Navigation
       Navigate(Screen),
       NavigateBack,

       // Overlays
       ToggleLyrics,
       ToggleQueue,
       OpenSearch,
       OpenCommandPalette,
       OpenHelp,
       OpenSettings,
       CloseOverlay,

       // List Operations
       MoveUp,
       MoveDown,
       SelectCurrent,
       ContextMenu,
       ReorderQueueUp,
       ReorderQueueDown,
       DeleteQueueItem,

       // App
       Refresh,
       Quit,
   }
   ```

2. **`KeyChord` & `Keymap`**:
   * `KeyChord`: Represents a key press with modifier bits (`code`, `ctrl`, `alt`, `shift`).
   * `Keymap`: Bidirectional map between `KeyChord` and `Action`.
   * Deserializable from TOML: users can define custom shortcuts in `~/.config/malus/keymap.toml`.

3. **Contextual Scopes**:
   Shortcut routing respects the active UI scope to avoid key collisions:
   * `Scope::Global`: Transport keys (`Space`, media keys, `q`, `l`, `/`).
   * `Scope::TextEntry`: Search bar and command palette; all printable characters go to the input buffer, `Esc` cancels, `Ctrl+U` clears.
   * `Scope::List`: Navigation (`j`/`k`, `Up`/`Down`, `Enter`).
   * `Scope::Queue`: Queue item reordering (`Alt+Up`/`Alt+Down`) and item deletion (`d`).

### 4.2. Default Keymap Reference

| Action | Default Shortcut | Context |
| :--- | :--- | :--- |
| **Play / Pause** | `Space` | Global |
| **Next Track** | `n` | Global |
| **Previous Track** | `p` | Global |
| **Volume Up / Down** | `+` / `-` (or `=` / `-`) | Global |
| **Toggle Shuffle** | `s` | Global |
| **Cycle Repeat** | `r` | Global |
| **Toggle Lyrics** | `l` | Global |
| **Toggle Queue** | `q` | Global |
| **Search** | `/` | Global |
| **Command Palette** | `Ctrl+K` | Global |
| **Screens 1–5** | `1`, `2`, `3`, `4`, `5` | Global |
| **Navigate List** | `j` / `k` or `▲` / `▼` | List |
| **Select / Play** | `Enter` | List |
| **Re-center Lyrics** | `c` | Lyrics Overlay |
| **Reorder Queue** | `Alt+▲` / `Alt+▼` | Queue |
| **Remove from Queue** | `d` or `Delete` | Queue |
| **Back / Dismiss** | `Esc` | Modal / Navigation |
| **Quit** | `Ctrl+C` or `Q` | Global |

---

## 5. Technical Realities & Feature Viability

Malus operates on observable facts and verified runtime evidence. Capabilities are never fabricated.

| Feature | Viability Status | Architectural Reality |
| :--- | :---: | :--- |
| **Catalog & Library Playback** | ✅ **Active** | Uses live headless Chromium CDP driver with authenticated Apple Music session. |
| **Real-Time Synced Lyrics** | ✅ **Active** | Apple Music TTML Timed Text parsed to millisecond timestamps with smooth sub-second interpolated auto-scroll and click-to-seek. |
| **D-Bus MPRIS v2 Control** | ✅ **Active** | Full desktop integration (`playerctl`, Waybar, media keys) via typed object paths. |
| **Zero-Zombie Guarantee** | ✅ **Active** | `PR_SET_PDEATHSIG` kills browser subprocess if parent UI process terminates. |
| **In-Place Status Feedback** | ✅ **Native** | Quiet inline TUI updates instead of disruptive popup notifications. |
| **Native Image Artwork** | ⚡ **Clean Two-Tier** | Kitty Graphics Protocol / Sixel when supported; clean typography placeholder otherwise. **No half-block pixelation.** |
| **Modular Keymap Configuration** | ⚡ **Planned** | Configurable `keymap.toml` replacing hardcoded match blocks. |
| **Infinite Autoplay (`∞`)** | ⚡ **Viable** | Queries `/v1/catalog/{sf}/songs/{id}/similar` when queue is low. |
| **Audio Spectrum Visualizer** | ❌ **DROPPED** | **Eliminated.** MusicKit web player provides no raw PCM stream or low-latency FFT buffers. Avoids fake or high-overhead DOM hacks. |
| **ALAC Lossless / Hi-Res** | ❌ **NON-VIABLE** | **Do Not Implement.** Apple Web Player strictly streams 256 kbps AAC. Lossless web claims are fabricated. |
| **Dolby Atmos / Spatial Audio** | ❌ **NON-VIABLE** | **Do Not Implement.** Web Widevine DRM does not deliver multi-channel spatial streams. |
| **Offline Track Decryption** | ❌ **NON-VIABLE** | **Do Not Implement.** Widevine EME CDM keys are sandboxed; audio cannot be decrypted to disk. |
