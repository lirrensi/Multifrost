# Rust Multifrost AGENTS Guide

## Overview

The Rust crate is the async-native v5 binding for Multifrost. The public surface
is router-based and uses the crate root exports:

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

## Working Rules

- Keep the Rust binding router-based and v5-shaped.
- Do not reintroduce ZeroMQ, parent/child IPC, or direct peer-to-peer transport.
- Keep caller code async.
- Keep service code on `ServiceWorker` or `SyncServiceWorker`.
- Keep `spawn(...)` as a process launcher only.
- Keep `Connection.handle()` as the live caller runtime.
- Keep `run_service(...)` and `run_service_sync(...)` responsible for service
  registration and dispatch.
- Preserve binary framing and `msg_id` correlation.

## Important Modules

- `src/lib.rs` for crate re-exports
- `src/parent.rs` for caller configuration and live handles
- `src/child.rs` for service runtime and dispatch
- `src/process.rs` for spawned service processes
- `src/router_bootstrap.rs` for router reachability and startup
- `src/message.rs` for protocol types and framing

## Validation

```bash
cargo build
cargo test
cargo fmt
cargo clippy
```

## Style

- Keep docs aligned with the v5 API surface, not the retired parent/child model.
- Prefer explicit error handling.
- Keep public examples short and close to the actual crate exports.
