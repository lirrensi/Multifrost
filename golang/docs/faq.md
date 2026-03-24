# Multifrost Go FAQ

## Is Go async-first?

No. Go is the synchronous, context-driven binding. Concurrency comes from goroutines around blocking calls.

## What are the main Go entry points?

- `Connect` for caller configuration.
- `Connection.Handle()` for the live caller runtime.
- `Handle.Call` for blocking remote calls.
- `Spawn` for starting a service process.
- `RunService` for registering and serving a service peer.

## Does Go expose `async` naming?

No. The public API stays idiomatic for Go and does not use async terminology.

## How do services dispatch calls?

Service code implements one method:

```go
HandleCall(ctx context.Context, function string, args []any) (any, error)
```

The implementation should switch on the function name explicitly.

## How do I run a service in another process?

Use `Spawn` to start the child process, then run the service entrypoint with `RunService`. The child process receives `MULTIFROST_ENTRYPOINT_PATH` and, if configured, `MULTIFROST_ROUTER_PORT`.

## How do I call a service?

Create a `Connection`, call `Handle()`, then call `Start(ctx)` and `Call(ctx, function, args...)`.

## How does routing work?

Peers register with the shared router over WebSocket. The router keeps live presence in memory and forwards binary frames by `peer_id`.

## What happens when the websocket closes?

The transport is marked dead immediately. Pending requests fail, and the handle or service runner should stop.

## How do errors work?

- Router and protocol problems come back as router-originated errors.
- Application failures in `HandleCall` come back as remote call errors.
- Registration failures are reported before the handle or service becomes active.

## What should I avoid?

- the retired v4 worker/proxy surface
- reflection as the public dispatch model
- service registry files
