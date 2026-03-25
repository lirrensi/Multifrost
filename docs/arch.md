# Multifrost Architecture

Status: Canonical
Scope: Shared v5 implementation architecture

## Overview

Multifrost v5 is implemented as one shared router plus any number of language
libraries that join that router as caller peers and service peers.

This document describes the broad implementation shape of the repository and the
shared architectural rules that apply across languages. It does not describe one
language binding in detail. Language-specific implementations may differ
internally as long as they preserve the behavioral contract in `docs/spec.md`.

The router is part of the core framework and has a concrete implementation in
this repository. That implementation is described separately in
`docs/arch_router.md`. The split architecture map lives in
`docs/arch_index.md`.

## Scope Boundary

**Owns**: the shared v5 runtime shape, router-centered topology, peer classes,
frame format, msgpack boundaries, bootstrap model, and cross-language
implementation invariants.

**Does not own**: per-language module layout, framework-specific ergonomics,
application-level method semantics, authorization policy, persistence, or load
balancing.

**Boundary interfaces**: `docs/spec.md` defines the required behavior; this
document defines how the current repository realizes that behavior at a shared
system level.

## Core Runtime Shape

### One Router, Many Libraries

The v5 runtime centers on one shared router process.

Language libraries do not talk to each other directly by default. They all join
the same router and exchange traffic through it.

This gives the system a stable central point for:

- live presence,
- registration,
- query,
- routing,
- disconnect handling.

### Two Peer Classes

Every library implementation must support the two v5 peer classes:

- `service peer`
- `caller peer`

A single process may host any number of either class. The process itself is not
the peer. Each peer instance has its own identity and responsibility.

At the architecture level:

- service peers expose callable functions and receive `call` traffic
- caller peers issue `call` traffic and receive `response` or `error` traffic
- both classes use the same routing fields: `from` and `to`
- only caller peers originate `call` traffic
- only service peers originate `response` or `error` traffic
- both classes may originate router `query`, `heartbeat`, and `disconnect`
  traffic
- the router enforces these source-class rules at the ingress boundary

### Optional Spawn Helper

Libraries may provide a `spawn` helper, but `spawn` is not part of the transport
topology.

`spawn` only starts a process. The started process then behaves like any other
service peer: it resolves its `peer_id`, connects to the router, registers, and
waits for calls.

### Shared Library Surface

Across languages, the intended house style is configuration-first and
handle-driven.

The shared caller-side pattern is:

- create a connection descriptor for the target service peer,
- optionally start a service process through a separate `spawn` helper,
- derive a runtime handle from the connection,
- call `start()` on that handle,
- use `handle.call.<function>(...)` for remote method calls.

The shared service-side pattern is:

- define a service implementation,
- create a service context with explicit or default `peer_id`,
- run the service so it joins the router and waits for inbound calls.

Exact names may differ by language, but primary v5 naming should center on
service, caller, connect, handle, and run-service concepts. `parent` and
`child` naming is legacy vocabulary and should not be the primary v5 style.

## Shared Data Models

### Peer Identity

Every peer has a `peer_id`.

- service peer ids are usually stable and intentionally known by callers
- caller peer ids may be generated and ephemeral
- when a service peer does not provide an explicit id, the implementation uses a
  canonical absolute entrypoint path as the default identity

### Router Registry Model

The live router registry is in memory.

At minimum, each live entry contains:

- `peer_id`
- peer class: `service` or `caller`
- live connection association
- connected state

The router is the source of truth for what is currently reachable.
Router-owned query responses expose `peer.exists` and `peer.get` views over
that live registry.

### Binary Frame Model

All normal traffic uses the shared v5 binary frame format:

```text
[ u32 envelope_len ][ envelope_bytes ][ body_bytes ]
```

The envelope contains routing metadata. The body contains msgpack payload bytes.

This split is an architectural boundary, not only a protocol detail:

- routers route from the envelope
- application payload remains in the body
- language libraries decode the body according to message kind and local API

### Msgpack Boundary

