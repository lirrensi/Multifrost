# Multifrost JavaScript v5 Architecture

This document describes the Node implementation that matches the v5 router-based spec in `../docs/spec.md`.

## Shape Of The Package

The package is organized around the same v5 concepts as the Rust implementation:

- `connect(...)` creates caller configuration
- `Connection.handle()` creates a live caller handle
- `spawn(...)` launches a service process only
- `ServiceWorker`, `ServiceContext`, and `runService(...)` provide the service-side runtime

The package entrypoint in `src/index.ts` re-exports the canonical v5 surface and the shared protocol/error types.

## Module Map

```text
src/
  protocol.ts       shared protocol constants, wire types, and envelope validation
  frame.ts          msgpack helpers and [u32][envelope][body] framing
  errors.ts         stable v5 error classes
  router_bootstrap.ts  router reachability, lock handling, and startup
  transport.ts      single WebSocket per live peer, binary frame I/O, register handshake
  connection.ts     caller config, live handle, proxy call surface, msg_id correlation
  process.ts        service process launcher and lifecycle wrapper
  service.ts        service runtime, dispatch, and `runService`
  index.ts          package entrypoint
```

## Runtime Model

Node v5 uses one shared router process that routes traffic by peer id. The Node package never creates a direct caller-to-service socket.

### Caller Flow

1. `connect(targetPeerId, options?)` returns configuration only.
2. `Connection.handle()` creates a `Handle`.
3. `await handle.start()` bootstraps the router if needed, opens one WebSocket, sends `register`, and waits for the register ack.
4. `handle.call.<method>(...)` sends `call` frames to the configured target peer.
5. `handle.queryPeerExists(...)` and `handle.queryPeerGet(...)` query the router directly.
6. `await handle.stop()` sends a graceful disconnect when possible and closes the socket.

### Service Flow

1. `spawn(...)` starts a child process only.
2. The service process reads `MULTIFROST_ENTRYPOINT_PATH` and resolves its peer id.
3. `runService(...)` bootstraps the router if needed, opens one WebSocket, sends `register`, and waits for the register ack.
4. Inbound `call` frames are decoded only when they reach the service dispatch layer.
5. Public methods on the `ServiceWorker` subclass are invoked by name.
6. Results return as `response` frames; failures return as `error` frames.

## Transport Rules

- WebSocket is the only live transport.
- Frames are binary only.
- The frame layout is `[ u32 envelope_len ][ envelope_bytes ][ body_bytes ]`.
- Envelope validation happens before outgoing frames are sent.
- The transport waits for the register ack before considering the peer active.
- Incoming frames are routed by kind.
- The WebSocket close event is the liveness signal for the peer.

## Request Correlation

Caller requests are correlated by `msg_id`.

The handle keeps a pending-request map and resolves or rejects each promise when the matching `response` or `error` arrives.

Router-originated errors are mapped to router errors. Service-originated errors are mapped to remote call errors.

## Service Dispatch

Service dispatch is intentionally simple:

- only public method names are callable
- names starting with `_` are ignored
- the base class helpers are not exposed as service methods
- malformed call bodies become application-level errors

## Bootstrap Behavior

`router_bootstrap.ts` mirrors the Rust bootstrap flow:

1. probe reachability first
2. acquire the shared startup lock only after the first failure
3. probe again once the lock is held
4. start the router only if it is still unreachable
5. wait for readiness up to a finite timeout
6. release the lock on both success and failure

## What This Package Does Not Do

- no ZeroMQ transport
- no parent/child IPC topology
- no synchronous handle or synchronous service runner
- no direct caller-to-service socket
- no global service registry as part of the v5 runtime

## Key References

- `../docs/spec.md`
- `../docs/product.md`
- `../docs/msgpack_interop.md`
- `../../rust/src/router_bootstrap.rs`
