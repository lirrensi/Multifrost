# Worker → Handle Refactor Plan

**Version**: comlink_ipc_v3 → comlink_ipc_v4  
**Date**: 2026-02-17

---

## Overview

Separate process definition (Worker) from runtime interface (Handle).

```
Worker = config/state (holds socket, process, registry internally)
Handle = lightweight API view (lifecycle + call interface)
```

---

## Core Pattern

```
worker = ParentWorker.spawn("script.py")  # Config (no process yet)
handle = worker.handle()                   # API view (async mode)
await handle.start()                       # Spawn process, connect ZMQ
result = await handle.call.method(args)    # Remote calls
await handle.stop()                        # Cleanup
```

---

## Per-Language Handle Modes

| Language | `handle()` | `handle_sync()` | Notes |
|----------|------------|-----------------|-------|
| Python | Async handle | Sync handle | Both available |
| JavaScript | Async handle | N/A | Async-only |
| Go | Handle (feels sync) | N/A | Goroutines |
| Rust | Async handle | N/A | Async-first |

---

## Implementation Steps

### 1. Protocol Version Update

**All languages**: Change app ID constant.

```
comlink_ipc_v3 → comlink_ipc_v4
```

Files to update:
- Python: `python/src/multifrost/protocol.py` (or equivalent)
- JavaScript: `javascript/src/protocol.ts`
- Go: `golang/protocol.go`
- Rust: `rust/src/protocol.rs`

---

### 2. Add Handle Class

Create a new `Handle` class/struct in each language.

**Handle responsibilities:**
- `start()` — spawn/connect process, bind ZMQ
- `stop()` — cleanup
- `call` namespace — proxy to remote methods

**Handle references Worker:**
- Handle holds reference to parent Worker
- Handle delegates lifecycle calls to Worker internals
- Handle is cheap to create (no state, just a view)

**Python structure:**
```python
class ParentHandle:
    def __init__(self, worker: ParentWorker, mode: str = "async"):
        self._worker = worker
        self._mode = mode
        self.call = CallProxy(worker)
    
    async def start(self): ...
    async def stop(self): ...
    
    # Context manager support
    async def __aenter__(self): ...
    async def __aexit__(self, *args): ...

class ParentHandleSync:
    def __init__(self, worker: ParentWorker):
        self._worker = worker
        self.call = CallProxy(worker)
    
    def start(self): ...
    def stop(self): ...
    
    def __enter__(self): ...
    def __exit__(self, *args): ...
```

**JavaScript structure:**
```typescript
class ParentHandle {
  private worker: ParentWorker;
  call: CallProxy;
  
  constructor(worker: ParentWorker) { ... }
  
  async start(): Promise<void> { ... }
  async stop(): Promise<void> { ... }
}
```

**Go structure:**
```go
type Handle struct {
    worker *ParentWorker
    call   *CallProxy
}

func (h *Handle) Start() error { ... }
func (h *Handle) Stop() error { ... }
```

**Rust structure:**
```rust
pub struct Handle<'a> {
    worker: &'a ParentWorker,
}

impl Handle<'_> {
    pub async fn start(&self) -> Result<()> { ... }
    pub async fn stop(&self) -> Result<()> { ... }
    pub fn call(&self) -> &CallProxy { ... }
}
```

---

### 3. Add `handle()` and `handle_sync()` Methods to Worker

**Worker methods to add:**
```python
# Python
def handle(self) -> ParentHandle: ...
def handle_sync(self) -> ParentHandleSync: ...
```

```typescript
// JavaScript
handle(): ParentHandle { ... }
```

```go
// Go
func (w *ParentWorker) Handle() *Handle { ... }
```

```rust
// Rust
impl ParentWorker {
    pub fn handle(&self) -> Handle { ... }
}
```

---

### 4. Move Lifecycle to Handle

**Before (current API):**
```
worker.start()
worker.stop()
worker.call.method()      # or worker.acall.method()
```

**After (new API):**
```
handle = worker.handle()
handle.start()
handle.stop()
handle.call.method()
```

**Implementation:**
- Keep `start()`/`stop()` logic in Worker (internal methods)
- Handle's `start()`/`stop()` delegate to Worker's internal methods
- Handle's mode (sync/async) determines whether to use blocking or async variants

**Python example:**
```python
# In Handle (async)
async def start(self):
    await self._worker._start_async()

# In HandleSync (sync)  
def start(self):
    self._worker._start_sync()
```

---

### 5. Implement `call` Namespace

**CallProxy/CallNamespace:**
- Proxy object that intercepts attribute access
- Returns callable that invokes remote method

**Before:**
```python
worker.call.method(args)      # sync
await worker.acall.method()   # async (confusing!)
```

**After:**
```python
await handle.call.method(args)   # async handle
handle.call.method(args)          # sync handle (Python)
```

**Implementation:**
- `handle.call` returns a proxy object
- Proxy `__getattr__` / `get` returns a callable
- Callable constructs CALL message, sends, waits for response

---

### 6. Remove Old API Surface

**Deprecate/remove:**
- `worker.call` (direct on worker)
- `worker.acall` (Python async variant)
- `worker.sync.call` (Python sync variant)
- Any `call_blocking` patterns

**Keep on Worker (introspection only):**
- `worker.is_healthy`
- `worker.circuit_open`
- `worker.metrics`
- `worker.last_heartbeat_rtt_ms`

---

### 7. Update Tests

For each language:
1. Update all test fixtures to use `handle.start()` / `handle.call.method()`
2. Add tests for Handle creation
3. Add tests for Python sync handle
4. Test that multiple handles can be created from same worker
5. Test context manager pattern (Python only)

---

## File Checklist

### Python (`python/`)
- [x] `src/multifrost/protocol.py` — update app ID
- [x] `src/multifrost/parent.py` — add Handle class, `handle()`, `handle_sync()`
- [x] Remove `acall` property (kept for backwards compat, deprecated)
- [x] Update `CallProxy` to work with Handle
- [x] `tests/` — update all tests

### JavaScript (`javascript/`)
- [x] `src/protocol.ts` — update app ID
- [x] `src/multifrost.ts` — add Handle class, `handle()`
- [x] Keep direct `call` on worker (backwards compat)
- [x] `tests/` — update all tests

### Go (`golang/`)
- [x] `protocol.go` — update app ID
- [x] `parent_worker.go` — add Handle struct, `Handle()` method
- [x] Update CallProxy
- [x] `*_test.go` — update all tests

### Rust (`rust/`)
- [x] `src/protocol.rs` — update app ID
- [x] `src/parent.rs` — add Handle struct, `handle()` method
- [x] Update CallProxy
- [x] `tests/` — update all tests

---

## Migration Guide (for users)

```python
# OLD (v3)
worker = ParentWorker.spawn("script.py")
await worker.start()
result = await worker.acall.method(args)  # confusing!
await worker.stop()

# NEW (v4)
worker = ParentWorker.spawn("script.py")
handle = worker.handle()
await handle.start()
result = await handle.call.method(args)   # clean!
await handle.stop()

# Python sync (v4)
worker = ParentWorker.spawn("script.py")
handle = worker.handle_sync()
handle.start()
result = handle.call.method(args)  # no await
handle.stop()
```

---

## Notes

- Handle is cheap — creating multiple handles from same worker is fine
- Handle mode is chosen once at creation, not per-call
- Worker retains all internal state; Handle is pure interface
- Context manager support (Python only): `async with worker.handle() as h:`
