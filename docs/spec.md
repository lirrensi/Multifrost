# Multifrost v5 Behavioral Specification

Status: Canonical
Version: v5 direction

## Abstract

This document defines the behavioral contract for Multifrost v5. Multifrost is a
router-based, cross-language RPC system in which `service peers` expose callable
functions, `caller peers` issue calls, and a shared `router` routes messages by
peer identity over WebSocket.

The goal of this specification is to define behavior precisely enough that an
independent implementation could interoperate without relying on the current v4
codebase or topology.

## Introduction

Multifrost v4 was centered on direct parent-child IPC. Multifrost v5 replaces
that model with a shared router that all participants connect to.

This changes the core mental model:

- `parent` and `child` are no longer protocol roles
- `spawn` is no longer a protocol concept
- `connect` becomes the canonical way to join the network
- the router becomes the central source of live presence and message routing

The system is designed so that services may start independently, callers may
appear independently, and either side may live in any supported language.

## Scope

This specification defines:

- peer classes and router responsibilities,
- routing and identity rules,
- registration and bootstrap behavior,
- message kinds and framing,
- presence queries,
- error handling and conformance.

This specification does not define:

- one required implementation language for the router,
- application-level function semantics,
- application-level authorization policy,
- router-side load balancing,
- process supervision policy outside peer bootstrap behavior,
- the exact public API spelling of every language binding beyond the shared v5
  library pattern defined here.

## Terminology

Shared terms are defined here and, where useful, in [`docs/glossary.md`](glossary.md).

### Router

The shared runtime component that accepts peer connections, tracks presence,
answers router-level queries, and routes messages by `peer_id`.

### Service Peer

A peer class that exposes callable functions and receives `call` messages
addressed to its own `peer_id`.

### Caller Peer

A peer class that issues `call` messages and receives `response` and `error`
messages addressed back to its own `peer_id`.

### Peer ID

A string identifier used for routing. Both service peers and caller peers have a
`peer_id`. Service peer identities are usually stable. Caller peer identities
are often ephemeral.

### Envelope

The routing metadata section of a message. The router MUST be able to route
traffic using the envelope without inspecting the application body.

### Body

The msgpack-encoded application payload carried after the envelope.

### Query

Router-only control traffic used to inspect router state, such as whether a
peer exists and what class it belongs to.

## Normative Language

The key words `MUST`, `MUST NOT`, `SHOULD`, `SHOULD NOT`, and `MAY` in this
document are to be interpreted as described in RFC 2119.

## System Model

### Actors

The system consists of three runtime actors:

- `router`
- `service peer`
- `caller peer`

These are distinct roles. A single process MAY host any number of service peers
and any number of caller peers, but each instance remains separate and has its
own responsibility and `peer_id`. The process itself is not the protocol peer;
the individual peer instances are.

### Peer Classes

#### Service Peer

A service peer:

- MUST expose callable functions,
- MUST register as class `service`,
- MUST have a unique `peer_id` among live peers on the router,
- MUST accept inbound calls addressed to its own `peer_id`.

If a service peer does not provide an explicit `peer_id`, the implementation
MUST default it to the absolute path of the service file or equivalent runtime
entrypoint.

#### Caller Peer

A caller peer:

- MUST have a `peer_id`, explicit or generated,
- MUST register as class `caller`,
- MAY be long-lived or short-lived,
- MUST be routable for return traffic addressed to its own `peer_id`.

This specification does not require caller peer IDs to be stable across runs.

### Router Registry

The router MUST maintain an in-memory registry keyed by `peer_id`.

Each registry entry MUST include at least:

- `peer_id`
- `class` (`service` or `caller`)
- live transport association
- connection state

The router MAY track additional metadata, but conformance MUST NOT depend on
implementation-specific metadata not defined here.

### Routing Model

All routed messages use the same identity fields:

- `from`
- `to`

The router MUST use these fields together with its registry to forward traffic.

The router MUST understand whether a target `peer_id` belongs to a `service` or
`caller` entry, because query responses and call validation depend on that peer
class.

