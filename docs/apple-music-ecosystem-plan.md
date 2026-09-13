# Apple Music Ecosystem Architecture and Implementation Plan for Malus

**Status:** Canonical Architecture Specification (Final Corrected Draft)  
**Author:** Antigravity  
**Date:** September 2026  
**Scope:** Research, Live Gateway Probing, Code Audit, and Architecture Specification  
**Target File:** `docs/apple-music-ecosystem-plan.md`  

---

## 1. Executive Summary

Malus is a native Linux audio player designed around an isolated WebKit/WPE runtime supporting Widevine CDM audio decryption. While milestone M1b stabilized audio playback and process supervision, it left the client/provider protocol severely constrained: frontends were limited to single-track playback, flat catalog search, and basic library listings.

The true Apple Music consumer product is fundamentally **feed-driven, editorial, and interactive**:
- **Discovery**: Home (Listen Now) recommendation shelves, Browse / New charts and editorial releases, Radio live broadcasts (Apple Music 1) and continuous algorithmic stations, and detailed Artist Discographies.
- **Engagement**: Explicit **Favorites** (`POST /v1/me/favorites`), **Ratings / Suggest Less** (`PUT /v1/me/ratings`), **Library membership**, **Recently Played / Heavy Rotation history**, **Annual Replay / Music Summaries**, **Time-synced lyrics**, and **Song credits**.
- **Playback & Queue**: Continuous playback of containers (albums, playlists, stations) and interactive queue control (reordering, jumping, clear upcoming).

### Architectural Foundations Established

1. **Three Explicit Apple Access Classes**:
   - **Class A: Official Apple Music API (`api.music.apple.com`)**: Documented, stable JSON:API endpoints for catalog, charts, search, library collections, recommendations, history, favorites, ratings, and music summaries. Preferred for all standard metadata.
   - **Class B: Apple First-Party Web API (`amp-api.music.apple.com`)**: Private Apple web service surface. Quarantined strictly for capabilities absent from public documentation (unfavoriting via delete query, line/syllable TTML lyrics, and song credits). Classified as **Private / Moderate or Fragile**.
   - **Class C: MusicKit JS / WPE Runtime**: Authoritative playback engine, Widevine DRM decrypter, and playback queue owner. Required for session bootstrapping, DRM media streaming, and authoritative queue mutations.
2. **Direct Native HTTP Performance & Gateway Binding**:
   - Native Rust HTTP requests (`reqwest`) using harvested tokens (`devToken` + `userToken`) reach Apple's gateways with **~250ms–450ms latency** (vs ~600ms–1400ms over browser IPC).
   - Apple's developer token contains `{ "root_https_origin": ["apple.com"] }`. Gateway requests require `Origin: https://music.apple.com` and `Referer: https://music.apple.com/`; requests omitting them receive HTTP 401.
3. **Accurate Transport Architecture**:
   - Malus's default WPE backend does **not** use CDP. It communicates via `malus-web-runtime` using `malus-wpe-host` over bidirectional standard I/O JSON IPC. (CDP WebSocket is used only when falling back to external Chromium).
4. **Authoritative Queue Ownership**:
   - **MusicKit owns the playback queue**. The provider observes and normalizes MusicKit's active queue; `malus-daemon` maintains a lightweight mirror; and clients render the mirror. Client queue actions travel downward (`Client → Daemon → Provider → MusicKit`).
5. **Truthful Domain Semantics**:
   - **Favorites** (`/v1/me/favorites`), **Ratings / Suggest Less** (`/v1/me/ratings`), and **Library Membership** (`/v1/me/library`) are distinct concepts and are not collapsed.
   - **History** (`RecentlyPlayed`, `HeavyRotation`) is strictly separated from **Library Collections** (`Songs`, `Albums`, `Artists`, `Playlists`, `RecentlyAdded`).
   - **Replay / Music Summaries** is an official Class A API (`GET /v1/me/music-summaries?filter[year]=latest&views=...`) returning annual top songs, artists, and albums.
   - Audio quality badges must distinguish catalog tags from active stream delivery (Widevine AAC-LC 256kbps in WPE).

---

## 2. Apple Access Stability Classification

Every Apple-facing operation in Malus is categorized into one of three explicit tiers:

```text
┌────────────────────────────────────────────────────────────────────────┐
│ CLASS A: OFFICIAL APPLE MUSIC API                                      │
│ Host: https://api.music.apple.com/v1/...                               │
│ Stability: STABLE / OFFICIAL                                           │
│ Scope: Catalog, Search, Charts, Library Collections, Recommendations,  │
│        History, Favorites, Ratings/Suggest Less, Music Summaries,      │
│        Station links                                                   │
└────────────────────────────────────┬───────────────────────────────────┘
                                     │ (Preferred for all standard metadata)
┌────────────────────────────────────▼───────────────────────────────────┐
│ CLASS B: APPLE FIRST-PARTY WEB API                                     │
│ Host: https://amp-api.music.apple.com/v1/...                           │
│ Stability: PRIVATE / MODERATE OR FRAGILE (Quarantined)                 │
│ Scope: Unfavorite (DELETE), Line TTML lyrics, Word TTML lyrics         │
│        (Karaoke), Song credits                                         │
└────────────────────────────────────┬───────────────────────────────────┘
                                     │ (Used ONLY when no Class A exists)
┌────────────────────────────────────▼───────────────────────────────────┐
│ CLASS C: MUSICKIT JS / WPE RUNTIME                                     │
│ Host: WPE WebProcess via malus-wpe-host IPC                            │
│ Stability: RUNTIME / INTERNAL                                          │
│ Scope: Auth bootstrap, Token refresh, Widevine DRM decryption,          │
│        Audio playback, Authoritative queue state & mutations           │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Current Malus Apple Provider & Runtime Audit

### 3.1. Process Hierarchy & IPC Transport
```text
malus-daemon
   └── providers/apple (stdio IPC via malus-provider-sdk)
          ├── Native HTTP Client (Proposed for Class A & B metadata)
          └── malus-web-runtime
                 └── malus-wpe-host (stdio JSON IPC)
                        └── WPE WebKit (WebProcess + libocdm + Widevine CDM)
