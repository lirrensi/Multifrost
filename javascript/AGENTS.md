# Multifrost JavaScript / TypeScript v5 Guide

## Architecture

The JavaScript package is a router-based v5 binding. Callers and services are separate runtime roles that communicate only through the shared router over WebSocket.

### Primary Modules

- `src/protocol.ts` for protocol constants, wire types, and envelope validation
- `src/frame.ts` for msgpack encoding and the `[u32][envelope][body]` frame layout
- `src/errors.ts` for the stable v5 error classes
- `src/router_bootstrap.ts` for router reachability, lock handling, and startup
- `src/transport.ts` for the live WebSocket transport
- `src/connection.ts` for caller configuration and the live `Handle`
- `src/process.ts` for service process launching
- `src/service.ts` for `ServiceWorker`, `ServiceContext`, and `runService`
- `src/index.ts` for package entrypoint exports

## Working Rules

- Keep the Node surface async-only.
- Do not reintroduce parent/child naming, ZeroMQ sockets, or direct peer-to-peer transport.
- Keep `connect(...)` as caller configuration and `spawn(...)` as process launch only.
- Keep `runService(...)` responsible for the service peer registration and dispatch loop.
- Preserve binary frame handling and `msg_id` correlation.
- When editing public files, make sure the README, examples, and docs continue to teach the v5 names.

## Testing

Use the package-level commands:

```bash
npm run typecheck
npm test
```

For examples, prefer `npx tsx` so the scripts stay close to the source files while the package is under rewrite.

## Style

- Prefer small, focused modules.
- Use TypeScript strictness and explicit public types.
- Keep error handling explicit and surface router-vs-service failures distinctly.
- Avoid adding compatibility aliases for retired v4 names.

## Non-Goals

- no ZeroMQ
- no synchronous API
- no `ParentWorker` / `ParentHandle` / `ChildWorker`
- no direct caller-to-service transport
