# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [5.0.0] - 2026-03-25

Version 5 is the largest rewrite in the project so far. Multifrost is no longer
a parent-child IPC library built around direct worker ownership. It is now a
router-based cross-language RPC fabric built around caller peers, service peers,
and a shared long-lived router.

### Added
- **Shared router runtime**: Added the standalone `multifrost-router` binary in `router/` as the central v5 routing component.
- **Router conformance coverage**: Added focused router integration coverage for registration, duplicate rejection, queries, routing, disconnect invalidation, malformed frames, and body preservation.
- **Rust v5 router model**: Added the Rust caller/service runtime around the shared router and WebSocket transport.
- **Python v5 router model**: Added the Python caller/service runtime around the shared router and WebSocket transport.
- **JavaScript v5 router model**: Added the Node.js / TypeScript caller/service runtime around the shared router and WebSocket transport.
- **Go v5 router model**: Added the Go caller/service runtime around the shared router and WebSocket transport.
- **Cross-language matrix harness**: Added the four-caller by four-service v5 matrix harness under `e2e/v5/tests/`.
- **v5 canon docs**: Added and rewrote the v5 canon in `docs/product.md`, `docs/spec.md`, `docs/arch.md`, and `docs/arch_router.md`.
- **MessagePack interop guide**: Added `docs/msgpack_interop.md` as the wire-value reference for the portable cross-language subset.

### Changed
- **Core paradigm**: Replaced the parent/child mental model with explicit `caller peer` and `service peer` roles.
- **Transport model**: Replaced the old direct IPC topology with a shared WebSocket router.
- **Lifecycle model**: Reframed `spawn(...)` as an operational helper instead of a protocol role.
- **Routing model**: Standardized on explicit `peer_id`, `from`, and `to` routing semantics.
- **Framing model**: Standardized on binary framing with a small routing envelope and a MessagePack body.
- **Presence model**: Moved service discovery and peer presence to router queries instead of the old v4 approach.
- **Repository docs**: Began the repo-wide shift from v4 language and examples to the v5 router-based model.

### Removed
- **v4 protocol posture from the active canon**: Retired the old parent-child protocol documentation from the main docs path and moved rewrite-era planning material into `docs/legacy/` where appropriate.
- **v4 as the active implementation target**: The active repo direction is now the v5 router model rather than extending the old topology.

### Notes
- v5 keeps the goal of making remote calls feel local, but the internal architecture is intentionally different from v4.
- Legacy material is retained only where it still helps explain history or migration context.

## [4.0.0] - 2025-02-19

### Changed
- **Unified API across all languages**: Removed the inconsistencies that leaked from Python's sync/async duality.
- **Cleaner external API**: `.spawn()`, `.handle()`, `.start()`, `.stop()` pattern became more consistent everywhere.
- **No more quirks**: Each language expressed its natural paradigm more directly.

### Migration from v3
- Replace `ParentWorker("script.py")` with `ParentWorker.spawn("script.py")`.
- Use `.handle()` for lifecycle management: `handle.start()`, `handle.stop()`.
- Async calls use `await handle.call.methodName(args)`.

## [3.0.0] - Universal Language Support

### Added
- **Go implementation**: Full Go support with goroutine-based concurrency and reflection-based dispatch.
- **Rust implementation**: Full Rust support with tokio async runtime and type-safe trait system.
- **4-language parity**: Python, JavaScript, Go, Rust all speaking the same protocol.

### Changed
- Expanded from Python + Node.js bridge to universal cross-language IPC.

## [2.0.0] - ZeroMQ & The Bridge

### Added
- **ZeroMQ transport**: Replaced STDIN/STDOUT with socket communication.
- **Node.js implementation**: JavaScript/TypeScript support.
- **Python ↔ Node.js bridge**: First cross-language communication.

### Changed
- Moved from simple STDIN/STDOUT to DEALER/ROUTER socket pattern.
- Message serialization switched from JSON to MessagePack.

### Note
> Legacy implementations are retained under `python/legacy/`.

## [1.0.0] - The Beginning

### Added
- **Initial Python package**: Simple, single-file IPC library.
- **STDIN/STDOUT transport**: Parent and child communicated over standard streams.
- **JSON serialization**: Simple JSON messages back and forth.
- **The idea**: Call remote functions like they are local.