```
- **WPE Transport Fact**: In `crates/malus-web-runtime/src/wpe.rs`, communication with `malus-wpe-host` is conducted over standard I/O pipes using line-delimited JSON commands (`call`, `eval`). CDP WebSockets are **not** used by WPE; CDP exists in `crates/malus-web-runtime/src/cdp.rs` purely as a secondary fallback when running standard Chromium.
- **Session Credentials**:
  `AppleHarvestedTokens` are cached at `~/.local/share/malus/profiles/apple/tokens.json` (`0600` permissions):
  - `devToken`: Bearer JWT (`iss: "AMPWebPlay"`, `root_https_origin: ["apple.com"]`).
  - `userToken`: Music User Token string (`media-user-token`).
  - `storefront`: Active two-letter country code (e.g. `us`, `in`, `gb`).

### 3.2. Current Provider Limitations
- **Queue Playback**: `providers/apple/src/web.rs:1133-1150` only handles `{ song: id }` or `{ songs: [id] }`. It does not support album queues, playlist queues, station streams, or queue mutations (reordering, jumping, clearing upcoming).
- **Discovery**: Exposes zero feeds, recommendations, history, or radio stations.
- **IPC Overhead**: All existing metadata queries (`search`, `get_catalog_item`, `get_library`) are bottlenecked on `malus-wpe-host` JavaScript evaluation, incurring unnecessary IPC serialization and JS heap overhead.

---

## 4. Legacy TUI Apple Integration Audit

The legacy TUI (`frontends/tui-legacy/`) proved that rich Apple Music features work reliably in Malus. The following table audits every legacy capability:

| Feature | Legacy Location | Endpoint / Call | Access Class | Feasibility & Notes |
| :--- | :--- | :--- | :---: | :--- |
| **Catalog Charts** | `engine/mod.rs:228` | `GET /v1/catalog/{sf}/charts?types=songs&limit=50` | **Class A** | Official; trivial to port to native HTTP. |
| **Radio Stations** | `engine/mod.rs:239` | `GET /v1/catalog/{sf}/search?term=Apple+Music&types=stations` | **Class A** | Official catalog station search. |
| **Recommendations** | `engine/mod.rs:269` | `GET /v1/me/recommendations?limit=10` | **Class A** | Official recommendation shelves. |
| **Recently Played** | `engine/mod.rs:318` | `GET /v1/me/recent/played/tracks?limit=30` | **Class A** | Official recent track history. |
| **Time-Synced Lyrics**| `engine/musickit.rs:304`<br>`model/lyrics.rs:1` | `GET /v1/catalog/{sf}/songs/{id}/lyrics` | **Class B** | Requires `amp-api`; parsed by `parse_ttml`. |
| **Album Queueing** | `engine/musickit.rs:188` | `mk.setQueue({ album: id })` | **Class C** | Native MusicKit JS container descriptor. |
| **Playlist Queueing**| `engine/musickit.rs:189` | `mk.setQueue({ playlist: id })` | **Class C** | Native MusicKit JS container descriptor. |
| **Station Queueing** | `engine/musickit.rs:190` | `mk.setQueue({ station: id })` | **Class C** | Continuous live/algorithmic stream. |
| **Play Next / Later** | `engine/musickit.rs:191` | `mk.playNext({song})` / `mk.playLater({song})` | **Class C** | Relative queue insertion. |
| **Jump Queue Index** | `engine/musickit.rs:192` | `mk.changeToMediaAtIndex(index)` | **Class C** | Direct queue playhead shift. |
| **Atomic Reorder** | `engine/musickit.rs:195` | `mk.queue.splice(start, len, items)` | **Class C** | Atomic sub-array splice. |
| **Clear Upcoming** | `engine/musickit.rs:194` | `mk.queue.clearAfterCurrent()` | **Class C** | Clears future queue items. |
| **Live Snapshot** | `engine/bridge.js:1` | `malusDispatch` binding snapshot | **Class C** | Continuous stream/queue state relay. |

---

## 5. Complete Apple Music Ecosystem Capability Matrix

The following matrix covers the complete consumer Apple Music surface. Every entry identifies verified sources, access classes, stability, and recommended Malus mechanisms:

| Feature / Capability | Consumer Surface | Apple Web Support | Legacy TUI | Current Malus | Official API (Class A) | Web API (Class B) | MusicKit (Class C) | Recommended Malus Mechanism | Stability Tier | Priority |
| :--- | :--- | :---: | :---: | :---: | :---: | :---: | :---: | :--- | :---: | :---: |
| **Single Track Playback** | Player | Yes | Yes | Yes | No | No | Yes | Class C (MusicKit `setQueue`) | Runtime | P0 |
| **Album Playback** | Player | Yes | Yes | No | No | No | Yes | Class C (MusicKit `setQueue`) | Runtime | P0 |
| **Playlist Playback** | Player | Yes | Yes | No | No | No | Yes | Class C (MusicKit `setQueue`) | Runtime | P0 |
| **Station Playback** | Player | Yes | Yes | No | No | No | Yes | Class C (MusicKit `setQueue`) | Runtime | P0 |
| **Play Next / Later** | Queue | Yes | Yes | No | No | No | Yes | Class C (`playNext` / `playLater`) | Runtime | P0 |
| **Queue Reorder / Splice** | Queue | Yes | Yes | No | No | No | Yes | Class C (`queue.splice`) | Runtime | P1 |
| **Queue Jump Index** | Queue | Yes | Yes | No | No | No | Yes | Class C (`changeToMediaAtIndex`) | Runtime | P1 |
| **Clear Upcoming Queue** | Queue | Yes | Yes | No | No | No | Yes | Class C (`clearAfterCurrent`) | Runtime | P1 |
| **Queue Mirror Relay** | Queue | Yes | Yes | Partial | No | No | Yes | Class C (JS binding snapshot) | Runtime | P0 |
| **Home Recommendations** | Home | Yes | Partial | No | Yes | Yes | Partial | Class A (`GET /v1/me/recommendations`) | Official | P0 |
| **Recently Played** | History | Yes | Yes | No | Yes | Yes | No | Class A (`GET /v1/me/recent/played`) | Official | P0 |
| **Recently Played Tracks**| History | Yes | Yes | No | Yes | Yes | No | Class A (`GET /v1/me/recent/played/tracks`) | Official | P1 |
| **Recently Played Stations**| Radio | Yes | No | No | Yes | Yes | No | Class A (`GET /v1/me/recent/radio-stations`) | Official | P1 |
| **Heavy Rotation** | History | Yes | No | No | Yes | Yes | No | Class A (`GET /v1/me/history/heavy-rotation`) | Official | P1 |
| **Top Charts (Songs/Albums)**| Browse | Yes | Yes | No | Yes | Yes | Partial | Class A (`GET /v1/catalog/{sf}/charts`) | Official | P0 |
| **Genre Charts** | Browse | Yes | No | No | Yes | Yes | No | Class A (`charts?genre={id}`) | Official | P2 |
| **Apple Music 1 / Hits** | Radio | Yes | Yes | No | Yes | Yes | Yes | Class A search + Class C play | Official/Runtime | P0 |
| **Station from Song** | Radio | Yes | No | No | Yes | Yes | Yes | Class A (`/songs/{id}/station`) | Official | P1 |
| **Station from Artist** | Radio | Yes | No | No | Yes | Yes | Yes | Class A (`/artists/{id}/station`) | Official | P1 |
| **Station Genres** | Radio | Yes | No | No | Yes | Yes | No | Class A (`/catalog/{sf}/station-genres`) | Official | P2 |
| **Catalog Search** | Search | Yes | Yes | Yes | Yes | Yes | Partial | Class A (`GET /v1/catalog/{sf}/search`) | Official | P0 |
| **Library Search** | Search | Yes | No | No | Yes | Yes | No | Class A (`GET /v1/me/library/search`) | Official | P1 |
| **Search Suggestions/Hints**| Search | Yes | No | No | Yes | Yes | No | Class A (`/search/hints`) | Official | P1 |
| **Recent Search History** | Search | Yes | No | No | No | No | No | **Local Client State** (404 on API) | Local UI | P2 |
| **Natural-Language Search**| Search | No | No | No | No | No | No | **Unsupported** (No API surface) | Unknown | - |
| **Library Songs** | Library | Yes | Yes | Yes | Yes | Yes | Partial | Class A (`GET /v1/me/library/songs`) | Official | P0 |
| **Library Albums** | Library | Yes | No | Yes | Yes | Yes | Partial | Class A (`GET /v1/me/library/albums`) | Official | P0 |
| **Library Artists** | Library | Yes | No | No | Yes | Yes | No | Class A (`GET /v1/me/library/artists`) | Official | P1 |
| **Library Playlists** | Library | Yes | Yes | Yes | Yes | Yes | Partial | Class A (`GET /v1/me/library/playlists`) | Official | P0 |
| **Recently Added (Library)**| Library | Yes | No | No | Yes | Yes | No | Class A (`GET /v1/me/library/recently-added`) | Official | P0 |
| **Favorite Item (Song/Album)**| Mutation | Yes | No | No | Yes | Yes | Partial | Class A (`POST /v1/me/favorites?ids[...]`)| Official | P1 |
| **Unfavorite Item** | Mutation | Yes | No | No | No | Yes | No | Class B (`DELETE amp-api /me/favorites`)| Private/Fragile| P1 |
| **Rate / Suggest Less** | Mutation | Yes | No | No | Yes | Yes | Partial | Class A (`PUT /v1/me/ratings/...` val: -1)| Official | P2 |
| **Clear Rating** | Mutation | Yes | No | No | Yes | Yes | No | Class A (`DELETE /v1/me/ratings/...`) | Official | P2 |
| **Favorite Songs Playlist**| Library | Yes | No | No | Yes | Yes | Yes | Class A (Smart library playlist) | Official | P1 |
| **Add to Library** | Mutation | Yes | No | No | Yes | Yes | Partial | Class A (`POST /v1/me/library?ids[...]`) | Official | P1 |
| **Remove from Library** | Mutation | Yes | No | No | No | Partial | No | **Deferred** (401 on api; 500 on amp-api)| Uncertain | P3 |
| **Create Playlist** | Mutation | Yes | No | No | Yes | Yes | Partial | Class A (`POST /v1/me/library/playlists`) | Official | P1 |
| **Edit Playlist Metadata** | Mutation | Yes | No | No | No | Partial | No | **Deferred** (401 on api; requires body)| Uncertain | P3 |
| **Add Track to Playlist** | Mutation | Yes | No | No | Yes | Yes | Partial | Class A (`POST .../playlists/{id}/tracks`)| Official | P1 |
| **Remove Track from Playlist**| Mutation| Yes | No | No | No | Partial | No | **Deferred** (401 on api; mode required)| Uncertain | P3 |
| **Delete Playlist** | Mutation | Yes | No | No | No | Partial | No | **Deferred** (401 on api) | Uncertain | P3 |
| **Pins (Persistence)** | UI State | Yes | No | No | No | No | No | **Persistence UNKNOWN** (404 on API) | Unknown | - |
| **Artist View (Full)** | Detail | Yes | No | Partial | Yes | Yes | No | Class A (`/artists/{id}?views=...`) | Official | P0 |
| **Album Detail (Full)** | Detail | Yes | No | Yes | Yes | Yes | Partial | Class A (`/albums/{id}?include=...`) | Official | P0 |
| **Playlist Detail (Full)** | Detail | Yes | Yes | Yes | Yes | Yes | Partial | Class A (`/playlists/{id}` or library) | Official | P0 |
| **Line-Synced Lyrics** | Experience | Yes | Yes | No | No | Yes | No | Class B (`amp-api ... /lyrics`) | Private/Fragile| P1 |
| **Word-Synced Lyrics** | Experience | Yes | No | No | No | Yes | No | Class B (`amp-api ... /syllable-lyrics`)| Private/Fragile| P2 |
| **Song Credits** | Experience | Yes | No | No | No | Yes | No | Class B (`amp-api ... /credits`) | Private/Fragile| P1 |
| **Music Summaries / Replay**| Experience | Yes | No | No | Yes | Yes | No | Class A (`/me/music-summaries?filter=latest`)| Official | P1 |
| **Curators / Record Labels**| Detail | Yes | No | No | Yes | Yes | No | Class A (`apple-curators`, `record-labels`)| Official | P2 |
| **Music Videos** | Catalog | Yes | No | No | Yes | Yes | No | Class A (`catalog/{sf}/music-videos`) | Official | P3 |
| **Active Stream Badges** | UI State | No | No | No | No | No | Partial | **Deferred** (Active WPE stream is AAC 256k)| Runtime | - |

---

## 6. Deep Dives into Core Product Areas

### 6.1. Home ("Listen Now")
- **Source**: Class A (`GET https://api.music.apple.com/v1/me/recommendations?limit=10`)
- **Structure**: Grouped sections containing heterogeneous resources (`playlist`, `album`, `station`).
- **Shelves Observed**:
  1. *Playlists Made for You*: Algorithmic mixes ("Favorites Mix", "Get Up! Mix", "Chill Mix").
  2. *Recently Played*: Chronological album/playlist/station history.
  3. *Stations for You*: Personal discovery stations ("User's Station", "Discovery Station").
  4. *Artist Spotlights*: "More from [Artist]" recommendations.
  5. *New Releases for You*: Targeted new album releases.
  6. *Curated Thematic Shelves*: Genre hubs and mood playlists.
