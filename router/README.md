# Multifrost Router

The Multifrost router is the central wiring point for v5.

Its job is intentionally small:

- accept peer connections
- track live `peer_id` registrations
- know whether a peer is a `service` or a `caller`
- route requests, responses, and errors by envelope identity
- answer router-level presence queries

It does not run application logic, validate function names, or act as a load
balancer.

## Runtime defaults

- transport: WebSocket
- default port: `9981`
- port override env var: `MULTIFROST_ROUTER_PORT`
- default log path: `~/.multifrost/router.log`
- default lock path: `~/.multifrost/router.lock`

## Operational model

- The router is long-lived.
- It is not owned by the peer that bootstrapped it.
- If the bootstrapper exits, the router may keep serving other peers.
- Peers use WebSocket connection state as the primary liveness signal.

In development, runtimes may bootstrap the router lazily when they discover it
is missing. In packaged or production-like setups, the router binary can be
distributed and launched directly.

## Get the binary

Typical usage is to ship or download the router binary rather than building it
inside every application repository clone.

When working in this repo, you can build it locally.

## Local development commands

Build the router:

```bash
cargo build --bin multifrost-router
```

Run on the default port:

```bash
cargo run --bin multifrost-router
```

Run on a custom port:

```bash
MULTIFROST_ROUTER_PORT=20080 cargo run --bin multifrost-router
```

Run the router integration tests:

```bash
cargo test --test router_integration -- --nocapture
```

## What peers expect

Language bindings assume:

- protocol key `multifrost_ipc_v5`
- WebSocket transport only
- binary frame layout with routing envelope plus MessagePack body
- immediate registration ack or rejection
- immediate disconnect invalidation when a socket closes

## Related docs

- `../docs/spec.md`
- `../docs/arch.md`
- `../docs/arch_router.md`
- `../README.md`