### Liveness Model

WebSocket connection state is the primary liveness signal.

Heartbeat exists for RTT and responsiveness telemetry. Heartbeat is not the
primary source of truth for whether a peer is live.

When the WebSocket connection closes, the router MUST immediately treat the peer
as disconnected and MUST remove or invalidate its live routing entry.

### Router Operational Posture

The router is a long-lived shared process.

Once started, it MUST NOT be considered owned by the peer that bootstrapped it.
If that bootstrapper later exits, the router remains valid and MAY continue
serving other peers until it is explicitly terminated or the host environment
cleans it up.

### Router Runtime Constants

The v5 router implementation MUST use these exact runtime constants:

- protocol key: `multifrost_ipc_v5`
- transport: WebSocket only
- default port: `9981`
- port override env var: `MULTIFROST_ROUTER_PORT`
- log file path: `~/.multifrost/router.log`
- bootstrap lock path: `~/.multifrost/router.lock`

The router process is long-lived, keeps live routing state only in memory, and
is not shut down automatically when the bootstrapper exits.

## Conformance

### Router Conformance

A conforming router implementation MUST:

- accept peer connections,
- maintain a registry keyed by `peer_id`,
- record peer class for each live registry entry,
- reject duplicate live `peer_id` registration,
- accept only binary WebSocket protocol messages for v5 traffic, while ping/pong
  control frames MAY be ignored,
- require `register` as the first binary frame on a new connection,
- route `call`, `response`, and `error` messages by envelope identity,
- enforce source-class routing rules for `call`, `response`, and `error`,
- answer `query` messages against current registry state,
- use transport connection state as the main liveness signal,
- use WebSocket as the transport,
- support coordinated bootstrap behavior when peers attempt startup.

### Service Peer Conformance

A conforming service peer implementation MUST:

- determine a `peer_id`, explicit or defaulted,
- eagerly attempt router connection on startup,
- register itself as class `service`,
- accept inbound calls to its own `peer_id`,
- return either `response` or `error` for handled calls.

### Caller Peer Conformance

A conforming caller peer implementation MUST:

- determine or generate a `peer_id`,
- connect to the router before issuing calls,
- register itself as class `caller`,
- receive return traffic addressed to its own `peer_id`,
- handle router and remote errors distinctly from successful responses.

### Optional Behavior

Implementations MAY provide process startup helpers such as `spawn`, but such
helpers are outside core conformance. A helper MUST NOT redefine the network
roles specified here.

### Canonical Library Pattern

This specification does not require one exact class or function name in every
language, but conforming language libraries SHOULD expose the same broad v5
usage pattern.

Caller-side libraries SHOULD separate configuration from live runtime use:

1. create a connection descriptor for a target service peer,
2. optionally start a service process through a separate `spawn` helper,
3. derive a runtime handle from the connection,
4. call `start()` on that handle,
5. issue remote calls through `handle.call.<function>(...)`,
6. stop or drop the handle when done.

Additional rules:

- `spawn` MUST remain operational helper behavior rather than a replacement for
  router-based connect/register flow
- libraries MAY provide a synchronous handle variant in ecosystems where that is
  natural, but async handle flow is the primary model
- primary v5 public naming SHOULD use service/caller/connect/handle terminology
  rather than parent/child terminology
- parent/child naming MAY exist only as compatibility aliases during migration

Service-side libraries SHOULD expose a small runner-oriented surface:

- a service implementation type or value,
- a service context that carries explicit or default `peer_id`,
- a runner function or method that starts the service peer and registers it with
  the router.

## Behavioral Specification

### Router Bootstrap

Router bootstrap is peer-driven.

Any service peer or caller peer MAY attempt bootstrap if its initial router
connection attempt fails.

Bootstrap behavior MUST follow this sequence:

1. The peer reads the router endpoint from environment or configuration.
2. The peer attempts to connect.
3. If connection succeeds, bootstrap ends.
4. If connection fails, the peer attempts to acquire the shared router startup
   lock.
