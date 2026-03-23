# Multifrost Product Canon

Status: Canonical
Version: v5 direction

## Overview

Multifrost is a cross-language RPC fabric for local and trusted-network runtimes.
It lets separately running code talk to named services as if they were local
functions, without forcing callers to manage direct sockets, ports, or bespoke
transport glue.

Version 5 changes the product shape in an important way: Multifrost is no longer
defined around a parent owning a child worker. It is defined around a shared
router and two explicit runtime classes:

- `service peer`: exposes callable functions under a unique `peer_id`
- `caller peer`: issues calls and receives responses under its own `peer_id`

Both connect to the same router. The router is the shared space. Callers do not
need a direct relationship with services, and services do not need to know who
started them.

The developer experience should still feel familiar: connect, call a method,
get a result, handle an error.

## Core Capabilities

- Cross-language request/response calls through one shared router
- Named service addressing through stable `peer_id` values
- Separate caller and service identities with the same routing model
- Router-backed presence checks before issuing work
- Optional process startup helpers without making process ownership the product
- A WebSocket transport model where connection state is the liveness signal

## Main User Flows

### Run a service

1. A service peer starts in its own process.
2. It determines its `peer_id`.
3. If no explicit `peer_id` is configured, the absolute file path becomes the
   default identity.
4. It attempts to connect to the router immediately.
5. If the router is missing, it may coordinate router bootstrap through a shared
   lock and then continue connecting.
6. Once connected, it registers itself as a live `service` entry and waits for
   incoming calls.

### Call a service

1. A caller peer starts or is created by application code.
2. It gets a `peer_id`, usually random or otherwise ephemeral.
3. It connects to the router.
4. It may query the router to verify that a target service peer exists.
5. It sends a call addressed to the target service's `peer_id`.
6. The router forwards the call and returns the response or error back to the
   caller's own `peer_id`.

### Host many peers in one process

One process may host:

- many service peers,
- many caller peers,
- or both.

These are still separate instances with separate responsibilities and separate
`peer_id` values. A service peer may use one or more caller peers internally,
but that does not merge the two classes.

### Start a worker process optionally

`spawn` remains useful when one process wants to launch and supervise another.
That is an operational helper, not the center of the product model.

`spawn` starts a process. The started process then behaves like any other
service peer: it eagerly connects to the router and may bootstrap the router if
needed. `spawn` itself does not own router startup.

## System Shape

### Router

The router is the shared runtime hub.

It is responsible for:

- accepting peer connections,
- tracking which `peer_id` values are live,
- recording whether a connected entry is a `service` or a `caller`,
- routing requests by `to`,
- routing responses and errors by `from`/`to`,
- answering router-level presence queries,
- using WebSocket connection state as the main liveness signal.

The router is intentionally small. It routes traffic and tracks presence. It is
not the application runtime.

The router is also long-lived. Once started, it is not owned by the peer that
bootstrapped it. If the bootstrapper later exits, the router continues running
until it is explicitly terminated or the host environment cleans it up.

### Service Peers

Service peers are the callable endpoints of the system.

They:

- expose functions,
- own stable identities,
- receive calls addressed to their own `peer_id`,
- return results or structured errors.

### Caller Peers

Caller peers are the outbound side of the system.

They:

- issue calls,
- receive responses and errors back on their own `peer_id`,
- may be short-lived or long-lived,
- are not considered callable services by default.

### Shared Identity Model

Service peers and caller peers use the same routing idea:

- every peer has a `peer_id`,
- every routed message has `from` and `to`,
- the router tracks both identity and peer class.

The important difference is semantic, not structural: service peers are public
call targets; caller peers are routing endpoints for outbound work and return
paths.

One process may host many peer instances. The process itself is not "the peer."
The individual service-peer and caller-peer instances are the peers.

### Router Bootstrap

The router is a shared local runtime dependency. It may already exist, or it may
need to be started by the first participant that notices it is absent.

Router bootstrap is coordinated by peers themselves:

- any service peer or caller peer may attempt bootstrap if connect fails,
- bootstrap is serialized with a shared OS-level lock,
- the peer holding the lock re-checks router reachability before starting a new
  router,
- once the router is reachable, normal registration and traffic continue.

This keeps startup race-safe without making router ownership part of the caller
or service relationship.

Bootstrap does not imply ownership. The peer that happened to start the router
does not gain special lifecycle authority over it.

## Design Principles

- Preserve the feeling of local method calls
- Make the router the single shared routing point
- Keep service peers and caller peers explicit and separate
- Keep routing metadata small and inspectable
- Use WebSocket connection state as the primary health signal
- Keep process management helpers outside the core network model
- Let one process host many independent peer instances

## Non-Goals

Multifrost v5 is not trying to be:

- a direct peer-to-peer mesh as the default model,
- a router that validates application function names or argument shapes,
- a built-in load balancer,
- a parent-child ownership protocol,
- a PID-based correctness model,
- a guarantee of exactly-once delivery,
- a public-internet security layer by itself.

## Migration Posture

v5 is a clean directional break from the v4 mental model.

What remains familiar:

- remote calls still feel local,
- handles and call-oriented APIs remain the intended developer experience,
- explicit service identities remain central.

What changes:

- the core model is router-based rather than direct parent-child IPC,
- `connect` is the canonical way peers join the network,
- `spawn` is only a helper,
- service presence is discovered through the router,
- service and caller responsibilities are separated explicitly.

v5 is intentionally incompatible with v4. The protocol key and runtime model are
separate, and coexistence during migration is an operational concern rather than
a compatibility promise.
