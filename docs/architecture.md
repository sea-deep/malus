# Malus Architecture

Malus is an extensible, process-isolated personal audio playback architecture.

```text
Frontends (CLI / TUI / GUI)
          ↕  (Unix Domain Socket / IPC, Client RPC)
        malusd (Daemon Supervisor & Coordinator)
          ↕  (Standard I/O Pipes, Provider Wire Protocol)
  [Provider Process A]    [Provider Process B (e.g. Apple Music CDP)]
```

---

## The Eight Core Invariants

1. **Core has zero transport/runtime/provider knowledge.**
   `malus-core` contains pure domain structures and representation invariants (`Player`, `Queue`, `MediaId`, `Track`). It has zero external dependencies, zero async runtimes, and zero serialization knowledge.

2. **Providers are processes, never linked plugins.**
   Providers are standalone executables spawned by `malusd`. A provider crash or lockup cannot crash the daemon.

3. **Provider stdout is protocol-only.**
   Standard output is exclusively reserved for length-delimited JSON-RPC protocol bytes. Standard error (`stderr`) is strictly reserved for diagnostic logs, captured and forwarded asynchronously by the daemon supervisor.

4. **Providers own service authentication and playback.**
   Providers manage credentials, DRM / Widevine, browser automation, stream sessions, and playback clocks. The daemon never holds service cookies, tokens, or raw playback streams.

5. **`malusd` owns provider lifecycle and normalized observable state.**
   `malusd` manages process supervision, restart backoff, shutdown escalation, active-provider routing, and state aggregation. The daemon mirrors the authoritative provider state.

6. **Frontends never communicate directly with providers.**
   Frontends talk only to `malusd` over Unix domain sockets using `client::` RPC protocol.

7. **Provider-specific semantics may extend actions/capabilities, never core presentation.**
   Specialized actions (e.g. `mock.repost`, playlist synchronization) use normalized action envelopes (`ActionRequestV0`), while primary playback and queue presentation remains strictly normalized.

8. **Wire DTOs are not domain models.**
   Over-the-wire serialization structures (`malus_protocol::wire`) are explicitly separated from core domain entities via checked conversion boundaries.
