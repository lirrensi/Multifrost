# Multifrost v5 Rewrite Plan

Status: Draft
Scope: Next major version
Branching model: v4 remains stable; v5 evolves separately

## Goal

Version 5 is a structural rewrite that keeps the public API broadly familiar while
changing the internal architecture from direct parent-child routing to a shared
brokered bus.

The intended outcome is:

- More language coverage with less per-language transport pain
- A unified shared space for peers
- Peer-to-peer communication through the same routing layer
- A stable, binary-first protocol that still uses msgpack for the payload

## What Stays Familiar

The public surface should remain roughly the same:

- `handle()`
- `call.methodName(...)`

The user experience should still feel like remote function calls, not manual
socket plumbing.

## What Changes

The main changes are internal:

- Direct parent-child topology becomes brokered routing
- The transport layer is isolated behind a small adapter boundary
- Message framing is split into routing metadata plus payload
- Peers no longer need to know which process started them
- Callers no longer need to establish a direct relationship with a peer

## Locked Decisions

These decisions are considered fixed for the v5 redesign:

- `spawn/connect` are no longer protocol-level modes
- `parent/worker` are no longer the conceptual model
- Every process is a `peer`
- A peer declares a unique `peer_id` on the network
- If no explicit `peer_id` is provided, the absolute file path is used as the default identifier
- On a given router instance, only one peer with the same `peer_id` may exist at a time
- The router uses an O(1) hash table for peer lookup
- The router only routes messages; it does not do load balancing
- Load balancing, if needed, is implemented by the peer itself
- WebSocket is the preferred transport for connection state and liveness
- `spawn` becomes a convenience helper for starting a process and keeping it alive
- `spawn` does not grant higher authority over a peer beyond the ability to shut it down
- `start` succeeds only when `register` is accepted by the router
- `heartbeat` is for RTT and responsiveness measurement, not liveness detection
- If the process is managed by systemd or another supervisor, it can connect directly without spawning

## Core Architecture

### 1. Shared Bus / Router Binary

The new central component is a long-lived router binary.

Responsibilities:

- Accept connections from peers
- Track peer registrations
- Route calls by peer ID or other routing key
- Forward responses back to the original sender
- Track connection state and heartbeat telemetry
- Support selective broadcast or fanout when needed

This router is the shared space.

### 1.1 Router Responsibilities

The router should stay intentionally small. It should not become an application
runtime or a function validator.

Router duties:

- Accept peer registration and reconnect events
- Maintain an in-memory lookup table for active peers
- Route messages based on the envelope only
- Forward envelopes and bodies without interpreting worker payloads
- Track connection state and heartbeat telemetry
- Reject duplicate peer IDs on the same router instance
- Provide a startup endpoint or socket that all peers can reach

Router should not:

- Inspect function names
- Validate argument shapes
- Decide whether a worker accepts a call
- Perform load balancing unless it is a future explicit feature
- Restart workers, except by optional helper integration outside the core router

### 1.2 Startup Model

The router startup flow should be simple and predictable:

1. A peer starts.
2. It reads the router port from the agreed environment variable or config.
3. It attempts to connect to the router.
4. If the router is absent, the optional spawn helper may start it.
5. If the router cannot be reached and no startup helper is present, this is a
   hard failure.

The router itself should be the only central routing point. Peers do not
communicate directly as part of the core model.

### 1.3 Heartbeat Model

Peers may heartbeat to the router for health and RTT measurement.

- The WebSocket connection itself is the primary liveness signal
- Heartbeat is for RTT and responsiveness telemetry
- Router does not need to know process IDs for correctness
- Process supervision belongs to the peer's host environment or optional spawn
  helper

PID tracking can exist later as an optional helper feature, but it should not be
part of the core router contract.

### 2. Protocol Shape

The message format should be split into two layers:

- **Envelope**: small routing metadata used by the router
- **Body**: the existing msgpack payload

#### Current Candidate Schema

