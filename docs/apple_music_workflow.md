# Apple Music Architecture: Stock Workflows, Screens & Capabilities

This document provides a comprehensive, structured reference of the official Apple Music user experience across native desktop (macOS) and web (`music.apple.com` / MusicKit). It maps information architecture, screen states, navigation models, and playback mechanics to serve as a design blueprint for Malus.

---

## 1. Global Information Architecture & Layout Shell

Stock Apple Music follows a strict three-region layout hierarchy:

```
+-------------------------------------------------------------------------------+
| TOP BAR / HEADER                                                              |
| [Back/Fwd]  [Search Input: Catalog | Library]          [User Profile / Cast]  |
+-------------------+-----------------------------------------------------------+
| SIDEBAR (NAV)     | MAIN CONTENT VIEWPORT                                     |
|                   |                                                           |
| Apple Music       | Dynamic screen rendering:                                 |
|  • Listen Now     |  • Home / Listen Now feed                                 |
|  • Browse         |  • Browse & Charts carousels                              |
|  • Radio          |  • Personal Library views (Songs, Albums, Artists)        |
|                   |  • Entity details (Album page, Artist page, Playlist)     |
| Library           |  • Search results                                         |
|  • Recently Added |                                                           |
|  • Artists        |                                                           |
|  • Albums         |                                                           |
|  • Songs          |                                                           |
|  • Playlists      |                                                           |
|                   |                                                           |
| Playlists (User)  |                                                           |
|  • [Playlist 1]   |                                                           |
|  • [Playlist 2]   |                                                           |
+-------------------+-----------------------------------------------------------+
| PERSISTENT PLAYER BAR                                                         |
| [Art] Title · Artist  | [‹] [Play/Pause] [›]  | [Seekbar 01:23 / 03:45]       |
|                       | [Shuffle] [Repeat]    | [Vol: 80%] [Lyrics] [Queue/∞] |
+-------------------------------------------------------------------------------+
```

---

## 2. Core Discovery & Catalog Screens

### 2.1. Listen Now (`/listen-now` or `/home`)
The personalized, algorithmic home screen based on the user's listening history:
* **Top Picks / Up Next:** Heavy-rotation albums and personalized recommendations.
* **Recently Played:** Horizontal carousel of recently streamed albums, playlists, and stations.
* **Made for You:** Apple's curated algorithmic mixes:
  * *Favorites Mix* (updated weekly with tracks you love).
  * *Get Up! Mix* (high-energy upbeat tracks).
  * *Chill Mix* (ambient, downtempo favorites).
  * *New Music Mix* (fresh releases tailored to your taste).
