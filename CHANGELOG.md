# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - 2026-03-13

### Added
- **Cross-language end-to-end coverage**: Added broader e2e test coverage for Python, JavaScript, Go, and Rust, including focused numeric boundary checks for JavaScript to Python calls
- **Interop reference docs**: Added and expanded `docs/msgpack_interop.md` and `docs/support_matrix.md` so the portable wire contract is easier to follow
- **More runtime test depth**: Expanded Python, JavaScript, Go, and Rust test coverage around lifecycle, dispatch, registry behavior, heartbeats, and message handling

### Fixed
- **Service registry consistency**: Aligned registry locking behavior across runtimes, including Rust lock support and a shared `services.lock` convention
- **Cross-language heartbeat compatibility**: Standardized heartbeat timestamp handling around `metadata.hb_timestamp` so RTT and health checks behave the same across languages
- **Restart and worker-mode handling**: Fixed Python restart spawning so restarted workers receive the same worker-mode startup contract as the initial spawn path
- **JavaScript numeric interop**: Switched JavaScript MessagePack handling to explicit int64-safe encode/decode behavior and reject unsafe integer inputs instead of silently losing precision
- **Multi-parent output routing**: Improved JavaScript child output tracking so connect-mode workers can forward output more reliably when more than one parent is attached
- **Windows process detection**: Corrected Go service liveness checks on Windows so stale registry entries and running processes are handled more accurately

### Changed
- **Portable numeric contract is now explicit**: The documented safe subset is signed int64 integers, finite floats, UTF-8 strings, binary payloads, arrays, and objects with string keys only
- **Interop expectations are clearer**: The docs now treat all 12 language-pair combinations as the target matrix while calling out the remaining edge cases for precision-sensitive values
- **Examples and e2e workflow improved**: Added dedicated e2e workers, helper scripts, and test targets to make cross-language verification easier to run repeatedly

## [4.0.0] - 2025-02-19

### Changed
- **Unified API across all languages**: Removed the inconsistencies that leaked from Python's sync/async duality
- **Cleaner external API**: `.spawn()`, `.handle()`, `.start()`, `.stop()` pattern now consistent everywhere
- **No more quirks**: Each language now expresses its natural paradigm (async in JS, goroutines in Go, tokio in Rust)

### Migration from v3
- Replace `ParentWorker("script.py")` with `ParentWorker.spawn("script.py")`
- Use `.handle()` for lifecycle management: `handle.start()`, `handle.stop()`
- Async calls use `await handle.call.methodName(args)` pattern

---

## [3.0.0] - Universal Language Support

### Added
- **Go implementation**: Full Go support with goroutine-based concurrency and reflection-based dispatch
- **Rust implementation**: Full Rust support with tokio async runtime and type-safe trait system
- **4-language parity**: Python, JavaScript, Go, Rust all speaking the same protocol

### Changed
- Expanded from Python + Node.js bridge to universal cross-language IPC

---

## [2.0.0] - ZeroMQ & The Bridge

### Added
- **ZeroMQ transport**: Replaced STDIN/STDOUT with proper socket communication
- **Node.js implementation**: JavaScript/TypeScript support
- **Python ↔ Node.js bridge**: First cross-language communication

### Changed
- Moved from simple STDIN/STDOUT to DEALER/ROUTER socket pattern
- Message serialization switched from JSON to msgpack for performance

### Note
> Legacy implementations available in `python/legacy/` and `javascript/legacy/`

---

## [1.0.0] - The Beginning

### Added
- **Initial Python package**: Simple, single-file IPC library
- **STDIN/STDOUT transport**: Parent and child communicated over standard streams
- **JSON serialization**: Simple JSON messages back and forth
- **The idea**: Call remote functions like they're local — no REST, no gRPC, just simple IPC

---

## The Journey

```
v1 ──► v2 ──► v3 ──► v4
│       │       │       │
│       │       │       └── Clean, unified API
│       │       └── Go + Rust join the family
│       └── ZeroMQ + Node.js bridge
└── Simple Python, STDIN/STDOUT, JSON
```

For migration guides and detailed examples, see the [documentation](docs/).
