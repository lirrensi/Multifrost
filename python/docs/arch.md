# Multifrost Python Architecture

The Python binding is a router-based v5 client and service library built around one async core.

## Design Summary

- `connect(...)` returns caller configuration only.
- `Connection.handle()` creates the async runtime handle.
- `Connection.handle_sync()` creates the sync convenience handle.
- `spawn(...)` starts a service process, but it does not connect the caller.
- `ServiceWorker` is only a method host.
- `run_service(...)` and `run_service_sync(...)` start the service side.
- The only live transport is WebSocket.
- The router is discovered or bootstrapped lazily through the shared bootstrap helper.

## Public Modules

```text
multifrost/
├── __init__.py
├── connection.py
├── errors.py
├── frame.py
├── process.py
├── protocol.py
├── router_bootstrap.py
├── service.py
├── sync.py
└── transport.py
```

## Caller Flow

1. `connect("math-service")` builds caller configuration.
2. `Connection.handle()` or `Connection.handle_sync()` creates the runtime handle.
3. `handle.start()` bootstraps the router if needed.
4. The caller registers as class `caller`.
5. `handle.call.<function>(...)` sends a `call` body over the router.
6. `handle.stop()` sends `disconnect` when possible and closes the WebSocket.

The synchronous handle is just a background event-loop wrapper around the same async core.

## Service Flow

1. `ServiceWorker` defines public methods.
2. `run_service(...)` resolves the service peer id.
3. The service connects to the router and registers as class `service`.
4. Incoming `call` frames are dispatched by public method lookup.
5. Synchronous methods run directly.
6. Async methods are awaited in the same event loop.

Private method names are rejected, and the service runner does not use a separate child-loop thread.

## Wire Model

The Python binding follows the shared v5 frame format:

```text
[ u32 envelope_len ][ envelope_bytes ][ body_bytes ]
```

Envelope and body payloads use msgpack. The router owns routing metadata, while the body bytes are preserved until the service dispatch layer reads them.

## Error Model

- `TransportError` covers websocket and framing failures.
- `BootstrapError` covers router startup failures.
- `RegistrationError` covers register ack failures.
- `RouterError` covers router-originated failures.
- `RemoteCallError` covers service/application failures.

## Working Rules

- Keep the async core single-sourced.
- Treat `HandleSync` as convenience, not as a second transport.
- Keep public names aligned with the frozen v5 API contract.
- Avoid reintroducing the retired v4 worker-centric surface or file-based service discovery.
