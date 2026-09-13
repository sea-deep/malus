# Retired Provider Framework Architecture

## Summary
The original Malus architecture was designed around an abstract multi-provider model where third-party music services (Apple Music, Spotify, Tidal, local folders, mock) ran as isolated child processes communicating with `malus-daemon` over standard I/O via `malus-provider-sdk` and framed JSON RPC.

## Archive Tag and Branch
- **Tag:** `provider-framework-final`
- **Branch:** `provider-framework-final`
- **Commit:** `a3726dfa754d583a912e1bdfac76f9a69a572f2b`

## Reason for Retirement
Product decision changed: **Malus is a dedicated, first-class native Apple Music client for Linux**.

The provider abstraction introduced significant indirection, JSON IPC serialization overhead, process management complexity, and capability discovery queries across every layer without providing consumer value. 

By removing the provider framework and integrating `malus-apple` directly into `malus-daemon` in-process:
1. Fast metadata (search, catalog, library, recommendations, charts, radio, replay) runs over native Rust HTTP with zero process hops.
2. Playback runtime (WPE WebKit / MusicKit / Widevine) remains lazily launched only when audio playback actually starts.
3. IPC between `malus-client` and `malus-daemon` is retained for CLI/GUI separation and background playback.
4. Product domain models are first-class Apple models (`ApplePageRoute`, `AppleEntityRefWire`, `AppleActionWire`, `AppleNavigationWire`, `ApplePageWire`).

## Date of Retirement
September 13, 2026
