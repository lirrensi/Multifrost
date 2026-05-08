# Adding a Language Binding

Status: Canonical
Scope: Protocol for adding a new v5 language binding to Multifrost

## Purpose

This document is the checklist for adding a new language binding. It defines
the minimum surface every binding MUST provide to be considered complete, plus
optional extensions. When you want to add a language, start here.

## Minimum Viable Surface (Caller Peer)

Every binding MUST support the **caller peer** role as a minimum. This is the
entry point — a caller connects to the router, issues calls to a service peer,
and receives responses. Without this, the language cannot participate in the
system at all.

### Canonical Source Modules

These files are required. Names follow language convention, but the
responsibilities are invariant.

| Module | Responsibility | Must implement |
|---|---|---|
| **Protocol** | Wire constants and structs | `ProtocolKey`, `ProtocolVersion`, `RouterPeerID`, `Kind*` constants, `PeerClass`, `Envelope`, `RegisterBody`, `RegisterAckBody`, `QueryBody`, `CallBody`, `ResponseBody`, `ErrorBody`, `ValidateEnvelope()`, `NewEnvelope()`, `nowTS()`, `newMsgID()` |
| **Frame** | Binary frame codec + MsgPack | `encodeMsgpack()`, `decodeMsgpack()`, `encodeFrame()` / `decodeFrame()` — layout: `[u32 envelope_len][msgpack envelope_bytes][msgpack body_bytes]` |
| **Errors** | Typed error model | `ErrorOrigin` variants, `TransportError`, `BootstrapError`, `RegistrationError`, `RouterError`, `RemoteCallError`, `ErrorFromWire()` |
| **Transport** | WebSocket dial, register, read/write | `dialPeerTransport()` — opens WS, sends `register`, waits for ack. `request()` — sends frame, waits for response on `msg_id` (pending map). `send()`, `call()`, `queryExists()`, `queryGet()`, `disconnect()`, `readLoop()` (routes incoming frames to pending map or dispatch) |
| **Router Bootstrap** | Router startup coordination | `ensureRouter()` — checks reachability, acquires OS lock, double-checks, spawns router process if absent, polls until ready, releases lock. `routerPortFromEnv()`, `routerEndpoint()` |
| **Connection** | Public caller API | `ConnectOptions`, `connect()`, `Connection`, `Handle` — `start()` (ensures router, dials, registers, optionally validates target), `stop()`, `call(function, ...args)` (blocking or async per language), `queryPeerExists()`, `queryPeerGet()` |
| **Package entrypoint** | Public exports | Re-exports everything callers need: `connect`, `Connection`, `Handle`, `ConnectOptions`, error types |

### Package Infrastructure

| Artifact | Purpose |
|---|---|
| Package manifest | Declares dependencies, version, autoloading. E.g. `composer.json`, `pyproject.toml`, `package.json`, `go.mod`, `Cargo.toml` |
| Lock file | Pinned dependency versions |
| Lint config | Language-standard linter/formatter config |

### Documentation (per `{lang}/docs/`)

| File | Content |
|---|---|
| `arch.md` | Language-specific architecture: design decisions, deviations from other bindings, paradigm notes (sync/async), module map |
| `faq.md` | Answers to the first 5-10 questions a user of this language would ask |
| `quick-examples.md` | Runnable examples: caller connecting and calling, service running (if implemented), spawn (if implemented) |

### Agent Guide

| File | Content |
|---|---|
| `AGENTS.md` | Build/test/lint commands. Architecture rules. Coding rules. Example patterns that mirror the public API shape. Non-goals (what NOT to reintroduce from v4) |

### Examples

Minimum examples directory:

| File | Pattern shown |
|---|---|
| `math_caller.{ext}` | Caller: connect to `math-service`, call `add(1,2)`, print result |
| `{lang}_calls_{other}.{ext}` | Cross-language: caller in this language calls a service in another language |

### Tests

Minimum test files:

| File | Covers |
|---|---|
| Frame encode/decode | Round-trip preserves body bytes. Short/malformed frame rejection. Envelope validation. |
| Transport + registration | Register ack. Duplicate peer ID rejection. Call/response round-trip. Query exists/get. Disconnect invalidation. |
| Bootstrap | Lock acquisition/release. Stale lock handling. Router startup timeout. |
| Interop | This language's caller calling a Rust service, and vice versa |

