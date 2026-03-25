# Router Architecture

Status: Canonical
Scope: Current router implementation in this repository

## Overview

The Multifrost router is the shared runtime hub for v5.

It accepts WebSocket peer connections, performs synchronous registration,
maintains the live in-memory peer registry, answers router-level queries, and
routes traffic by `peer_id`.

The router enforces the v5 source-class rules at the ingress boundary:

- callers originate `call` traffic
- services originate `response` and `error` traffic
- both peer classes may send router `query`, `heartbeat`, and `disconnect`
  traffic

The router is intentionally narrow. It is not an application runtime, load
balancer, persistence layer, or schema validator.

## Scope Boundary

**Owns**: WebSocket accept loop, frame decoding at the router boundary, peer
registration, in-memory live registry, router-level query handling, routing,
disconnect invalidation, and router-owned error generation.

**Does not own**: application handler execution, application payload semantics,
language-level worker APIs, persistence of live routing state, or transport
alternatives.

**Boundary interfaces**: receives v5 frames defined by `docs/spec.md`; is used
by all language libraries that implement caller peers and service peers.

## Components

### Entry Point

- `router/src/main.rs`

Thin process entrypoint that starts the router runtime.

### Configuration

- `router/src/config.rs`

Owns runtime constant loading, port resolution, and operational path handling.

### Protocol Layer

- `router/src/protocol.rs`

Owns:

- envelope definitions
- register/query/call/response/error body definitions
- router-owned body definitions
- frame encoding and decoding
- protocol constants
- WebSocket binary message validation

### Registry

- `router/src/registry.rs`

Owns the in-memory live peer registry keyed by `peer_id`.

### Server Runtime

- `router/src/server.rs`

Owns:

- connection acceptance
- synchronous `register` handshake
- per-connection message loop
- source-class validation for `call`, `response`, and `error`
- `query`, `call`, `response`, `error`, `heartbeat`, and `disconnect` routing
- live-entry invalidation on disconnect or transport close

## Data Models / Storage

### Live Peer Registry

The registry is in memory only.

Each live entry includes at least:

- `peer_id`
- peer class
- sender or connection association
- connected state

No file-backed service registry is used for live routing.

### Operational Files

The router uses operational paths such as:

- `~/.multifrost/router.log`
- `~/.multifrost/router.lock`

These are operational support artifacts, not the active routing database.

## Relationships and Flow

### Registration Flow

1. A peer opens a WebSocket connection.
2. The first binary frame must be `register`.
3. The router validates envelope and register body alignment, including router
   targeting and matching `peer_id` values.
4. The router rejects malformed, duplicate, or source-invalid live identities.
5. The router inserts the peer only after validation succeeds.
6. The router replies explicitly with a `response` accept or an `error`
   rejection.
7. Only after acceptance does the connection enter the normal message loop.

### Query Flow

1. A caller or service peer sends router-owned `query` traffic.
2. The router looks up the requested `peer_id` in the live registry.
3. The router responds with `peer.exists` or `peer.get` data, including
   existence, peer class, and connected state.

### Call Routing Flow

1. A caller peer sends a `call` frame.
2. The router reads the envelope.
3. The router confirms that the `to` target exists and is a `service` peer.
4. The router forwards the frame to the destination peer.
5. The destination service peer later returns a `response` or `error`.
6. The router routes that return traffic back to the caller peer.
7. If a service peer sends `call`, or a caller peer sends `response` or
   `error`, the router rejects the frame.

### Disconnect Flow

If a peer sends `disconnect` or the WebSocket closes, the router removes the
live routing entry immediately. A graceful `disconnect` is acknowledged with a
`response` frame when delivery succeeds, but the router does not keep the peer
live after removal.

## Contracts / Invariants

| Invariant | Description |
|---|---|
| First message is register | A connection is not considered active before synchronous registration succeeds |
| Duplicate live peer rejection | Two live peers cannot share the same `peer_id` |
| Envelope-only routing | Router routing decisions do not depend on application body semantics |
| Body preservation | Routed body bytes are forwarded unchanged |
| Service-only call target | `call` traffic cannot target a `caller` entry |
| Source-class enforcement | `call` comes from callers; `response` and `error` come from services |
| Immediate disconnect eviction | Closed connections stop being live routing targets immediately |
| In-memory live state | Active peer routing state is not persisted to disk |

## Configuration / Operations

Current runtime constants:

- protocol key: `multifrost_ipc_v5`
- default port: `9981`
- env override: `MULTIFROST_ROUTER_PORT`
- log path: `~/.multifrost/router.log`
- lock path: `~/.multifrost/router.lock`

Operational posture:

- router is long-lived
- router is shared by all peers
- router is not owned by the bootstrapper
- router keeps all live routing state in memory

## Testing Strategy

Current router-focused coverage lives in:

- `router/tests/router_integration.rs`

The router should be validated with tests for:

- register accept and reject
- register-first connection rejection
- duplicate live `peer_id` rejection
- query behavior
- call routing
- source-class rejection for invalid `call`/`response`/`error` origins
- invalid target class handling
- disconnect eviction
- body preservation

End-to-end validation with real language libraries is part of the broader system
test strategy and complements router-only integration tests.

## Design Decisions

### Separate Router Crate

Confidence: High

The router lives outside any one language library because it is a shared core
component rather than a supplementary binding detail.

### In-Memory Live Registry

Confidence: High

The active routing source of truth stays in memory. This keeps liveness aligned
with the real transport state instead of with a stale file-based registry.

### Narrow Router Responsibility

Confidence: High

The router handles connection, presence, query, and routing only. Application
method execution remains inside service peers.
