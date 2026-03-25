# Multifrost Rust FAQ

## What is the Rust v5 API surface?

The main public exports are:

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

## Is Rust async-only?

The caller side is async-native. The service side supports both async and sync
service traits.

## How do I call a service?

Use `connect(...)`, then `Connection.handle()`, then `handle.start().await`.
After that you can call `handle.call(...)` or the `call!` macro.

## How do I define a service?

Implement `ServiceWorker` for async service methods or `SyncServiceWorker` for
blocking service methods. Then run the peer with `run_service(...)` or
`run_service_sync(...)`.

## How does Rust find the router?

The Rust binding bootstraps the router lazily through the shared router
bootstrap helper. The default router port is `9981`, and
`MULTIFROST_ROUTER_PORT` overrides it.

## How do I start a separate service process?

Use `spawn(service_entrypoint, executable)` to start a separate process. The
spawned process receives `MULTIFROST_ENTRYPOINT_PATH` and, if set,
`MULTIFROST_ROUTER_PORT`.

## How does service identity work?

If you do not set an explicit service id in `ServiceContext`, the binding uses a
canonical path-based default derived from the entrypoint or current executable.

## How do queries work?

After the caller handle is started, call `query_peer_exists(...)` or
`query_peer_get(...)` on the handle.

## What should I avoid?

- parent/child naming as the public model
- ZeroMQ assumptions
- direct caller-to-service transport
- file-backed live service registries