- **Pagination**: Returns `next` continuation cursor when additional recommendation groups exist. Initial fetch yields a rich starting page (~10 sections).

### 6.2. Browse & New Releases
- **Source**: Class A (`GET https://api.music.apple.com/v1/catalog/{storefront}/charts?types=songs,albums,playlists&limit=20`)
- **Structure**: Ranked lists of top tracks, top albums, and editorial playlists.
- **Editorial Groupings**: Class A editorial groupings (`GET /v1/editorial/{sf}/groupings?platform=web&name=browse`) provide curated banner features.

### 6.3. Radio Hub
- **Live Broadcasts**: Class A catalog search (`/v1/catalog/{sf}/search?term=Apple+Music+1&types=stations`) returns flagship stations (`ra.978194965`). Attributes include `isLive: true`, `stationProviderName: "Apple Music"`, and editorial tagline.
- **Recently Listened Stations**: Class A (`GET /v1/me/recent/radio-stations?limit=10`).
- **Dynamic Station Generation**: Class A relationship endpoints:
  - Song Station: `GET /v1/catalog/{sf}/songs/{id}/station`
  - Artist Station: `GET /v1/catalog/{sf}/artists/{id}/station` (e.g. "Radiohead & Similar Artists Station").
- **Playback**: Initiated via MusicKit JS `mk.setQueue({ station: id })`. Live streams report continuous playback without seek capabilities.

