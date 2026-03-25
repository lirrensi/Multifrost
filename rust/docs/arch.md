# Rust Multifrost Architecture

This document describes the current Rust binding for Multifrost v5.

## Overview

The Rust crate is a router-based, async-first IPC binding. Caller peers connect
to a shared router, service peers register with the same router, and all normal
traffic moves through the router over WebSocket.

The public crate surface is v5-oriented:

- `connect`
- `Connection`
- `Handle`
- `spawn`
- `ServiceProcess`
- `ServiceWorker`
- `SyncServiceWorker`
- `ServiceContext`
- `run_service`
- `run_service_sync`
- `call!`

The internal module names are partly legacy:

- `parent.rs` owns caller configuration and live caller transport
- `child.rs` owns the service runtime and method dispatch
- `process.rs` owns spawned service processes
- `router_bootstrap.rs` owns router reachability and startup
- `message.rs` owns protocol types and framing
- `caller_transport.rs` owns the live caller transport implementation

Those internal names are implementation detail. The crate root re-exports the
v5 surface that consumers should use.

## Scope Boundary

**Owns**: caller configuration, live caller handles, service runtime, spawn
process wrappers, router bootstrap, protocol types, frame encoding and
decoding, and the Rust-specific public API shape.

**Does not own**: the router implementation itself, application-level business
logic, or any direct peer-to-peer transport outside the router.

**Boundary interfaces**: `docs/spec.md` defines the v5 behavior contract; this
document describes how the Rust crate realizes that contract.

## Shape Of The Crate

The crate is organized around two live peer roles.

### Caller Side

- `connect(target_peer_id, timeout_ms)` creates caller configuration
- `Connection.handle()` creates the live caller handle
- `Handle.start().await` boots or connects the router, registers the caller,
  and activates the transport
- `Handle.call(...)` and `call!(...)` issue remote calls
- `Handle.query_peer_exists(...)` and `Handle.query_peer_get(...)` query the
  router directly
- `Handle.stop().await` disconnects and closes the transport

### Service Side

- `ServiceWorker` defines the async service dispatch trait
- `SyncServiceWorker` provides a sync convenience trait for service methods
- `ServiceContext` carries service identity and bootstrap options
- `run_service(...)` starts the async service runtime
- `run_service_sync(...)` runs the same service runtime behind a blocking
  wrapper
- `spawn(...)` starts a separate service process and returns a `ServiceProcess`

The Rust caller side is async-native. The service side can be async or sync
depending on which service trait is implemented.

## Module Map

```text
src/
  lib.rs               crate root and public re-exports
  parent.rs            caller config, connection, and live handle
  child.rs             service runtime and dispatch
  process.rs           spawned service process wrapper
  router_bootstrap.rs  router reachability, lock handling, and startup
  caller_transport.rs  live caller transport and request correlation
  message.rs           protocol constants, frame types, and wire values
  call_macro.rs        ergonomic `call!` macro helpers
  error.rs             crate error types
  logging.rs           structured logging helpers
  metrics.rs           runtime metrics
```

## Caller Flow

1. Code calls `connect("math-service", timeout_ms)`.
2. The caller creates a `Connection` and then a `Handle`.
3. `Handle.start().await` resolves the router endpoint and bootstraps the router
   if needed.
4. The caller registers as class `caller`.
5. The caller issues calls through `handle.call(...)` or `call!(...)`.
6. The caller may query the router with `query_peer_exists(...)` or
   `query_peer_get(...)`.
7. `Handle.stop().await` sends a graceful disconnect and closes the transport.

### Request Correlation

The caller transport keeps a pending-request map keyed by `msg_id`. Matching
`response` or `error` frames resolve or reject the corresponding request.

## Service Flow

1. Code constructs a `ServiceContext`.
2. `run_service(...)` or `run_service_sync(...)` resolves the service peer id.
3. The service runtime bootstraps the router if needed.
4. The service registers as class `service`.
5. Inbound `call` frames are decoded and dispatched to the service worker.
6. Public methods are invoked by name.
7. Results return as `response` frames; failures return as `error` frames.

Service peer ids default to a canonical path when not explicitly provided. The
context may also carry an explicit service id and an entrypoint path override.

## Transport Rules

- WebSocket is the only live transport
- Frames are binary
- The frame layout is `[ u32 envelope_len ][ envelope_bytes ][ body_bytes ]`
- The router-owned envelope is msgpack-encoded
- The body bytes are preserved until application decoding
- The router bootstrap flow is shared with the other v5 bindings

## Bootstrap Behavior

Rust uses peer-driven bootstrap coordination.

1. The peer checks whether the router is already reachable.
2. If not, it acquires a shared lock file under `~/.multifrost/router.lock`.
3. After the lock is held, the peer re-checks reachability.
4. If the router is still unreachable, the peer starts the router process.
5. The peer waits for router readiness up to a finite timeout.
6. The lock is released when the router is reachable or startup fails.

The router log path is `~/.multifrost/router.log`. The port defaults to `9981`
and may be overridden with `MULTIFROST_ROUTER_PORT`.

## Data Model Notes

The Rust wire types mirror the shared v5 protocol:

- `Envelope`
- `RegisterBody`
- `RegisterAckBody`
- `QueryBody`
- `QueryExistsResponseBody`
- `QueryGetResponseBody`
- `CallBody`
- `ResponseBody`
- `ErrorBody`
- `FrameParts`

Numeric payloads follow the portable subset defined in `docs/spec.md`.

## What This Crate Does Not Do

- no ZeroMQ transport
- no parent/child IPC topology
- no direct caller-to-service socket
- no file-backed live service registry
- no reflection-based public dispatch contract

## Key References

- `../docs/spec.md`
- `../docs/product.md`
- `../docs/msgpack_interop.md`
- `../src/lib.rs`
- `../src/parent.rs`
- `../src/child.rs`