### Repo-Level Artifacts to Update

When adding a new language, these files in the repo root MUST be updated:

| File | Change |
|---|---|
| `docs/support_matrix.md` | Add column to the v5 surface table. Add a "Binding Shapes" section following the existing pattern. |
| `docs/CODEMAP.L1.md` | Add the new language's file tree section. |
| `docs/CODEMAP.L2.md` | Add the new language's signatures section (if maintained). |
| `docs/arch.md` | Add the new language directory to the architecture map at the end. |
| `README.md` | Add the language directory to the packages list. Add language to the "one shared way" sentence. Add link to `{lang}/docs/quick-examples.md`. |
| `CHANGELOG.md` | Entry describing the new binding and its key dependencies/paradigm. |
| `e2e/workers/math_worker.{ext}` | A math worker in the new language implementing the standard math methods (add, multiply, divide, factorial, fibonacci, echo, get_info, throw_error). Used by E2E cross-language tests. |
| `e2e/v5/tests/test_matrix.py` | Add service and caller process commands for the new language. Wire into the `_run_four_service_matrix()` (or equivalent) test. |

## Full Surface (Service Peer)

Once the caller peer is working, the binding SHOULD add service peer support.

| Module | Responsibility |
|---|---|
| **Service** | `ServiceWorker` (interface/trait — `handleCall(function, args)`), `ServiceContext` (peer_id, entrypoint path), `runService()` (register as service, dispatch loop) |
| **Process** | `Spawn()` — launches a service process. `ServiceProcess` — owns process lifecycle (start/stop/wait). Must NOT own network state. |

Service peer conformance rules are in `docs/spec.md` §Service Peer Conformance.

## Optional Extensions

These exist in some bindings but are NOT required:

| Feature | Present in |
|---|---|
| Sync wrapper over async core | Python (`HandleSync`) |
| `call!` macro for ergonomic calling | Rust |
| Logging subsystem | JS, Rust |
| Metrics subsystem | JS, Rust |
| Circuit breaker / retry | JS, Rust |
| Heartbeat telemetry | JS, Rust |

## Suggested Implementation Order

1. Package manifest + directory scaffold
2. `Protocol` (zero deps beyond MsgPack)
3. `Frame` (depends on Protocol)
4. `Errors` (zero deps)
5. `Transport` (depends on Frame, Errors, Protocol, WebSocket library)
6. `RouterBootstrap` (depends on Transport/lock primitives)
7. `Connection` (depends on Transport, RouterBootstrap)
8. Package entrypoint
9. `Service` (depends on Transport, Protocol, Frame, Errors)
10. `Process` (depends on RouterBootstrap)
11. Examples
12. Tests
13. Language docs (`arch.md`, `faq.md`, `quick-examples.md`)
14. `AGENTS.md`
15. Repo-level artifact updates (support_matrix, CODEMAP, README, CHANGELOG, e2e)
16. CI pipeline integration

## Dependency Rules

- The shared router (`router/`) is the only component allowed to be implemented
  in exactly one language. Everything else must be implementable in any language.
- Bindings MUST NOT talk to each other directly. All traffic goes through the
  router.
- Bindings MUST use the same protocol key `multifrost_ipc_v5`, default port
  `9981`, and WebSocket transport.
- MsgPack is the required payload encoding. The binding MUST properly distinguish
  str/bin/map/array types per `docs/msgpack_interop.md`.
- The binary frame layout `[u32 envelope_len][msgpack envelope][msgpack body]`
  is invariant across all bindings.
- The `register`-first connection rule is invariant. No traffic before
  registration is accepted.

## Design Principles

- Preserve the v5 naming: `service`, `caller`, `connect`, `handle`, `run_service`.
  Do not introduce `parent`/`child` terminology as primary names.
- Match the language's natural paradigm. If the language is synchronous, make the
  API synchronous. If async, make it async. Do not force one paradigm on a
  language that naturally uses another.
- Keep the public surface small. The router handles presence, routing, and
  liveness. The binding only needs to connect, register, call, and respond.
- Short-lived callers are valid. The spec says caller peers "MAY be long-lived
  or short-lived." A script that connects, calls, and exits is conforming.