### 6.4. Search & Discovery
- **Catalog Search**: Class A (`/v1/catalog/{sf}/search?term={q}&types=songs,albums,artists,playlists,stations`).
- **Library Search**: Class A (`/v1/me/library/search?term={q}&types=library-songs,library-albums,library-playlists`).
- **Search Suggestions / Hints**: Class A (`/v1/catalog/{sf}/search/hints?term={q}`). Returns real-time completion strings in `results.terms`.
- **Search History**: Empirical probing of `/v1/me/history/searches` returns HTTP 404 on both public and private gateways. Search history is strictly **frontend-local client state** (saved in client configuration/cache). It must not be part of the provider wire protocol.
- **Natural-Language Search**: Neither documented nor present in Apple API. Search is tokenized keyword-based.

### 6.5. Library Collections vs. History
- **Strict Semantic Separation**:
  - **Library Collections (`LibraryKindWire`)**:
    - `Songs` (`/v1/me/library/songs`)
    - `Albums` (`/v1/me/library/albums`)
    - `Artists` (`/v1/me/library/artists`)
    - `Playlists` (`/v1/me/library/playlists`)
    - `RecentlyAdded` (`/v1/me/library/recently-added`)
  - **History & Discoveries (`HistoryKindWire`)**:
    - `RecentlyPlayed` (`/v1/me/recent/played`)
    - `RecentlyPlayedTracks` (`/v1/me/recent/played/tracks`)
    - `RecentlyPlayedStations` (`/v1/me/recent/radio-stations`)
    - `HeavyRotation` (`/v1/me/history/heavy-rotation`)