5. After acquiring the lock, the peer MUST retry router reachability.
6. If the router is now reachable, the peer MUST release the lock and continue.
7. If the router is still unreachable, the peer MAY start the router process.
8. The peer MUST wait for router readiness up to an implementation-defined
   timeout.
9. The peer MUST release the lock once the router is reachable or startup has
   failed.

Peers that do not hold the lock MUST wait, retry reachability, and only attempt
bootstrap after lock release if the router is still unreachable.

Stale lock handling MUST exist.

`spawn` MUST NOT be treated as the owner of router bootstrap. If `spawn` starts a
service process, that service process behaves like any other service peer and
performs its own connection/bootstrap flow.

### Registration

After router connection is established, a peer MUST register before issuing or
receiving normal traffic.

The first binary frame on a new connection MUST be `register`.

Registration is synchronous. A peer sends `register` and the router responds
with acceptance or rejection before the peer is considered active.

The `register` message MUST identify:

- the registering `peer_id`
- the peer `class`

The register envelope MUST target the router and the envelope `from` field MUST
match the register body `peer_id`.

The router MUST acknowledge successful registration with a `response` frame.
The response body MUST indicate acceptance. The router MUST return an explicit
`error` frame for failed registration and MAY close the connection immediately
after doing so.

The router MUST reject registration when:

- the `peer_id` is already live,
- the peer class is missing or invalid,
- the envelope is malformed.

Peer startup or connect flow MUST succeed only when registration is accepted.

### Query

`query` is router-only control traffic.

A conforming router MUST answer `peer.exists` by `peer_id` and MUST answer
`peer.get` by `peer_id`.

The router MUST be able to answer:

- whether a given `peer_id` currently exists,
- whether that entry is a `service` or `caller`.

The `peer.get` response MUST include:

- `peer_id`
- `exists`
- `class`
- `connected`

The router MAY support richer queries such as `peer.list`.

Default discovery behavior SHOULD focus on service peers. Caller peers MAY be
returned when explicitly requested.

### Call Routing

`call` is normal application request traffic sent from a caller peer to a
service peer.

Only caller peers MAY originate `call` traffic.

For a valid call:

- `from` MUST identify the caller peer,
- `to` MUST identify the target service peer,
- the target MUST exist as a live `service` entry,
- the body MUST contain the application call payload.

If the target does not exist, the router MUST return an `error`.

If the target exists but is registered as class `caller`, the router MUST return
an `error` indicating an invalid target class.

The router MUST NOT inspect function names or argument shapes in order to route
the message.

The router MUST make its routing decision from the envelope alone. The router
MUST NOT interpret application-level body semantics. After routing, the router
MUST forward the body bytes unchanged except for transport framing required by
delivery.

### Response Routing

`response` is successful return traffic from a service peer to the originating
caller peer.

Only service peers MAY originate `response` traffic.

For a valid response:

- `from` MUST identify the responding service peer,
- `to` MUST identify the original caller peer,
- the response MUST be routed using the same `peer_id` contract as any other
  message.

### Error Routing

`error` may originate from the router or a service peer.

Only the router and service peers MAY originate `error` traffic.

Router-generated errors cover conditions such as:

- unknown target,
- invalid target class,
- malformed envelope,
- malformed body,
- duplicate registration,
- router bootstrap failure.

Service-generated errors cover conditions such as:

- function not found,
- invalid call body for the service,
- application failure during execution.

Implementations SHOULD preserve whether an error originated at router/protocol
level or at service/application level.

### Heartbeat

`heartbeat` is optional telemetry traffic used for RTT and responsiveness
measurement.

Heartbeat MUST NOT be the primary liveness rule. A lost WebSocket connection is
sufficient to mark a peer disconnected.

### Disconnect

`disconnect` is graceful leave traffic.

When a peer disconnects gracefully or the transport closes, the router MUST mark
the registry entry as no longer live and MUST stop routing new traffic to that
entry.

## Data and State Model

### Framing

The binary frame layout is:

```text
[ u32 envelope_len ][ envelope_bytes ][ body_bytes ]
```

Rules:

