# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