### 6.6. Favorites, Ratings & Library Mutations
- **Favorites vs. Ratings Separation**:
  - **Favorite (New Apple Favorites API)**:
    - Add to Favorites: `POST https://api.music.apple.com/v1/me/favorites?ids[songs]={id}` (Class A Official). Empirical probe returned **HTTP 202 Accepted**.
    - Remove from Favorites (Unfavorite): `DELETE https://amp-api.music.apple.com/v1/me/favorites?ids[songs]={id}` (Class B Private). Empirical probe on `api.music` failed with 400 "Insufficient Permissions", but on `amp-api` succeeded with **HTTP 204 No Content**.
    - **Favorite Songs Playlist**: Apple automatically populates a smart playlist named `"Favourite Songs"` (or `"Favorite Songs"`). Live probing confirmed this playlist exists in `/v1/me/library/playlists` with `hasPlayParams: true`.
  - **Ratings / Suggest Less (Legacy Star/Thumbs System)**:
    - Positive Rating: `PUT /v1/me/ratings/{songs|albums|playlists}/{id}` with `{ "type": "rating", "attributes": { "value": 1 } }` (Class A).
    - Dislike / Suggest Less: `PUT /v1/me/ratings/{songs|albums|playlists}/{id}` with `{ "type": "rating", "attributes": { "value": -1 } }` (Class A).
    - Clear Rating: `DELETE /v1/me/ratings/{songs|albums|playlists}/{id}` (Class A).
  - **Library Membership**:
    - Add to Library: `POST /v1/me/library?ids[songs]={id}&ids[albums]={id}` (Class A).
    - Favorite status, Rating value, and Library presence are distinct concepts and must not be collapsed.

### 6.7. Destructive & Edit Mutations Re-verification
The following table details the exact current status of edit and delete operations under live testing:

| Operation | Current Apple Doc Name / URL | HTTP Method & Path | Live Probe Result (`api.music`) | Live Probe Result (`amp-api`) | Current Stability Classification | Recommended Action |
| :--- | :--- | :--- | :---: | :---: | :---: | :--- |
| **Remove from Library** | Delete a Resource from Library | `DELETE /v1/me/library/songs/{id}` | **HTTP 401 Unauthorized** | **HTTP 500** ("Unable to delete") | **Uncertain / Private Candidate** | **Defer RPC encoding**; needs further web payload audit. |
| **Edit Playlist Metadata** | Change Library Playlist Metadata | `PATCH /v1/me/library/playlists/{id}` | **HTTP 401 Unauthorized** | **HTTP 400** ("Invalid playlist data") | **Uncertain / Private Candidate** | **Defer RPC encoding**; requires full playlist schema. |
| **Delete Playlist** | Delete a Library Playlist | `DELETE /v1/me/library/playlists/{id}` | **HTTP 401 Unauthorized** | Status not safely probed | **Uncertain / Private Candidate** | **Defer RPC encoding**. |
| **Remove Track from Playlist** | Delete Tracks from Playlist | `DELETE .../playlists/{id}/tracks` | **HTTP 401 Unauthorized** | **HTTP 400** ("No mode supplied") | **Uncertain / Private Candidate** | **Defer RPC encoding**; `mode` parameter required. |
| **Reorder Playlist Tracks** | Undocumented in MusicKit Web API | Unverified | **HTTP 401 Unauthorized** | Unverified | **Unsupported / Unknown** | **Do NOT implement**. |

*Finding*: Public gateway `api.music.apple.com` rejects destructive mutations (HTTP 401) under Web Play tokens. `amp-api` recognizes them but enforces private parameters (e.g. `mode`). Therefore, **destructive playlist mutations must NOT be encoded into the canonical protocol yet**.

### 6.8. Pins Persistence
- **Empirical Evidence**: Probing `/v1/me/pins` returns HTTP 404 on both public and private gateways.
- **Finding**: A 404 endpoint does not prove local storage. Whether Apple syncs pinned items through CloudKit, private Store APIs, or local application preferences is **UNKNOWN**.
- **Architecture**: Malus leaves the persistence mechanism marked **UNKNOWN**. We will not design a premature local pinning database until desktop requirements justify it.

### 6.9. Replay / Music Summaries (Class A Verified)
- **Documented API**: `GET https://api.music.apple.com/v1/me/music-summaries?filter[year]=latest&views=top-artists,top-albums,top-songs`
- **Empirical Result**: **HTTP 200 OK**!
  - `year: "2026"`
  - `top-songs`: 99 song period summaries with catalog song relationships (`href: "/v1/catalog/in/songs/..."`).
  - `top-artists`: 15 artist period summaries with catalog artist relationships.
  - `top-albums`: 15 album period summaries with catalog album relationships.
- **Architecture**: Malus will expose Music Summaries as an official Class A feature (`GetMusicSummary { year: Option<u32> }`), providing true analytical listening data alongside "Replay YYYY" library playlists.

### 6.10. Lyrics & Credits (Class B Private Surface)
- **Line-Synced Lyrics**: `GET https://amp-api.music.apple.com/v1/catalog/{sf}/songs/{id}/lyrics`.
  - Returns TTML XML with `itunes:timing="Line"`.
  - Line timestamps: `<p begin="00:15.20" end="00:19.45">Karma police, arrest this man</p>`.
- **Word-Synced Lyrics (Karaoke / Sing Mode)**: `GET https://amp-api.music.apple.com/v1/catalog/{sf}/songs/{id}/syllable-lyrics`.
  - Returns TTML XML with `itunes:timing="Word"`.
  - Word spans: `<span begin="00:15.20" end="00:15.80">Karma </span><span begin="00:15.80" end="00:16.40">police</span>`.
- **Song Credits**: `GET https://amp-api.music.apple.com/v1/catalog/{sf}/songs/{id}/credits`.
  - Returns categorized role categories: Performers (Vocals, Guitars, Drums), Writers (Lyrics, Music), Production & Engineering (Producers, Mix Engineers).
- **Gateway Quarantining**: Both endpoints fail with HTTP 400 on `api.music.apple.com` due to missing developer scopes. They must be routed through `amp-api.music.apple.com` with `Origin: https://music.apple.com`.