Envelope:

```json
{
  "v": 5,
  "kind": "call",
  "msg_id": "uuid",
  "from": "peer-a",
  "to": "peer-a",
  "ts": 1234.5
}
```

Body:

```json
{
  "function": "methodName",
  "namespace": "internal-space",
  "args": [1, 2]
}
```

Interpretation:

- The router only needs the envelope
- The worker parses the body and decides whether it accepts the message
- `to` identifies the destination peer
- Function name and internal namespacing live in the body
- The body remains msgpack-encoded and is unchanged in spirit from v4

Important rule:

- Do not introduce a second msgpack layer if it can be avoided
- Keep the body msgpack-compatible and as close to the current schema as
  possible
- Prefer binary framing so the router can inspect routing data without decoding
  the full body

Preferred framing:

- First `u32`: envelope length
- Envelope bytes: msgpack envelope
- Remaining bytes: msgpack body

If the transport supports message boundaries, the entire record can ride in one
transport frame. If not, the same logical split can be carried inside a single
binary stream record.

### 2.1 Message Kinds

The control plane should stay intentionally small.

Core kinds:

- `register`: peer registration and initial hello
- `disconnect`: graceful leave notification
- `heartbeat`: RTT and responsiveness measurement
- `call`: request to execute work on a target peer
- `response`: successful return value for a call
- `error`: call failure or body parse failure

Response body candidate:

- Success response body: `{"result": <any msgpack-serializable value>}`
- Error response body: `{"error": {...}}`

`result` can be:

- A primitive value
- A list or array
- A map or object
- Binary data
- A nested structure composed of the above

For numeric values, follow the portable numeric subset already defined in the
protocol docs. If a value needs precision outside that subset, the application
should encode it explicitly.

Recommended error object:

```json
{
  "error": {
    "code": "FUNCTION_NOT_FOUND",
    "message": "Function add was not found",
    "kind": "library",
    "stack": "best-effort stack trace string",
    "details": {}
  }
}
```

Error fields:

- `code`: stable machine-readable identifier
- `message`: human-readable summary
- `kind`: `library`, `protocol`, `transport`, or `application`
- `stack`: best-effort stack trace or traceback as a string
- `details`: optional structured metadata for extra debugging context

Standardization rule:

- Library-generated failures use standardized `code` values
- Application failures use `kind: application` and may preserve the worker's
  native error message and stack/trace information
- If a language cannot provide a stack trace, it may omit `stack` or set it to
  `null`
- The wire contract should prefer string traces over frame arrays for maximum
  cross-language compatibility

Optional future kinds:

- `update`: metadata refresh
- `broadcast`: explicit fanout use case
- `shutdown`: router-initiated stop signal for helper-managed peers

Recommended handshake:

1. Peer opens the transport connection.
2. Peer sends `register`.
3. Router validates `peer_id` uniqueness and accepts or rejects immediately.
4. A successful `register` ack means the peer is known to the router.
5. Peer may begin heartbeating after registration.
6. Heartbeat replies are used for RTT and health telemetry.
7. On graceful exit, peer sends `disconnect` and closes the transport.

### 2.2 Error Categories

Errors should be separated by layer:

- **Transport errors**: the message could not be delivered, framed, routed, or
  accepted by the router
- **Protocol errors**: the peer or router received a malformed envelope or body
- **Application errors**: the worker parsed the body successfully but the
  requested function failed

Recommended handling:

- Transport errors are handled by the transport layer and should not be confused
  with application-level failures
- Protocol errors should be emitted as `error` at the protocol level
- Application errors should live in the call response body and remain the
  worker's responsibility

### 2.3 Peer Reconnect Rule

Duplicate peer IDs require a clear policy.

Recommended rule:

- A second live peer with the same `peer_id` is rejected
- If the original connection is clearly gone, a reconnect using the same
  `peer_id` may replace the stale entry
- WebSocket close or a short heartbeat grace window is the signal used to
  consider an old registration stale
