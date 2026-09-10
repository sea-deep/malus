# Malus TUI: Architecture, Wireframes & Viability Blueprint

This document translates stock Apple Music screens and workflows into a clean, high-performance terminal user interface (TUI) optimized for **Ratatui** and **ratcn**.

---

## 1. Global Terminal Shell & Responsive Layout

Malus adapts to terminal dimensions using three responsive layout profiles:

### 1.1. Wide Viewport (≥ 110 Columns) — The Three-Region Studio Shell
```text
+----------------------------------------------------------------------------------------------------------+
| 󰎆 malus    [1] Listen Now   [2] Browse   [3] Radio   [4] Library            ● Connected   󰍉 [/] Search   |
+----------------------+-----------------------------------------------------------------------------------+
| 󰎈 DISCOVERY          | 󰀥 My Library — Songs (215 tracks)                         Sort: [Artist ▾]  Filter: [/] |
|  • 󰋋 Listen Now      |-----------------------------------------------------------------------------------|
|  • 󰲸 Browse          | #   TITLE                        ARTIST             ALBUM                󰋑   TIME  |
|  • 󰐹 Radio           | 01  Blinding Lights        [E]   The Weeknd         After Hours          ♥   03:20 |
|                      | 02  Starboy                [E]   The Weeknd         Starboy              ♥   03:50 |
| 󰀥 LIBRARY            | 03  Save Your Tears              The Weeknd         After Hours              03:35 |
|  • 󰎆 Songs (215)     | 04  Midnight City                M83                Hurry Up, We're ...      04:03 |
|  • 󰀥 Albums          | 05  Instant Crush                Daft Punk          Random Access M...   ♥   05:37 |
|  • 󰠃 Artists         | 06  Get Lucky                    Daft Punk          Random Access M...       04:08 |
|  • 󰄲 Playlists       | 07  The Less I Know The Better   Tame Impala        Currents             ♥   03:36 |
|                      | 08  Borderline                   Tame Impala        The Slow Rush            03:57 |
| 󰲸 PLAYLISTS          | 09  Lost in Yesterday            Tame Impala        The Slow Rush            04:09 |
|  • Heavy Rotation    | 10  Breathe Deeper               Tame Impala        The Slow Rush            06:12 |
|  • Chill Mix         |                                                                                   |
|  • Favorites Mix     | [▲/▼] Navigate  [Enter] Play  [Space] Pause  [m] Lyrics  [q] Queue  [r] Radio        |
+----------------------+-----------------------------------------------------------------------------------+
| 󰐊 The Weeknd — Blinding Lights   [E] [MASTER] [AAC 256]                       01:24 ━━━━━●━━━━━ 03:20    |
| [ ‹ ] [ 󰐊 Play ] [ › ]   [󰒝 Shuffle] [󰑖 Repeat]   [∞ Autoplay]      [󰕾 80%]   [󰔑 Lyrics]  [󰄲 Queue 14]  |
+----------------------------------------------------------------------------------------------------------+
```

### 1.2. Compact Viewport (80–109 Columns)
* Sidebar collapses into top horizontal navigation tabs (`tabs-basic` / `tabs-large` from `ratcn`).
* Table columns gracefully truncate non-critical metadata (`Album` collapses first, keeping `Title`, `Artist`, `Time`).

### 1.3. Minimal Viewport (< 80 Columns)
* High-density single-column list with badges.
* Transport bar condenses to essential glyphs (`[‹] [󰐊] [›]  01:24/03:20  [󰕾 80%]`).

---

## 2. Screen-by-Screen TUI Translations

### 2.1. Screen: Listen Now / Home (Algorithmic Curation)
Uses card containers and horizontal scroll rows:
```text
+-----------------------------------------------------------------------------------+
| LISTEN NOW                                                                        |
|                                                                                   |
| ▸ TOP PICKS                                                                       |
|   ┌─────────────────────┐   ┌─────────────────────┐   ┌─────────────────────┐     |
|   │ ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄ │   │ ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄ │   │ ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄ │     |
|   │ After Hours         │   │ Currents            │   │ Random Access Mem.  │     |
|   │ The Weeknd          │   │ Tame Impala         │   │ Daft Punk           │     |
|   └─────────────────────┘   └─────────────────────┘   └─────────────────────┘     |
|                                                                                   |
| ▸ MADE FOR YOU (Updated Weekly)                                                   |
|   [Favorites Mix · 25 tracks]   [Get Up! Mix · Upbeat]   [Chill Mix · Ambient]    |
|                                                                                   |
| ▸ RECENTLY PLAYED                                                                 |
|   1. Starboy (Album) — The Weeknd                  20 mins ago                    |
|   2. The Slow Rush — Tame Impala                   Yesterday                      |
|   3. Discovery Station                             2 days ago                     |
+-----------------------------------------------------------------------------------+
```