### 6.11. Audio Quality Badges: Catalog vs. Active Playback Format
- **The Discrepancy**:
  - Catalog metadata exposes: `audioTraits: ["lossless", "hi-res-lossless", "spatial"]` and `isAppleDigitalMaster: true`.
  - **Actual Playback Delivery in WPE**: WebKit running in Linux decodes Widevine-protected **AAC-LC 256 kbps stereo** (or AAC-ELD for live radio). Apple does **not** deliver ALAC (Lossless) or Dolby Atmos bitstreams to browser clients.
- **Strict Rule**:
  - Malus will **not** display deceptive "Lossless" or "Dolby Atmos" playback badges during active playback.
  - Catalog metadata badges (e.g. "Available in Lossless on Apple Devices") may be displayed as static informational tags on album detail pages, but the player bar will only reflect what the active stream authoritatively delivers.

---

## 7. Authoritative Playback & Queue Architecture

The playback queue is owned strictly by MusicKit in WPE. The daemon does not maintain an independent, competing queue engine.

> **Architecture Note:** As part of the Apple-First transition (archiving the multi-provider framework at git tag `provider-framework-final`), `AppleService` is hosted directly in-process inside `malus-daemon`.

```mermaid
sequenceDiagram
    participant GUI as Frontend (malus-gui-next)
    participant Daemon as malus-daemon
    participant Apple as malus-apple (In-Process)
    participant WPE as MusicKit JS (WPE Runtime)

    Note over WPE: MusicKit owns active playback & queue
    WPE->>Apple: malusDispatch snapshot (Queue, NowPlaying, Status)
    Apple->>Daemon: StatusChanged(PlayerStatusWire)
    Daemon->>GUI: ClientEvent::StatusChanged(PlayerStatusWire)
    
    Note over GUI: User reorders queue item (e.g. move index 3 to 1)
    GUI->>Daemon: ClientRequest::QueueAction(QueueActionWire::Move { from: 3, to: 1 })
    Daemon->>Apple: apple.queue_action(QueueActionWire::Move { from: 3, to: 1 })
    Apple->>WPE: mk.queue.splice(start, count, items)
    Note over WPE: Queue mutated atomically
    WPE->>Apple: malusDispatch updated snapshot
    Apple->>Daemon: StatusChanged(PlayerStatusWire)
    Daemon->>GUI: ClientEvent::StatusChanged(PlayerStatusWire)
```

### Queue Operations:
- `PlayMedia { media_id }`: Triggers `mk.setQueue({ songs: [id] })`, `{ album: id }`, `{ playlist: id }`, or `{ station: id }`.
- `EnqueueNext { media_id }`: Calls `mk.playNext(...)`.
- `EnqueueLater { media_id }`: Calls `mk.playLater(...)`.
- `Jump { index }`: Calls `mk.changeToMediaAtIndex(index)`.
- `Remove { index }`: Calls `mk.queue.splice(index, 1)`.
- `Move { from, to }`: Executes atomic sub-array reorder via `mk.queue.splice(...)`.
- `ClearUpcoming`: Calls `mk.queue.clearAfterCurrent()`.

---

## 8. Provider-Authored Surface Architecture

The fundamental architectural principle of Malus is:
```text
PROVIDERS OWN PRODUCT SEMANTICS
MALUS PROTOCOL CARRIES STRUCTURED SURFACES
FRONTENDS OWN NATIVE PRESENTATION
```

Rather than attempting to invent universal enums for every music service's product taxonomy (e.g. `HeavyRotation`, `Replay`, `MadeForYou`, `RadioPage`, `BrowsePage`), Malus treats providers as the authors of their own consumer product navigation, feeds, and item hierarchies.

- A provider defines its own navigation structure (e.g. Apple exposes *Home*, *New*, *Radio*, *Library*; a local provider exposes *Folders*, *Artists*, *Albums*).
- A provider normalizes its pages into structural surfaces composed of headers, sections, items, and actions.
- Clients treat surface identifiers, entity references, and action tokens as opaque data.
- The frontend maps structural presentation hints (`shelf`, `grid`, `track-list`) into native GTK widgets with complete visual polish and responsiveness.

### 8.1. Provider Surface Manifest
Providers advertise their navigation hierarchy via a manifest:

```rust
/// Provider-authored navigation manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSurfaceManifestWire {
    pub provider_id: String,
    pub default_surface_id: String,
    pub groups: Vec<SurfaceNavGroupWire>,
}

/// A grouping of navigation entries (e.g. "Discover", "Library").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceNavGroupWire {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub entries: Vec<SurfaceNavEntryWire>,
}

/// A specific navigation entry opening an opaque surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceNavEntryWire {
    pub surface_id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_hint: Option<String>,
}
```

### 8.2. Generic Surface Transport Model
A surface is an opaque container representing any top-level feed, collection, or detail page:

```rust
/// A structured, provider-authored surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceWire {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<SurfaceHeaderWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<SurfaceActionWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<SurfaceSectionWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<SurfaceCursorWire>,
}

/// Optional rich header for album, artist, playlist, or summary surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceHeaderWire {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork: Option<ArtworkWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<SurfaceBadgeWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<SurfaceActionWire>,
}

/// An ordered section within a surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceSectionWire {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub items: Vec<SurfaceItemWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continuation: Option<SurfaceCursorWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation_hint: Option<String>, // Advisory hint: "shelf", "grid", "track-list", "hero"
}

/// A normalized display-oriented surface item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceItemWire {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tertiary_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork: Option<ArtworkWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<ProviderEntityRefWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_surface_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub badges: Vec<SurfaceBadgeWire>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<SurfaceActionWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation_hint: Option<String>,
}

/// Opaque provider entity reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProviderEntityRefWire {
    pub provider_id: String,
    pub id: String,
    pub kind: String, // Extensible provider kind: "song", "album", "station", "apple-curator", "folder"
}

/// Provider-authored badge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceBadgeWire {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_hint: Option<String>, // e.g. "explicit", "live", "accent"
}
```

