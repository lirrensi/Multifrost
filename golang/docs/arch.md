# Multifrost Go v5 Architecture

This document describes the Go binding for the Multifrost v5 router-based IPC runtime.

## Overview

Go is the synchronous, context-driven binding.

- Callers use `Connect`, `Connection`, and `Handle`.
- Services use `Spawn`, `ServiceProcess`, `ServiceContext`, `ServiceWorker`, and `RunService`.
- The shared router handles presence, routing, and peer identity.
- Transport is WebSocket only.
- Frames are binary msgpack envelopes followed by raw body bytes.

## Caller Flow

1. `Connect(targetPeerID)` returns caller configuration.
2. `Connection.Handle()` creates a live caller handle.
3. `Handle.Start(ctx)` bootstraps or connects the router.
4. The handle registers as class `caller`.
5. `Handle.Call(ctx, function, args...)` sends a blocking call to the target service peer.
6. `Handle.Stop(ctx)` disconnects and closes the socket.

The caller handle owns live network state. The connection object only stores configuration.

## Service Flow

1. `Spawn` starts a service process if you want a separate child process.
2. The service process receives `MULTIFROST_ENTRYPOINT_PATH` and optional router port override.
3. `RunService(ctx, worker, serviceContext)` bootstraps or connects the router.
4. The service registers as class `service`.
5. Incoming `call` frames are decoded and dispatched through `ServiceWorker.HandleCall`.
6. Results are returned as `response` frames; failures are returned as `error` frames.

Service dispatch is explicit. Reflection is not the public model.

## Wire Format

The Go binding follows the canonical v5 frame layout:

```text
[ u32 envelope_len ][ envelope_bytes ][ body_bytes ]
```

- `envelope_bytes` are msgpack-encoded routing metadata.
- `body_bytes` are preserved as raw bytes until the caller or service explicitly decodes them.
- The envelope must include `v`, `kind`, `msg_id`, `from`, `to`, and `ts`.

## Bootstrap

Go shares the router bootstrap behavior with the other v5 bindings.

- Router port defaults to `9981`.
- The port override env var is `MULTIFROST_ROUTER_PORT`.
- Router log path is `~/.multifrost/router.log`.
- Router lock path is `~/.multifrost/router.lock`.
- `MULTIFROST_ROUTER_BIN` overrides the router binary path.
- Reachability is checked before startup work begins.
- The shared lock is acquired only after the first reachability failure.
- The router is spawned only if the endpoint is still unreachable after the lock is held.

## Concurrency

Go uses goroutines internally to keep the transport responsive.

- One websocket per live caller handle or live service runner.
- A single pending-request map tracks in-flight requests by `msg_id`.
- The public API stays synchronous and context-driven.
- Go does not expose async terminology.

## Dispatch

Caller-side methods decode router responses explicitly:

- `response` frames carry successful results.
- `error` frames carry router or remote error bodies.
- `peer.exists` and `peer.get` are router queries.

Service-side handlers should switch on `function` inside `HandleCall`.

## Non-Goals

- No retired v4 worker/proxy surface.
- No service registry files.
- No reflection-based public dispatch contract.