---

### 2.2. Screen: Browse & Top Charts
```text
+-----------------------------------------------------------------------------------+
| BROWSE & CHARTS                                     Category: [All ▾]             |
|                                                                                   |
| ▸ DAILY TOP 100                                                                   |
|   1. 󰐊 Espresso                      Sabrina Carpenter       Short n' Sweet 02:55 |
|   2.   BIRDS OF A FEATHER            Billie Eilish           HIT ME HARD... 03:03 |
|   3.   A Bar Song (Tipsy)       [E]  Shaboozey               Where I've ... 02:51 |
|   4.   Not Like Us              [E]  Kendrick Lamar          Not Like Us    04:34 |
|   5.   Good Luck, Babe!              Chappell Roan           Good Luck,...  03:38 |
|                                                                                   |
| ▸ GENRES & MOODS                                                                  |
|   [ Pop ]  [ Hip-Hop ]  [ Electronic ]  [ Indie/Alt ]  [ Rock ]  [ R&B ]  [ Chill]|
+-----------------------------------------------------------------------------------+
```

---

### 2.3. Screen: Now Playing & Live Synchronized Karaoke Lyrics
Split-pane rendering (`panels` component) combining artwork, spectrum bars, and live scrolling lyrics:
```text
+-----------------------------------------------------------------------------------+
| NOW PLAYING                                                                       |
+-----------------------------------+-----------------------------------------------+
|                                   | SYNCHRONIZED LYRICS                           |
|       ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄       |                                               |
|       █                   █       | (Past line - dimmed)                          |
|       █    [ALBUM ART]    █       | I've been on my own for long enough           |
|       █    Half-Block     █       | You don't have to say too much                |
|       █    or Kitty/Sixel █       |                                               |
|       █                   █       | >>> ACTIVE SUNG LINE (Glows in Accent) <<<    |
|       ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀       | >> I said, ooh, I'm blinded by the lights <<  |
|                                   |                                               |
|  Blinding Lights             [E]  | (Upcoming lines - normal)                     |
|  The Weeknd · After Hours (2020)  | No, I can't sleep until I feel your touch     |
|  [MASTER] [AAC 256 kbps]          | I said, ooh, I'm drowning in the night        |
|                                   | Oh, when I'm like this, you're the one I trust|
|  SPECTRUM VISUALIZER              |                                               |
|   ▂▃▅▆█▆▅▃▂ ▂▃▄▅▆▇█▇▆▅▄▃▂ ▂▃▄▅    |                                               |
+-----------------------------------+-----------------------------------------------+
| [ ‹ ] [ 󰏤 Pause ] [ › ]   01:45 ━━━━━━━━━━●━━━━ 03:20    [󰕾 80%]   [Esc] Back     |
+-----------------------------------------------------------------------------------+
```

---

### 2.4. Screen: Up Next Queue with Infinite Autoplay (`∞`)
```text
+-----------------------------------------------------------------------------------+
| QUEUE & PLAYING NEXT                                  Autoplay: [󰒝 ON (∞)]        |
|                                                       Clear Queue: [c]            |
| NOW PLAYING                                                                       |
|   ▶ Blinding Lights             The Weeknd           After Hours            03:20 |
|                                                                                   |
| UP NEXT (Will play next)                                                          |
|   1. In Your Eyes          [E]  The Weeknd           After Hours            03:57 |
|   2. Save Your Tears            The Weeknd           After Hours            03:35 |
|   3. After Hours                The Weeknd           After Hours            06:01 |
|   4. Starboy               [E]  The Weeknd           Starboy                03:50 |
|                                                                                   |
| AUTOPLAY RECOMMENDATIONS (Continuous ∞ Similar Music)                             |
|   ∞  Midnight City              M83                  Hurry Up, We're Dr...  04:03 |
|   ∞  The Less I Know The Better Tame Impala          Currents               03:36 |
|   ∞  Instant Crush              Daft Punk            Random Access Mem.     05:37 |
|                                                                                   |
| [J/K] Move item in queue   [d] Remove item   [Enter] Jump to track   [Esc] Close  |
+-----------------------------------------------------------------------------------+
```