- The router should not require PID tracking for this rule

This keeps the logical identity stable while still allowing network recovery.

### 2.4 Peer Identity and Routing

Current candidate:

- `peer_id` is the only stable public identity
- No separate `service_id` is required in the core model
- Calls target a specific `peer_id`
- The router does not maintain a function registry
- The router does not validate function names or method availability

### 2.5 Capability Validation

Current direction:

- The router should not track callable functions
- Function name validation belongs to the worker/library side
- If a worker cannot parse a call body or does not implement a requested
  function, it returns a structured application/library error
- The common denominator should remain strict enough for typed languages

### 2.6 Spawn Helper Scope

The helper is separate from the core router contract.

Current direction:

- The spawn helper may start a process, keep it alive, and restart it if
  configured to do so
- The spawn helper may observe heartbeat loss for restart decisions
- The spawn helper does not change the routing contract
- `connect` remains a separate operational path and is not absorbed into the
  router
- The router itself does not issue shutdown commands
- Process termination remains the responsibility of the helper or the process
  supervisor that launched the peer

### 3. Transport Layer

The transport layer should become swappable.

Candidates:

- WebSocket for broad language support and easy client availability
- A binary socket transport if it proves simpler for the router binary

The key point is that the transport is an implementation detail. The public API
should not care whether the wire is WebSocket, raw TCP, or another compatible
pipe.

### 3.1 Router Implementation Candidate

Preferred implementation choice: `Rust`

Reasoning:

- The router is stateful and correctness-sensitive
- Tokio fits the async control plane well
- Rust makes message framing and routing state explicit
- A single static binary is a natural deployment fit

Go remains a valid alternative if the team later prefers faster iteration or
stronger familiarity, but for this project Rust is the better default because it
aligns with the desire for a strict, low-level, single-binary router.

## v5 Design Principles

- Preserve the developer mental model
- Keep msgpack as the payload format
- Keep the router binary small and focused
- Make routing explicit and inspectable
- Avoid forcing every language to reimplement transport behavior from scratch
- Keep v4 behavior intact on the stable branch

## Rewrite Sequence

### Phase 1: Freeze v4

- Keep the existing ZeroMQ-based implementation stable
- Treat v4 as the compatibility line
- Do not break the existing wire protocol or public API in v4

### Phase 2: Define v5 Spec

- Write the new envelope/body framing
- Define router responsibilities
- Define how peers and peer-to-peer calls behave
- Define migration rules and compatibility boundaries

### Phase 3: Build the Router Binary

- Implement the central router first
- Add peer registration and routing table management
- Add liveness and heartbeat support
- Add selective broadcast only after direct request-response works

### Phase 4: Port One Language End-to-End

- Pick one language as the reference implementation
- Rewrite its transport adapter and runtime integration first
- Use this port to validate the new protocol and router behavior

### Phase 5: Add Conformance Tests

- Cross-language request/response round-trips
- Unknown route and missing peer cases
- Heartbeat and timeout cases
- Peer disconnect and reconnect behavior
- Binary framing compatibility tests

### Phase 6: Port Remaining Languages

- Rewrite each language one by one
- Keep the public API shape as close as possible
- Let each language use its idiomatic async/sync model internally

## Compatibility Plan

The goal is not to redesign the user-facing library from scratch.

Instead:

- Keep the naming and call flow familiar
- Change the internals enough to support the shared bus model
- Allow v4 and v5 to coexist during migration
- Avoid forcing a “big bang” rewrite across all languages

## Open Questions

- No reconnect policy remains open: WebSocket close evicts immediately, and a
  reconnect is just a fresh `register` with the same `peer_id`.
- Which language should become the first reference v5 implementation?

## Short Version

v5 is a brokered, shared-space rewrite that keeps the public API familiar while
changing the internals into a router-centric architecture. v4 stays stable. v5
gets the new framing, router binary, and language-by-language migration path.
