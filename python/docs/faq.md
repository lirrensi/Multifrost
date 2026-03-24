# Multifrost Python FAQ

## What is the v5 Python API?

The v5 API is built around `connect`, `Connection`, `Handle`, `HandleSync`, `spawn`, `ServiceProcess`, `ServiceWorker`, `ServiceContext`, `run_service`, and `run_service_sync`.

## Do I use async or sync callers?

Use `Handle` when you want native async calls. Use `HandleSync` when you want the same API with blocking behavior.

## Can a service method be synchronous?

Yes. `ServiceWorker` methods may be synchronous or asynchronous public methods.

## How does Python find the router?

The Python binding bootstraps the router lazily through the shared router bootstrap helper. The default port is `9981`, and `MULTIFROST_ROUTER_PORT` overrides it.

## How do I run a named service?

Use `run_service(...)` or `run_service_sync(...)` with a `ServiceContext` that sets `peer_id`, for example `ServiceContext(peer_id="math-service")`.

## How do I start a service process?

Use `spawn(...)` to start the entrypoint process, then connect to the service with `connect(...)`.

## How do I check whether a peer exists?

After `handle.start()`, call `await handle.query_peer_exists("math-service")` or `handle.query_peer_get("math-service")`.

## What should I avoid?

Avoid the retired v4 worker-centric naming and keep the surface aligned with the router-based v5 model.