---

## 3. Feature Viability Analysis

| Feature | Viability Status | Implementation Path in Malus |
| :--- | :---: | :--- |
| **Catalog & Library Playback** | ✅ **Active** | Uses existing MusicKit JS queue and DRM playback pipeline. |
| **Volume & Seek Sliders** | ✅ **Active** | Native `ratcn` sliders mapped directly to MusicKit `seek()` & `volume()`. |
| **D-Bus MPRIS v2 Desktop Control** | ✅ **Active** | Linux media keys, Waybar, and `playerctl` integration in `src/engine/mpris.rs`. |
| **Zero-Zombie Process Guarantee** | ✅ **Active** | Linux `PR_SET_PDEATHSIG` kills browser if Malus closes. |
| **Clamped Headless Engine** | ✅ **Active** | 0.3% GPU CPU, 128MB V8 heap, images disabled in DOM. |
| **Nerd Font Auto-Provisioning** | ⚡ **Easy Tweak** | Check Fontconfig; if missing, auto-fetch `SymbolsNerdFont-Regular.ttf` to `~/.local/share/fonts` or fallback to Unicode. |
| **Infinite Autoplay (`∞`)** | ⚡ **Easy Tweak** | When `queue.len() <= 1`, query Apple Music API `/v1/catalog/{sf}/songs/{id}/similar` and append tracks via `mk.playLater()`. |
| **Real-Time Synced Lyrics** | ⚡ **Medium Tweak** | Fetch time-coded `.lrc` from Apple Music API; auto-scroll Ratatui list based on `currentPlaybackTime`. |
| **Authentic Quality Badges** | ⚡ **Easy Tweak** | Parse `attributes.contentRating == "explicit"` (`[E]`) and `attributes.audioTraits` (`[MASTER]`, `[AAC 256]`). |
| **In-Place Status Feedback** | ✅ **Native** | Quiet inline UI updates (instant volume slider value, queue counter badge, status bar text) instead of intrusive popups. |
| **ASCII Spectrum Visualizer** | ⚡ **Medium Tweak** | Integrate `demos/barchart` from cloned `ratcn` driven by WebAudio `AnalyserNode` frequency bins. |
| **Waybar / Status Line Flag** | ⚡ **Easy Tweak** | Add `malus status --json` or `--waybar` output for instant Hyprland/Polybar integration. |
| **ALAC Lossless 24-bit/192kHz** | ❌ **NON-VIABLE** | **Do Not Implement.** Apple Web Player/MusicKit JS strictly streams 256 kbps AAC. Claims of Lossless on web are fabricated. |
| **Dolby Atmos / Spatial Audio** | ❌ **NON-VIABLE** | **Do Not Implement.** Web Widevine DRM does not receive Spatial multi-channel streams. |
| **Offline Track Decryption / Rip** | ❌ **NON-VIABLE** | **Do Not Implement.** Widevine EME keys are hardware/sandboxed; audio cannot be decrypted to disk. |

---

## 4. Keybinding & Keyboard Navigation Hierarchy

Designed for high-speed, muscle-memory operation:

| Key | Action | Context |
| :--- | :--- | :--- |
| `Space` | Toggle Play / Pause | Global |
| `n` / `›` | Skip to Next Track | Global |
| `p` / `‹` | Skip to Previous Track | Global |
| `+` / `-` | Volume Up / Down (5%) | Global |
| `s` | Toggle Shuffle | Global |
| `r` | Cycle Repeat (`Off` → `All` → `One`) | Global |
| `R` (Shift+R) | **Start Song Radio** (seed station from current song) | Global |
| `m` | Toggle Synchronized Lyrics Screen | Global |
| `q` | Toggle Up Next Queue Drawer | Global |
| `a` | Toggle Autoplay (`∞`) | Inside Queue |
| `j` / `k` / `▲` / `▼` | Navigate List / Table rows | Active view |
| `Enter` | Play highlighted song / Open Album or Playlist | Active view |
| `J` / `K` (Shift) | Reorder highlighted song Up / Down in Queue | Inside Queue |
| `d` | Remove highlighted song from Queue | Inside Queue |
| `/` | Open Search Bar (Type query + Enter) | Global |
| `Ctrl+C` / `Esc` | Cancel Search / Close Overlay / Exit | Global |
