# API Vision: Worker → Handle Pattern

## Core Concept

Separate **process definition** from **runtime interface**.

```
Worker (definition) → Handle (runtime) → call.<method>()
```
Goal: make standard initialization in all 4 langs;
make async default + remove ugly call/acall difference

python => switching between sync/async is one handle change + awaits everywehre
js => same just initialization diff

go => make as js, sync way is redundant; 

rust => ...


```rust
let worker = ParentWorker::spawn("script.py")

let handle = worker.handle()
...start
let result = handle.call("method", args).await?;
```

```go
worker := multifrost.Spawn("script.py")
handle := worker.Handle()
result := handle.Call("method", args)  // Blocks goroutine, not OS thread
```


| Language | Sync Handle? | Async Handle? | Why |
|----------|--------------|---------------|-----|
| Python | ✅ YES | ✅ YES | Both ecosystems exist |
| JavaScript | ❌ NO | ✅ YES | Everything is async |
| Go | N/A | ✅ YES (but feels sync) | Goroutines blur the line |
| Rust | ❌ NO | ✅ YES | Async is the pattern for network I/O |


┌─────────────────────────────────┐
│ The YAGNI Principle             │
│ (You Ain't Gonna Need It)       │
└─────────────────────────────────┘

Python:   handle() + handle_sync()  ← Real use cases exist
JS:       handle()                   ← Async-only world
Go:       Handle()                   ← Goroutines = async+sync hybrid
Rust:     handle()                   ← Ship it, wait for complaints

If Rust users complain → add handle_blocking() later
If Go users complain → ??? (they won't, goroutines ftw)



So the flow is:

```
Worker::spawn()     → Creates config (sync)
    ↓
worker.handle()     → Returns Handle struct (sync)
    ↓
handle.start().await → Actually connects/spawns process (async!)
    ↓
handle.call().await → Use it
```

**This means:**

✅ `worker.handle()` is cheap - just returns a view struct
✅ `worker.handle()` multiple times = multiple views, same underlying worker
✅ `.start()` is where ZMQ connects, process spawns, etc
✅ Handle can be passed around before starting

*tail swishes*

**So in all languages:**

```python
# Python
worker = ParentWorker.spawn("script.py")  # Config
handle = worker.handle()                   # View (cheap)
await handle.start()                       # Connection (expensive)
result = await handle.call.method()

# Rust  
let worker = ParentWorker::spawn("script.py")?;
let handle = worker.handle();              // View (cheap)
handle.start().await?;                     // Connection (expensive)
let result = handle.call("method", args).await?;

# Go
worker := multifrost.Spawn("script.py")
handle := worker.Handle()                  // View (cheap)
handle.Start()                             // Connection (expensive, blocks goroutine)
result := handle.Call("method", args)

# JS
const worker = ParentWorker.spawn("script.py")
const handle = worker.handle()             // View (cheap)
await handle.start()                       // Connection (expensive)
const result = await handle.call.method()
```

---

## Pattern

### 1. Define Worker

Process configuration, no runtime yet.

```python
# Spawn mode (owns child process)
worker = ParentWorker.spawn("script.py", executable="python")

# Connect mode 
worker = ParentWorker.connect("my-service")
```

### 2. Create Handle

Pick your runtime mode once.

```python
# Async handle (default, native)
handle = worker.handle()

# Sync handle (wraps async core)
handle = worker.handle_sync()
```

### 3. Use

Same API surface regardless of mode.

```python
# Async usage
async with worker.handle() as h:
    result = await h.call.my_func(1, 2)

# Sync usage
with worker.handle_sync() as h:
    result = h.call.my_func(1, 2)

# Or manual lifecycle
handle = worker.handle()
await handle.start()
result = await handle.call.my_func(1, 2)
await handle.stop()
```

---

## Namespace Structure

| Namespace | Purpose |
|-----------|---------|
| `handle.start()` / `handle.stop()` | Lifecycle |
| `handle.call.<method>()` | Remote function calls |
| `handle.metrics` / `handle.is_healthy` | Process introspection |

**Why `call` namespace?** Separates process methods from remote methods. Avoids collision if remote method is named `start` or `stop`.

---

## Cross-Language Consistency

| Language | Worker | Async Handle | Sync Handle |
|----------|--------|--------------|-------------|
| Python | `ParentWorker.spawn()` | `worker.handle()` | `worker.handle_sync()` |
| JavaScript | `ParentWorker.spawn()` | `worker.handle()` | N/A (async-only) |
| Go | `multifrost.Spawn()` | `worker.Handle()` | Same (goroutines) |
| Rust | `ParentWorker::spawn()` | `worker.handle().await` | `worker.handle_blocking()` |

---

## Design Rationale

1. **One decision point** — async/sync chosen when creating handle, not per-call
2. **Same surface after** — `start/stop/call.<method>` works identically
3. **Async-first internally** — sync handle wraps async core (event loop in background thread)
4. **No `acall` property** — the handle type *is* the mode
5. **Clean separation** — worker = config, handle = runtime, call = remote methods

---

## Migration Notes

Current API (v3/v4):
```python
worker.call.my_func()      # sync
await worker.acall.my_func()  # async (confusing)
```

New API:
```python
handle = worker.handle()
await handle.call.my_func()   # async (default)

handle = worker.handle_sync()
handle.call.my_func()         # sync (explicit)
```