* **Stations for You:** Continuous personalized radio streams (e.g. *[User]'s Station*, *Discovery Station*).
* **Friend Activity / Social Listening:** Friends' recently shared tracks and playlists.
* **Apple Music Replay:** Annual breakdown of top songs, artists, and listening minutes.

### 2.2. Browse (`/browse`)
The editorial, global showcase:
* **Hero Carousel:** Major album releases, exclusive interviews, and cultural moments.
* **Top Charts:**
  * *Daily Top 100: Global*
  * *Daily Top 100: City Charts* (e.g. Tokyo, London, New York).
  * *Genre Charts* (Hip-Hop, Electronic, Rock, R&B, Pop, Indie).
* **New Music / Just Updated:** Fresh Friday album and single drops.
* **Browse by Category:** Grid cards for Genres, Decades, Moods & Activities (Chill, Focus, Workout, Party, Sleep, Romance).
* **Music Videos:** Curated video premieres and video playlists.
* **Record Labels:** Deep-dive directory into specific record labels.

### 2.3. Radio (`/radio`)
Broadcast radio and on-demand artist audio:
* **Apple Music Live Broadcasts:**
  * *Apple Music 1* (flagship live global broadcast).
  * *Apple Music Hits* (popular music from the 80s, 90s, and 2000s).
  * *Apple Music Country*.
* **On-Demand Radio Shows:** Archives of host-driven and artist-hosted shows (e.g. Elton John's Rocket Hour, The Zane Lowe Show).
* **Genre & Decades Radio:** Automated stations based on broad genres and eras.

---

## 3. Personal Library Screens

### 3.1. Recently Added
* Reverse-chronological grid/card view of newly added albums and saved playlists.
* Focuses on physical album art and quick access to newest additions.

### 3.2. Songs (High-Density Grid/Table)
The definitive power-user table view:
* Multi-column sortable table:
  * `Title` (with Explicit `[E]` badge if applicable)
  * `Artist` (linked to artist profile)
  * `Album` (linked to album view)
  * `Time` (track duration formatted as `mm:ss`)
  * `Favorite` (Heart `♥` toggle)
  * `Date Added`
  * `Play Count` / `Rating`
* Inline actions: Play on double-click/Enter, Right-click context menu (Play Next, Play Later, Add to Playlist, Love/Dislike, Show Album, Show Artist).

### 3.3. Albums
* Alphabetical or chronological card grid of all albums in user library.
* Sub-metadata on card: Release Year, Artist Name, Total Track Count.

### 3.4. Artists
* Split-pane or list view:
  * Left sidebar list: Alphabetical artist roster.
  * Right main viewport: All library tracks and albums matching the selected artist.

### 3.5. Playlists
* User-created playlists, followed Apple editorial playlists, and folder hierarchies.
* Custom playlist covers, descriptions, and manual track reordering.

---

## 4. Entity Detail Pages

### 4.1. Album Detail Page
* **Hero Banner:**
  * High-resolution Album Cover Art.
  * Album Title, Artist Name (clickable), Genre, Release Year.
  * Quality badges: `[E]` (Explicit), `[MASTER]` (Apple Digital Master), `[256K AAC]`.
  * Actions: `Play`, `Shuffle`, `Add to Library (+ / ✓)`, `Favorite (♥)`, `More Options (···)`.
  * Editorial Review / Liner Notes snippet.
* **Track Listing:**
  * Track index number / disc grouping (Disc 1, Disc 2).
  * Title, duration, featured artists.
  * Inline popularity indicator (small star indicating top streaming singles on the album).

### 4.2. Artist Profile Page
* **Header:** Hero backdrop photography and artist avatar.
* **Top Songs:** 5 to 10 most popular songs by global stream volume.
* **Latest Release:** Banner highlighting the latest album or EP.
* **Discography Tabs/Carousels:**
  * *Albums* (studio releases).
  * *Singles & EPs*.
  * *Live Albums*.
  * *Compilations*.
  * *Appears On* (features, collaborative tracks).
* **Artist Station CTA:** Button to launch instant infinite radio based on the artist.
* **Similar Artists:** Recommended related artist cards.

### 4.3. Playlist Detail Page
* **Header:** Cover collage or editorial artwork, Playlist Title, Curator attribution, Description, Last Updated date, Track count, Total running time.
* **Tracklist Table:** Index, Title, Artist, Album, Date Added, Duration.

---

## 5. Persistent Player Bar & Playback Workflows

### 5.1. The Player Bar Controls
Always pinned and visible across all page navigation:
1. **Current Item Identity:** Thumbnail artwork, Track Title, Artist, Album link, Favorite toggle (`♥`).
2. **Transport Controls:** Previous Track (`‹`), Play/Pause (`▶` / `⏸`), Next Track (`›`).
3. **Timeline Scrubber:** Elapsed time, interactive seek slider, total/remaining duration.
4. **Queue Modes:**
   * **Shuffle (`🔀`):** Toggles randomized queue execution.
   * **Repeat (`🔁`):** Cycles `Off` → `Repeat All` → `Repeat One (🔂)`.
5. **Volume Controller:** Level slider, Mute toggle (`🔊` / `🔇`).
6. **Drawer Toggles:**
   * **Lyrics (`💬`):** Opens synchronized lyrics.
   * **Up Next / Queue (`☰`):** Opens queue manager with Autoplay toggle.

---

## 6. Up Next, Queue Management & Autoplay

### 6.1. The "Playing Next" Drawer
* Displays:
  1. **Now Playing:** Currently active song with progress bar.
  2. **Up Next (Queue):** List of upcoming songs.
  3. **History:** Chronological list of tracks played earlier in the current session.
* Actions:
  * Reorder songs (drag & drop or keyboard move up/down).
  * Remove specific song from queue.
  * Clear entire queue.
  * "Play Next" (inserts item at position `current + 1`).
  * "Play Later" (appends item to the very end of the queue).

### 6.2. The Infinite Autoplay (`∞`) Engine
* **The Infinity Button (`∞`):** Located at the top of the queue drawer.
* **Behavior:**
  * When **Disabled**: Playback halts when the last queued song finishes.
  * When **Enabled**: When the queue exhausts its final song, the client automatically requests song-similarity recommendations from the Apple Music API (`/v1/catalog/{storefront}/songs/{id}/similar` or auto-generated station queue) and appends recommended tracks seamlessly. Playback never stops.

---

## 7. Synchronized Lyrics Workflow

* **Time-Coded Lyrics (`.lrc` format):**
  * Lyrics data is served with millisecond timestamps per line/word.
* **Presentation:**
  * Live vocal synchronization: current vocal line is highlighted and visually enlarged.
  * Prior lines dim and recede.
  * Upcoming lines sit in secondary text.
  * Viewport automatically and smoothly scrolls to keep the active line centered.
* **Interactive Seeking:** Clicking any lyric line immediately seeks track playback to that timestamp.

---

## 8. Search Architecture

* **Dual-Scope Search Input:**
  * Toggle between `Apple Music (Global Catalog)` and `Your Library`.
* **Search Landing (Before Typing):**
  * Recent searches.
  * Trending searches / Top search terms.
  * Browse category tiles.
* **Live Search Results (Debounced):**
  * **Top Result:** Prominent hero card showing the highest-confidence match (can be an Artist, Album, or Song).
  * **Categorized Sections:**
    * Songs (with instant play button).
    * Artists.
    * Albums.
    * Playlists.
    * Radio Stations.

---

## 9. Context Menus & Micro-Interactions

Every track, album, and playlist in stock Apple Music supports a standard context menu (`···` or right-click):
* `Play Next`
* `Play Later`
* `Add to Library` / `Delete from Library`
* `Add to Playlist → [List of User Playlists | New Playlist]`
* `Create Station` (instant algorithmic radio)
* `Favorite (Love)` / `Suggest Less`
* `Go to Album`
* `Go to Artist`
* `Share / Copy Link`

---

## 10. Summary Matrix for Malus Alignment

| Feature / Screen | Stock Web / Native | Malus Implementation Status | Next Enhancement Opportunity |
| :--- | :--- | :--- | :--- |
| **Listen Now / Home** | Full algorithmic grid | Initial screen router | Integrate personalized Mixes & Top Picks |
| **Browse / Charts** | Carousels & Top 100 | Initial catalog queries | Add Top 100 Global & City chart lists |
| **Radio** | Apple Music 1 & Genre stations | Station model supported | Add 1-key "Create Station" (`r`) |
| **Library Songs** | High-density sortable table | In-memory library cache | Add column sorting (`Title`, `Artist`, `Album`) |
| **Queue / Up Next** | Reorderable list | MusicKit queue splice supported | Add live queue reorder UI & History |
| **Autoplay (`∞`)** | Seamless similar track stream | Manual queue handling | **Auto-fetch similar tracks on queue end** |
| **Lyrics** | Synchronized smooth scroll | Static lyrics drawer | **Millisecond synced scrolling karaoke** |
| **Player Bar** | Transport, seek, volume | Full player bar with sliders | **Nerd Font icons & genuine badges (`[E]`, `[MASTER]`)** |
| **Search** | Dual-scope live search | Live catalog search | Add "Your Library" filter toggle |
