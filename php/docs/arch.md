# Multifrost PHP Architecture

Status: Canonical
Scope: PHP v5 binding implementation architecture

## Overview

The PHP binding for Multifrost v5 is a **blocking, synchronous** implementation
that mirrors Go's synchronous pattern. PHP's natural execution model (script
runs, does work, exits) aligns with the v5 spec's allowance for short-lived
caller peers.

## Design Decisions

### Blocking by Default

PHP is synchronous by nature. The binding uses blocking WebSocket I/O via
`phrity/websocket`. There is no event loop, no fibers, no async/await. This
matches Go's `Handle.Call()` pattern exactly.

### Short-Lived Callers

A PHP script that connects, calls, and exits is a fully conforming caller peer.
The WebSocket connection close acts as the liveness signal. No cleanup needed.

### `__call` Magic for Ergonomic Calling

PHP's `__call` magic method enables the `$handle->add(1, 2)` pattern without
requiring a code generator or proxy class. This mirrors Python's
`_AsyncCallNamespace` and Rust's `call!` macro.

## Module Map

| File | Responsibility |
|---|---|
| `src/Protocol.php` | Wire constants, structs, envelope validation |
| `src/Frame.php` | MsgPack encode/decode, binary frame codec |
| `src/Errors.php` | Typed error model + `errorFromWire()` |
| `src/Transport.php` | WebSocket dial, register, send/receive, pending routing |
| `src/RouterBootstrap.php` | Router reachability, lock, startup |
| `src/Connection.php` | `connect()`, `Connection`, `Handle` |
| `src/Process.php` | `spawn()`, `ServiceProcess` |
| `src/Service.php` | `ServiceWorker`, `ServiceContext`, `runService()` |

## Dependencies

- `rybakit/msgpack:^0.10` — pure PHP MsgPack, with proper str/bin/map/array handling
- `phrity/websocket:^3.7` — blocking WebSocket client with close/ping middleware

## Thread Safety

PHP is single-threaded. No goroutine equivalents. The transport's read loop runs
inline during `call()` (caller mode) or `serve()` (service mode). There is no
concurrent access to shared state.

## Error Handling

All errors are typed exceptions extending `MultifrostException`. Wire errors
decode into `RouterError` or `RemoteCallError` via the `errorFromWire()` function.

### Windows Router Bootstrap

On Windows, the PHP binding uses **PowerShell's `Start-Process`** to launch the
router instead of PHP's `proc_open`. This is documented in the code as
`RouterBootstrap::spawnRouterViaPowerShell()`.

**Why not `proc_open` on Windows?** PHP's `proc_open` on some Windows builds
creates child processes where the Winsock service provider fails to initialise
(error 10106: `WSAEPROVIDERFAILEDINIT`). This is a known interaction between
PHP's process creation flags and the Windows Layered Service Provider (LSP)
chain. The router, being a Rust binary that calls `TcpListener::bind()` on
startup, crashes immediately when this happens.

**Why PowerShell?** `Start-Process` delegates to .NET's `Process.Start`, which
uses `CreateProcess` with flags that correctly initialise Winsock in the child.
PowerShell 5.1+ is a Windows component, present on all supported Windows
versions (10+, Server 2016+, including Server Core). The overhead (~500ms) is
a one-time cost during router bootstrap.

**If PowerShell is unavailable** (Nano Server, IoT Core, or broken installation),
the error message tells the user exactly how to start the router manually:
```
set MULTIFROST_ROUTER_PORT=9981 && start /B "" "C:\path\to\multifrost-router.exe"
```

## Deviations from Other Bindings

- No async layer (PHP's paradigm doesn't need one)
- No sync wrapper (it's already sync)
- No logging or metrics subsystem (not required for MVP)
- No `call!` macro (`__call` magic provides the same ergonomics natively)
