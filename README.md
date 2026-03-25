# Multifrost

Cross-language RPC for local and trusted-network runtimes.

Multifrost exists for the annoying real-world case where one system is better as
many small runtimes, not one giant homogeneous app.

Maybe your API layer is in Node.js, your data tooling is in Python, your hot path
is in Rust, and some platform integration is easiest in Go. You still want one
clean mental model: connect, call a method, get a result, handle an error.

Multifrost v5 gives you that model without forcing you to build REST glue,
invent per-language socket code, or pretend everything should live in one
runtime.

At a high level, Multifrost is awesome because it gives you:

- one shared way for Python, JavaScript, Go, and Rust to talk
- local-feeling remote calls instead of bespoke transport code
- explicit service identity through stable `peer_id` values
- one small router that wires peers together without becoming your app runtime
- a design that works well for local orchestration, tools, agents, and trusted internal systems

Multifrost v5 is a router-based system. Python, JavaScript, Go, and Rust peers
connect to the same shared router and call named services as if they were local
functions.

The router exists only to wire peers together:

- service peers register a stable `peer_id`
- caller peers issue calls and receive responses
- the router tracks live peers and forwards frames by `from` and `to`

It is not the application runtime, not a function registry, and not a load
balancer.

## v5 at a glance

- Router-based architecture instead of parent-child IPC
- Separate `service peer` and `caller peer` roles
- WebSocket transport with binary framing
- MessagePack payloads with a portable cross-language subset
- Optional `spawn(...)` helpers without making process ownership the core model
- One shared protocol shape across Python, JavaScript, Go, and Rust

## What changed in v5

Version 5 is a structural rewrite.

- v4 centered on parent and child workers
- v5 centers on a long-lived shared router
- `connect(...)` joins the network as a caller peer
- `run_service(...)` or equivalent joins as a service peer
- `spawn(...)` remains a convenience for starting a process, not a protocol role

The goal is to keep remote calls feeling local while making cross-language
interop and routing much simpler to reason about.

## High-level diagram

```text
  Python caller        Node caller         Rust caller
       |                   |                   |
       +--------- connect/register -----------+
                           |
                    +-------------+
                    |   Router    |
                    | tracks live |
                    | peer_ids    |
                    | routes call |
                    +-------------+
                      /    |    \
                     /     |     \
                    /      |      \
          Python service  Go service  Rust service
             peer_id        peer_id      peer_id
```

Every peer speaks the same routing model. Callers send work to a target
`peer_id`. Services receive calls for their own `peer_id` and send back a
response or structured error.

## Tiny pseudocode

```text
service starts:
  connect to router
  register(peer_id="math-service", class="service")
  wait for calls

caller starts:
  connect to router
  register(peer_id="caller-123", class="caller")
  send call(to="math-service", function="add", args=[10, 20])
  wait for response

router:
  receive frame
  look up destination peer_id
  forward frame unchanged except for transport delivery

service:
  run add(10, 20)
  send response(result=30)

caller:
  receive result
  return 30 to user code
```

## Core model

1. A service process starts and registers a `peer_id`.
2. A caller process connects to the router.
3. The caller uses a handle and issues `handle.call.<method>(...)`.
4. The router forwards the request to the target service peer.
5. The service returns either a response or a structured error.

One process may host many caller peers, many service peers, or both.

## Router

The router is a small shared runtime dependency.

- default port: `9981`
- port override: `MULTIFROST_ROUTER_PORT`
- default log path: `~/.multifrost/router.log`
- default bootstrap lock: `~/.multifrost/router.lock`

In normal usage you usually do not build it manually unless you are developing
the repo or packaging the router yourself. Language runtimes can bootstrap it
when needed, and production setups can ship the router binary directly.

See `router/README.md` for operator notes.

## Why this model is useful

- `Cross-language by default`: each runtime can do the job it is best at
- `Small mental surface`: caller peers call, service peers serve, router routes
- `Less transport pain`: the routing contract is shared instead of re-invented per language pair
- `Operationally cleaner`: services can exist independently of who started them
- `Scales better than parent-child assumptions`: many callers and many services can coexist on one router

## Packages

- `python/` - Python v5 binding
- `javascript/` - Node.js / TypeScript v5 binding
- `golang/` - Go v5 binding
- `rust/` - Rust v5 binding
- `router/` - standalone v5 router binary

## Quick examples

### Python caller

```python
import asyncio
from multifrost import connect


async def main() -> None:
    connection = connect("math-service")
    handle = connection.handle()

    await handle.start()
    try:
        result = await handle.call.add(10, 20)
        print(result)
    finally:
        await handle.stop()


asyncio.run(main())
```

### Python service

```python
import asyncio
from multifrost import ServiceContext, ServiceWorker, run_service


class MathService(ServiceWorker):
    def add(self, a: int, b: int) -> int:
        return a + b


async def main() -> None:
    await run_service(MathService(), ServiceContext(peer_id="math-service"))


asyncio.run(main())
```

For language-specific examples, see:

- `python/docs/quick-examples.md`
- `javascript/docs/quick-examples.md`
- `golang/docs/quick-examples.md`
- `rust/docs/quick-examples.md`

## Canon docs

- `docs/product.md` - product posture and system shape
- `docs/spec.md` - behavioral contract
- `docs/arch.md` - shared architecture
- `docs/arch_index.md` - guide to the split architecture docs
- `docs/arch_router.md` - router implementation architecture
- `docs/glossary.md` - core v5 terms
- `docs/msgpack_interop.md` - portable wire-value rules

## More docs

- `router/README.md` - router operator notes
- `javascript/README.md` - Node.js package overview
- `CHANGELOG.md` - release history and the v5 rewrite summary

## Status

The repo is in the v5 rewrite/finalization phase. The core router model and the
language bindings exist, and the remaining work is release hardening: docs,
cleanup, consistency checks, and final release gates.
