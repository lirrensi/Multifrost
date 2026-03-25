# Multifrost v5 Support Matrix

Status: Informational
Scope: Cross-language v5 surface snapshot

This document is a quick reference for what the current v5 bindings support.
It is not the behavioral canon. For exact behavior, see `docs/spec.md` and the
language-specific architecture docs.

## Shared V5 Surface

All four language bindings support the same router-based model:

| V5 Surface | Python | JavaScript | Go | Rust | Notes |
|---|---|---|---|---|---|
| Caller configuration + live handle | ✅ | ✅ | ✅ | ✅ | Configuration is separate from the live runtime in every binding |
| Service runtime | ✅ | ✅ | ✅ | ✅ | Each binding exposes a way to run a service peer on the router |
| Separate service process launcher | ✅ | ✅ | ✅ | ✅ | `spawn` is a helper, not the core network model |
| Router bootstrap | ✅ | ✅ | ✅ | ✅ | Bootstrap is peer-driven and uses the shared lock path |
| Binary WebSocket transport | ✅ | ✅ | ✅ | ✅ | WebSocket is the only live transport |
| Msgpack payloads | ✅ | ✅ | ✅ | ✅ | The body is msgpack-encoded and forwarded unchanged by the router |
| Framed messages | ✅ | ✅ | ✅ | ✅ | The shared layout is `[ u32 envelope_len ][ envelope_bytes ][ body_bytes ]` |
| `register` handshake | ✅ | ✅ | ✅ | ✅ | Peers register before normal traffic becomes active |
| `query_peer_exists` / `peer.exists` | ✅ | ✅ | ✅ | ✅ | Supported for live presence checks |
| `query_peer_get` / `peer.get` | ✅ | ✅ | ✅ | ✅ | Supported for peer class and connection state lookup |
| `call` routing | ✅ | ✅ | ✅ | ✅ | Calls originate from callers and target service peers |
| `response` routing | ✅ | ✅ | ✅ | ✅ | Responses return from services to callers |
| `error` routing | ✅ | ✅ | ✅ | ✅ | Router and service peers can emit error traffic |
| Default service identity from entrypoint path | ✅ | ✅ | ✅ | ✅ | Explicit `peer_id` overrides the default when provided |

## Binding Shapes

### Python

- Async caller core with a sync convenience layer
- Public caller surface: `connect`, `Connection`, `Handle`, `HandleSync`
- Public service surface: `spawn`, `ServiceProcess`, `ServiceWorker`, `ServiceContext`, `run_service`, `run_service_sync`

### JavaScript

- Async-only public API
- Public caller surface: `connect`, `Connection`, `Handle`
- Public service surface: `spawn`, `ServiceProcess`, `ServiceWorker`, `ServiceContext`, `runService`

### Go

- Synchronous, context-driven public API
- Public caller surface: `Connect`, `Connection`, `Handle`
- Public service surface: `Spawn`, `ServiceProcess`, `ServiceContext`, `RunService`

### Rust

- Async caller surface with sync service convenience support
- Public caller surface: `connect`, `Connection`, `Handle`
- Public service surface: `spawn`, `ServiceProcess`, `ServiceWorker`, `SyncServiceWorker`, `ServiceContext`, `run_service`, `run_service_sync`

## What This Matrix Leaves Out

This document does not try to compare:

- ZeroMQ-era behavior
- file-backed service registries
- metrics and logging internals
- retry and circuit-breaker implementation details
- per-language performance differences
- legacy parent/child naming

Those details belong in the per-language architecture docs and the code itself.

## Related Docs

- `docs/spec.md`
- `docs/arch.md`
- `javascript/docs/arch.md`
- `python/docs/arch.md`
- `golang/docs/arch.md`
- `rust/docs/arch.md`