### 8.3. Provider Actions & Continuation
Actions and continuations allow interactive workflows without embedding provider logic in frontends:

```rust
/// A contextual or standalone provider action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceActionWire {
    pub invocation_token: String,
    pub label: String,
    pub role: ActionRoleWire,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_hint: Option<String>,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<ActionStateWire>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionRoleWire {
    Primary,
    Secondary,
    Toggle,
    Destructive,
    Context,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionStateWire {
    Inactive,
    Active,
    Mixed,
}

/// Result of an action invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceActionResultWire {
    pub status: ActionStatusWire,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default)]
    pub refresh: SurfaceRefreshWire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActionStatusWire {
    Success,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "surfaces", rename_all = "kebab-case")]
pub enum SurfaceRefreshWire {
    #[default]
    None,
    CurrentSurface,
    SpecificSurfaces(Vec<String>),
}

/// Opaque pagination cursor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceCursorWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_id: Option<String>,
    pub token: String,
}

/// Result of continuing a surface or section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SurfaceContinuationWire {
    Section {
        section_id: String,
        items: Vec<SurfaceItemWire>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        continuation: Option<SurfaceCursorWire>,
    },
    Sections {
        sections: Vec<SurfaceSectionWire>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        continuation: Option<SurfaceCursorWire>,
    },
}
```

---

## 9. Provider Internal Architecture & Fallback Policy (Phase 2 Completed)

### 9.1. Final API Module Structure
Inside `providers/apple/src/api/`, the native HTTP engine is organized into clean, focused submodules:

```text
providers/apple/src/api/
├── mod.rs           # Module root, re-exports of OfficialAppleMusicApi, credentials, and errors
├── credentials.rs   # AppleCredentials struct, TokenProvider trait, ProfileTokenProvider, StaticTokenProvider
├── error.rs         # AppleApiError enum and conversion into ProviderError
├── parse.rs         # Canonical Apple JSON parsers (artwork, track, album, artist, playlist)
└── official.rs      # OfficialAppleMusicApi: reqwest client, path resolution, header construction, retries
```

- **`TokenProvider` Trait**: Decouples credential acquisition and refresh from HTTP transport. `ProfileTokenProvider` loads credentials from disk cache and delegates refresh to `AppleWebSession::refresh_tokens`. `StaticTokenProvider` allows deterministic unit testing.
- **`OfficialAppleMusicApi`**: Owns `reqwest::Client` (with 10-second request timeouts and custom base URL override support for testing).
- **Mandatory Headers**: All requests construct:
  - `Authorization: Bearer <developer_token>`
  - `Music-User-Token: <music_user_token>`
  - `Origin: https://music.apple.com`
  - `Referer: https://music.apple.com/`
  - `User-Agent: Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15`
- **Storefront Path Resolution**: Resolves `{storefront}` placeholders in endpoint paths using the user's active storefront (e.g. `/v1/catalog/{storefront}/search` -> `/v1/catalog/in/search`).

### 9.2. Credential Ownership & Secret Isolation
- **Ownership**: The Apple provider is the sole owner of Apple credentials. Developer and user tokens are never leaked to `malus-daemon` or frontend clients.
- **Disk Storage**: Cached at `~/.local/share/malus/profiles/apple/tokens.json` with strict POSIX `0600` permissions.
- **Redaction**: `AppleCredentials` implements custom `fmt::Debug` that prints `developer_token: "[REDACTED]"` and `music_user_token: "[REDACTED]"` to prevent token exposure in traces or log files.

### 9.3. HTTP Error Translation Policy
`AppleApiError` maps cleanly into `ProviderError`:
- **HTTP 401 Unauthorized**: Handled automatically in `OfficialAppleMusicApi::send_request`. On 401, the API performs **exactly one** token refresh via `TokenProvider::refresh_credentials()`. If the retried request succeeds, the flow continues transparently. If refresh fails or the retried request returns 401, it translates to `ProviderError::AuthRequired`.
- **HTTP 403 Forbidden**: Translates to `ProviderError::Forbidden`.
- **HTTP 404 Not Found**: Translates to `ProviderError::NotFound`. Authoritative from Apple; never falls back to WPE.
- **HTTP 429 Too Many Requests**: Extracts `Retry-After` header into `ProviderError::RateLimited { retry_after }`.
- **HTTP 5xx Server Error**: Translates to `ProviderError::Server`.
- **Transport / Timeout / Connection Error**: Retried up to 2 times with exponential backoff (200ms, 400ms). If all retries fail, translates to `ProviderError::Network`.
- **JSON Deserialization Error**: Translates to `ProviderError::Internal`.

### 9.4. Operations Migrated to Native HTTP
All metadata operations in `AppleProvider` are migrated to `OfficialAppleMusicApi`:
1. `search(query, kinds, limit, cursor)`: `GET /v1/catalog/{storefront}/search`
2. `get_catalog_item(media_id)`: `GET /v1/catalog/{storefront}/{type}s/{id}`
3. `get_collection_items(media_id, limit, cursor)`: `GET /v1/catalog/{storefront}/{type}s/{id}/tracks`
4. `get_library(kind, limit, cursor)`: `GET /v1/me/library/{kind}`

### 9.5. Complete Removal of WPE Metadata Paths
- All metadata methods (`search`, `get_catalog_item`, `get_collection_items`, `get_library`, `call_musickit_api`, `get_storefront_id`) and duplicate parsing logic have been completely deleted from `AppleWebSession` and `ProductionAppleWebSession`.
- `AppleWebSession` now only handles playback lifecycle (`play`, `pause`, `resume`, `stop`, `seek`, `set_volume`, `get_status`) and authentication (`ensure_authenticated`, `get_auth_state`, `refresh_tokens`, `logout`).
- Metadata requests never touch, launch, or wake the WPE runtime.

