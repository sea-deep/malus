# Malus Audio Provider Protocol

This document defines the wire specification for Malus audio provider child processes.

The provider protocol is a language-independent JSON-RPC protocol over standard input/output. Any process (written in Rust, Python, Go, Node.js, C++, or Shell) adhering to this specification can be supervised by `malusd`.

---

## 1. Framing

All messages on `stdin` and `stdout` are framed with LSP-style length headers:

```text
Content-Length: <byte_length>\r\n
\r\n
<exact UTF-8 JSON payload bytes>
```

### Framing Constraints & Hostile Input Protection
- `MAX_HEADER_BYTES`: 8,192 bytes (8 KiB). Streams that exceed 8 KiB without delimiter `\r\n\r\n` are rejected immediately.
- `MAX_FRAME_SIZE`: 16,777,216 bytes (16 MiB).
- `Content-Length` must be non-negative ASCII digits only. Non-digits, signs, or trailing text are rejected.
- Duplicate `Content-Length` headers in a single frame are rejected.
- Provider `stdout` is **strictly protocol bytes**. Provider logs must go to `stderr`.

---

## 2. Handshake Sequence

Upon process execution, `malusd` sends a `Hello` request within a strict timeout window (default 5.0 seconds):

```json
{
  "method": "Hello",
  "params": {
    "version": [0, 1]
  }
}
```

The provider must respond with its metadata, protocol version, capabilities, and readiness:

```json
{
  "type": "Hello",
  "data": {
    "id": "apple",
    "name": "Apple Music Provider",
    "version": [0, 1],
    "capabilities": ["search", "playback", "playback.seek", "queue.read"],
    "status": "ready"
  }
}
```

If authentication is required before playback can occur (e.g. browser profile login required), the provider sets `status: "needs_auth"`.

---

## 3. Supervisor State Machine

`malusd` tracks provider lifecycle using the following state machine:

```text
Stopped
   ↓ (spawn)
Starting
   ↓ (pipes ready)
Handshaking
   ↓ (hello response)
Ready  /  NeedsAuth  /  Incompatible
   │
[Unexpected Process Exit]
   ↓
Restarting (attempt N, wait exponential backoff)
   ↓ (retry limit exceeded, e.g. 3 attempts)
Crashed
```

---

## 4. Action Envelope & Media Targeting

Provider-specific actions are routed using the normalized `ActionRequestV0` envelope:

```json
{
  "provider": "mock",
  "action": "mock.repost",
  "target": "mock:track:3",
  "params": {}
}
```

- `action`: The provider-specific action name (opaque to `malusd`).
- `target`: An optional normalized `MediaId` (`<provider>:<kind>:<opaque_id>`).
- `params`: Arbitrary action-specific JSON arguments.

---

## 5. Shutdown Escalation Lifecycle

To ensure clean process and child resource cleanup (e.g., automated browser instances):

```text
malusd → ProviderRequest::Shutdown
          ↓ (wait grace period, e.g. 1.0s)
Child exited? ─── yes ───> Done (Stopped)
          ↓ no
Send SIGTERM to child PID
          ↓ (wait kill timeout, e.g. 1.0s)
Child exited? ─── yes ───> Done (Stopped)
          ↓ no
Send SIGKILL to child PID
          ↓
Reap and finalize (Stopped)
```