MessagePack is the shared payload encoding across implementations.

At the architecture level, the important constraints are:

- envelope and router-owned bodies must remain cross-language decodable
- generic numeric payloads must stay within the portable subset defined in
  `docs/spec.md`
- application payload bytes must survive router forwarding unchanged

## Relationships and Flow

### Service Startup Flow

1. A service peer instance starts.
2. The service peer resolves its `peer_id`.
3. The service peer attempts router connection.
4. If the router is absent, the service peer may participate in router
   bootstrap coordination.
5. The first binary frame on the connection is `register`.
6. The service peer registers synchronously as class `service`.
7. After acceptance, the service peer begins receiving calls addressed to its
   own `peer_id`.

### Caller Flow

1. A caller peer instance starts.
2. The caller peer gets or generates a `peer_id`.
3. The caller peer attempts router connection.
4. If needed, the caller peer may participate in router bootstrap coordination.
5. The first binary frame on the connection is `register`.
6. The caller peer registers synchronously as class `caller`.
7. The caller peer may issue router `query` traffic.
8. The caller peer sends `call` traffic to a target service `peer_id`.
9. The caller peer receives a routed `response` or `error`.

### Disconnect Flow

When a peer disconnects gracefully or loses its WebSocket connection, the router
removes or invalidates the live routing entry immediately.

That behavior is architecture-critical because all libraries rely on router
presence as the shared liveness truth.

## Contracts / Invariants

| Invariant | Description |
|---|---|
| Router-centered topology | Normal peer traffic routes through the shared router |
| Envelope-only routing | The router makes routing decisions from envelope metadata only |
| Body preservation | The router forwards body bytes unchanged except for transport delivery framing |
| Service-only call targets | Only `service` peers are valid `call` destinations |
| Source-class routing | Callers originate `call`; services originate `response` and `error` |
| Register-first connections | New connections do not become active until `register` succeeds |
| Immediate disconnect invalidation | Closed WebSocket connections stop being live routing targets immediately |
| Synchronous registration | A peer is not active until `register` has been explicitly accepted |
| Bootstrap without ownership | The peer that started the router does not own router lifetime |
| In-memory live state | Live routing state is not persisted as the active source of truth |

## Configuration / Operations

The shared runtime constants are defined in `docs/spec.md`. The architecture
depends on them being consistent across implementations.

Current core constants:

- protocol key: `multifrost_ipc_v5`
- transport: WebSocket only
- default port: `9981`
- port override env var: `MULTIFROST_ROUTER_PORT`
- router log path: `~/.multifrost/router.log`
- router bootstrap lock path: `~/.multifrost/router.lock`

Operationally:

- the router is a long-lived shared process
- the router is not tied to the lifetime of the bootstrapper
- peers coordinate bootstrap with an OS-level lock
- the active routing registry remains in memory only

## Architecture Map

- `docs/arch_index.md`: map of the split architecture docs
- `docs/arch.md`: shared v5 system architecture
- `docs/arch_router.md`: concrete router implementation architecture
- `router/`: router implementation
- `rust/`: first v5 language implementation
- other language implementations are expected to follow the same shared v5
  architecture as they are ported

## Design Decisions

### Router As Core Runtime Component

Confidence: High

The router is part of the framework core, not an optional side utility. That is
why it receives its own architecture document.

### Language Libraries As Replaceable Implementations

Confidence: High

Language bindings are supplementary implementations of the same shared model.
They may grow independently and additional languages may be added later. The
top-level architecture therefore describes the common shape, not one binding's
internals.

### MessagePack As Shared Payload Boundary

Confidence: High

Msgpack remains the shared cross-language payload format, but architectural
correctness depends on preserving the envelope/body split and portable value
rules rather than on reproducing one language's internal types.

### Architecture Docs Split

Confidence: High

This repository now uses a split architecture layer:

- `docs/arch.md` for shared system architecture
- `docs/arch_router.md` for the router's concrete implementation shape

Additional architecture documents may be added later if other components become
large enough to justify their own canon docs.