### 9.6. Measured Latency Improvements (Live Parity Benchmark)
Live benchmarks against real Apple Music production endpoints demonstrated significant performance gains:

| Operation | Native HTTP (`reqwest`) | Legacy WPE JS (`eval`) | Latency Reduction |
| :--- | :---: | :---: | :---: |
| Search ("Daft Punk") | **1098 ms** | 2026 ms | **~46% faster (1.8x)** |
| Catalog Track Lookup | **297 ms** | 1475 ms | **~80% faster (5.0x)** |
| Catalog Album Lookup | **378 ms** | 663 ms | **~43% faster (1.8x)** |
| Catalog Artist Lookup | **305 ms** | 653 ms | **~53% faster (2.1x)** |
| Catalog Playlist Lookup | **373 ms** | 643 ms | **~42% faster (1.7x)** |
| Library Tracks (Songs) | **621 ms** | 904 ms | **~31% faster (1.5x)** |
| Library Albums | **787 ms** | 949 ms | **~17% faster (1.2x)** |
| Library Playlists | **592 ms** | 874 ms | **~32% faster (1.5x)** |

All response payloads verified **100% field-for-field parity** with canonical wire models.

---

## 10. Revised Implementation Roadmap

The revised roadmap reflects the Provider-Authored Surface Architecture:

```text
Phase 1: Provider Surface Foundation (Generic surface protocol, manifest, mock proof) [COMPLETE]
   ↓
Phase 2: Apple Official HTTP Engine (api.music.apple.com client in providers/apple) [COMPLETE]
   ↓
Phase 3: Apple Surface Implementation (Home, New, Radio, Library, Details via surfaces) [NEXT]
   ↓
Phase 4: MusicKit Container Playback + Authoritative Queue (WPE bridge)
   ↓
Phase 5: Apple Private Enhancements (Lyrics & Credits via amp-api)
   ↓
Phase 6: Provider Actions (Favorites, Ratings, Library mutations)
   ↓
Phase 7: Greenfield Native GUI (apps/malus-gui-next consuming generic surfaces)
```

### Phase 1: Provider Surface Foundation (COMPLETED)
- Generic surface wire models in `malus-protocol`.
- RPC methods (`GetProviderSurfaceManifest`, `GetSurface`, `ContinueSurface`, `InvokeSurfaceAction`).
- Provider SDK surface hooks and server dispatch.
- Surface routing in `malus-daemon`.
- Typed client methods in `malus-client`.
- Comprehensive mock provider surfaces and tests.

### Phase 2: Apple Official HTTP Engine (COMPLETED)
- Implemented `OfficialAppleMusicApi` in `providers/apple/src/api/` with `reqwest`.
- Extracted canonical Apple parsers into `providers/apple/src/api/parse.rs`.
- Built `TokenProvider` abstraction and integrated single-retry on 401.
- Migrated all metadata operations (`search`, `get_catalog_item`, `get_collection_items`, `get_library`).
- Completely removed old metadata methods from `AppleWebSession` and `ProductionAppleWebSession`.
- Validated with in-memory mock HTTP server tests, live acceptance suite, and latency benchmarks.

### Phase 3: Apple Surface Implementation (NEXT PHASE)
- Implement `get_surface_manifest` and `get_surface` in `providers/apple`:
  - Home surface (mapping `recommendations` into shelves).
  - New surface (mapping `charts` into top songs/albums).
  - Radio surface (mapping live stations and recent stations).
  - Library surfaces (mapping `library/songs`, `albums`, `artists`, `playlists`, `recently-added`).
  - Detail surfaces (album, artist, playlist detail surfaces with rich headers).

### Phase 4: MusicKit Container Playback + Authoritative Queue
- Update `malus-wpe-host` bridge to support container queueing (`album`, `playlist`, `station`).
- Implement atomic queue splice (`mk.queue.splice`), jump, and clear upcoming.
- Wire authoritative queue change broadcasts from WPE to provider and daemon mirror.

### Phase 5: Apple Private Enhancements
- Implement `AppleWebApi` targeting `amp-api.music.apple.com`.
- Parse line-synced and syllable TTML lyrics.
- Parse structured song credits.

### Phase 6: Provider Actions
- Implement `InvokeSurfaceAction` in `providers/apple` for zero-input actions:
  - Favorite (`POST /v1/me/favorites`) & Unfavorite (`DELETE` on `amp-api`).
  - Suggest Less (`PUT /v1/me/ratings`).
  - Add to Library (`POST /v1/me/library`).

### Phase 7: Greenfield Native GUI (`apps/malus-gui-next`)
- Build responsive 4px-grid GUI consuming normalized `SurfaceWire` data, rendering native shelves, carousels, lists, and playback controls.

---

## 11. Open Questions & Final Decisions

1. **Feed Pagination Behavior**:
   - `GET /v1/me/recommendations` returns a `next` cursor when additional groups exist.
   - *Design Decision*: Surface returns an initial batch (~10 sections); continuation tokens allow lazy streaming of subsequent sections.
2. **Action Input Extensibility**:
   - Phase 1 handles zero-input actions.
   - *Design Decision*: Action model uses opaque `invocation_token`. Parameterized actions (like naming a playlist) will extend the action payload in Phase 6 without breaking the Phase 1 surface structure.
3. **Lyrics Word-by-Word Timing**:
   - *Design Decision*: Return TTML-derived timed lyrics in Phase 5 with line timestamps as base and word timestamps as optional enrichment.

---
*End of Plan. Phase 1 & 2 Completed. Ready for Phase 3 (Apple Surface Implementation).*