- `envelope_len` MUST describe only the envelope byte length
- `envelope_bytes` MUST encode routing metadata
- `body_bytes` MUST contain the msgpack application payload
- the router MUST be able to route using the envelope without decoding the full
  body

### Envelope

The envelope MUST contain enough information to route and validate traffic.

Minimum envelope fields:

| Field | Type | Required | Notes |
|---|---|---|---|
| `v` | integer | Yes | Protocol version marker |
| `kind` | string | Yes | Message kind |
| `msg_id` | string | Yes | Correlation identifier |
| `from` | string | Yes | Origin `peer_id` |
| `to` | string | Yes except bootstrap-local cases | Target `peer_id` |
| `ts` | number | Yes | Timestamp |

### Body

The body MUST be msgpack-encoded.

Portable numeric values in the generic message body MUST be limited to:

- signed integers in the range `[-2^63, 2^63 - 1]`
- finite IEEE-754 floating-point values

`NaN`, `Infinity`, and `-Infinity` MUST NOT appear as generic numeric payload
values unless explicitly encoded by the application into another representation.

Applications that need larger integers, exact decimals, tensor-like dtypes, or
bit-exact float preservation MUST encode those values explicitly at the
application layer.

Recommended call body shape:

```json
{
  "function": "methodName",
  "namespace": "default",
  "args": []
}
```

Recommended success response body shape:

```json
{
  "result": null
}
```

Recommended error body shape:

```json
{
  "error": {
    "code": "STRING_CODE",
    "message": "Human readable message",
    "kind": "library",
    "stack": null,
    "details": {}
  }
}
```

### Registry Entry Model

Minimum router registry entry:

```json
{
  "peer_id": "string",
  "class": "service",
  "connected": true
}
```

The live transport handle is implementation-specific and need not be exposed in
the wire model.

## Error Handling and Edge Cases

### Registration Errors

The router MUST reject registration when:

- a live peer already owns the requested `peer_id`,
- the peer class is neither `service` nor `caller`,
- required routing fields are missing,
- the transport is not in a valid registered state.
- the first binary frame is not `register`.
- the register envelope does not target the router.
- the register envelope `from` does not match the register body `peer_id`.

If a connection begins with any non-`register` frame, the router MUST reject it
and close the connection.

### Query Errors

The router MUST return explicit errors for malformed queries.

A query for an unknown `peer_id` SHOULD return a successful negative result
rather than a transport failure.

### Call Errors

The router MUST return an error when:

- the target `peer_id` does not exist,
- the target exists but is class `caller`,
- the caller is not registered,
- the envelope is malformed.
- a caller peer sends `response` or `error`.
- a service peer sends `call`.

The receiving service peer MUST return an error when:

- the requested function does not exist,
- the requested function is not callable,
- the service rejects the payload,
- the application handler fails.

### Bootstrap Errors

If router startup fails after lock acquisition, the peer MUST release the lock
and surface a bootstrap failure.

If router readiness is not reached within timeout, the peer MUST release the
lock and surface a bootstrap timeout.

## Security Considerations

This specification assumes trusted local or trusted private-network use unless a
higher-level security layer is added.

Multifrost v5 does not by itself provide:

- authentication,
- encryption,
- authorization.

Implementations SHOULD NOT expose the router to untrusted networks without an
additional security layer.

Service peers MUST treat call arguments as untrusted input.

## Compatibility and Migration

Multifrost v5 is a behavioral break from v4.

Key differences:

- v4 used parent-child IPC as the core mental model
- v5 uses router-based routing between caller peers and service peers
- v4 treated spawn/connect as central lifecycle modes
- v5 treats `connect` as the core network action and `spawn` as a helper
- v4 relied on direct transport relationships
- v5 routes all normal traffic through the router
- v5 uses a separate protocol key and is intentionally incompatible with v4

This specification does not require wire compatibility with v4.

## References

### Normative References

- RFC 2119: Key words for use in RFCs to Indicate Requirement Levels
- MessagePack specification

### Informative References

- `docs/product.md`
- `docs/v5_rewrite_plan.md`
