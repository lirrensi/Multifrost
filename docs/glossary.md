# Glossary

Status: Canonical
Scope: Shared v5 terminology

## Router

The shared WebSocket runtime hub that registers peers, tracks live presence,
answers router queries, and routes messages by `peer_id`.

## Service Peer

A peer that exposes callable functions and receives inbound `call` traffic
addressed to its own `peer_id`.

## Caller Peer

A peer that issues `call` traffic, may issue router `query` traffic, and
receives routed `response` or `error` traffic addressed back to its own
`peer_id`.

## Peer ID

The string identifier used for routing and presence tracking.

## Envelope

The routing metadata portion of a v5 frame. The router uses it without
inspecting application payload semantics.

## Body

The msgpack-encoded application payload portion of a v5 frame.

## Register

The synchronous registration handshake that a peer sends as the first binary
frame after opening a router connection.

## Query

Router-only control traffic used to inspect live presence, including
`peer.exists` and `peer.get`.

## Call

Application request traffic from a caller peer to a service peer.

## Response

Successful return traffic from a service peer back to the originating caller
peer.

## Error

Failure traffic emitted by the router or a service peer.

## Heartbeat

Optional telemetry traffic for RTT and responsiveness measurement.

## Disconnect

Graceful leave traffic that tells the router to stop routing new traffic to the
peer.
