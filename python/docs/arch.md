# Multifrost Python Implementation

Python-specific implementation details for the Multifrost IPC library.

## Overview

This document describes the Python implementation of Multifrost, focusing on language-specific features, internal architecture, and implementation details that differ from the global specification.

## Language-Specific Features

### Async-First Design

The Python implementation is built around Python's `asyncio` event loop:

- **ParentWorker**: Fully async, uses `zmq.asyncio` for non-blocking ZeroMQ operations
- **ChildWorker**: Manages a dedicated event loop in a daemon thread for async method handlers
- **No blocking I/O**: All ZeroMQ operations are async; no explicit threading required for parent

### Worker → Handle Pattern (v4)

Starting with v4, the API separates process definition (Worker) from runtime interface (Handle):

```python
# Async usage (recommended)
worker = ParentWorker.spawn("worker.py")  # Config (no process yet)
handle = worker.handle()                   # Async handle
await handle.start()                       # Spawn process, connect ZMQ
result = await handle.call.add(1, 2)      # Remote calls
await handle.stop()                        # Cleanup

# Sync usage
worker = ParentWorker.spawn("worker.py")
handle = worker.handle_sync()              # Sync handle
handle.start()                             # Blocking start
result = handle.call.add(1, 2)             # No await needed
handle.stop()

# Context manager support (async)
async with worker.handle() as h:
    result = await h.call.add(1, 2)

# Context manager support (sync)
with worker.handle_sync() as h:
    result = h.call.add(1, 2)
```

**Key concepts:**
- **Worker** = config/state (holds socket, process, registry internally)
- **Handle** = lightweight API view (lifecycle + call interface)
- Handle is cheap to create — multiple handles from same worker is fine
- Handle mode is chosen once at creation, not per-call

### Legacy API (deprecated but still available)

For backwards compatibility, the old API is still available:

```python
# OLD (v3) - deprecated
worker = ParentWorker.spawn("worker.py")
await worker.start()
result = await worker.acall.add(1, 2)  # confusing naming
await worker.stop()

# NEW (v4) - recommended
worker = ParentWorker.spawn("worker.py")
handle = worker.handle()
await handle.start()
result = await handle.call.add(1, 2)  # clean!
await handle.stop()
```

### Async Method Support

Child workers can implement both sync and async methods:

```python
class MyWorker(ChildWorker):
    # Synchronous method
    def add(self, a, b):
        return a + b

    # Asynchronous method (uses dedicated event loop)
    async def fetch_data(self, url):
        import aiohttp
        async with aiohttp.ClientSession() as session:
            async with session.get(url) as resp:
                return await resp.json()
```

**Implementation details:**

- ChildWorker spawns a daemon thread with an event loop at startup
- Async function calls use `asyncio.run_coroutine_threadsafe()` to submit to the loop
- 30-second timeout for async calls to prevent hangs
- Loop is properly stopped during cleanup

## Internal Architecture

### Module Structure

```
multifrost/
├── __init__.py          # Public API exports
└── core/
    ├── __init__.py      # Core module exports
    ├── parent.py        # ParentWorker, ParentHandle, ParentHandleSync
    ├── child.py         # ChildWorker implementation
    ├── message.py       # Message serialization/parsing
    ├── service_registry.py  # Service discovery with locking
    ├── metrics.py       # Request tracking & statistics
    ├── logging.py       # Structured JSON logging
    └── sync_wrapper.py  # Legacy sync API over async core
```

### Core Components

#### ParentWorker (`async_worker.py`)

**Responsibilities:**

- Manage child process lifecycle (spawn/connect modes)
- Handle ZeroMQ DEALER socket in async mode
- Manage pending requests via `asyncio.Future`
- Circuit breaker implementation
- Heartbeat monitoring with RTT calculation
- Metrics collection
- Structured logging

**Key attributes:**

```python
self._loop: asyncio.AbstractEventLoop          # Running event loop
self.socket: zmq.asyncio.Socket                # DEALER socket
self.process: subprocess.Popen                 # Child process (spawn mode)
self.pending_requests: Dict[str, Future]       # Request -> Future mapping
self._heartbeat_task: asyncio.Task             # Heartbeat loop task
```

**Async event loop (`_zmq_event_loop`):**

- Pure async loop (no executor needed)
- Receives `[empty_frame, message_data]` from DEALER
- Routes responses to pending futures by message ID
- Handles STDOUT/STDERR forwarding
- Checks child process health

**Circuit breaker implementation:**

- Tracks consecutive failures via `_consecutive_failures`
- Opens after `max_restart_attempts` (default: 5)
- Records trip events to metrics
- Resets on successful call
- Cannot make calls when open

**Heartbeat monitoring:**

- Sends heartbeat every `heartbeat_interval` seconds (default: 5.0)
- Waits for response with `heartbeat_timeout` (default: 3.0)
- Calculates RTT from original timestamp in metadata
- Tracks consecutive misses; trips circuit breaker after `heartbeat_max_misses` (default: 3)
- RTT used for metrics and health checks

#### ChildWorker (`child.py`)

**Responsibilities:**

- Run ZeroMQ ROUTER socket (supports multiple parents)
- Handle function call dispatch
- Support both sync and async method handlers
- Redirect stdout/stderr to parent
- Register/unregister with service registry (connect mode)

**Key attributes:**

```python
self.context: zmq.Context                      # ZeroMQ context
self.socket: zmq.Socket                        # ROUTER socket
self.port: Optional[int]                       # Port for ROUTER
self.original_stdout/stderr                    # Saved original streams
self._async_loop: Optional[asyncio.AbstractEventLoop]
self._async_thread: Optional[threading.Thread]
```

**Async loop setup (`_setup_async_loop`):**

- Creates new event loop in daemon thread
- Calls `asyncio.set_event_loop()` for the thread
- Runs `run_forever()` until stopped
- Used for async method handlers only

**IO redirection (`_setup_io_redirection`):**

- Creates `ZMQWriter` class that wraps stdout/stderr
- Installs writer at `sys.stdout`/`sys.stderr`
- Sends output via `_send_output()` with retry logic
- Restores originals during cleanup

**Message handling (`_handle_message`):**

- Validates `app` ID and `namespace`
- Routes based on `message.type`:
  - `CALL`: Calls method via `_handle_function_call`
  - `HEARTBEAT`: Echoes back immediately
  - `SHUTDOWN`: Sets `_running = False`

**Function call handling (`_handle_function_call`):**

1. Validates message (has `function` and `id` fields)
2. Extracts args via `extract_call_args()`
3. Validates method exists and is callable
4. Rejects private methods (`_` prefix)
5. Routes to sync or async handler:

```python
if asyncio.iscoroutinefunction(func):
    # Async path
    future = asyncio.run_coroutine_threadsafe(func(*args), self._async_loop)
    result = future.result(timeout=30.0)
else:
    # Sync path
    result = func(*args)
```

6. Creates RESPONSE or ERROR message
7. Sends via ROUTER envelope: `[sender_id, b"", response_data]`

#### ServiceRegistry (`service_registry.py`)

**Responsibilities:**

- Atomic file-based registry at `~/.multifrost/services.json`
- Port allocation for connect mode workers
- Process health checking
- Concurrent access via file locking

**Registry format:**

```json
{
  "my-service": {
    "port": 5555,
    "pid": 12345,
    "started": "2026-02-11T12:00:00"
  }
}
```

**File locking (TOCTOU-safe):**

- Uses `os.open(O_CREAT | O_EXCL)` for atomic lock acquisition
- Platform-specific:
  - **Windows**: `msvcrt.locking()` on file descriptor
  - **Unix**: `fcntl.flock()` on file descriptor
- Lock file removed after release
- 10-second max wait for lock acquisition
- Fails gracefully if lock cannot be acquired (logged, not crashed)

**Process health checking (`_is_process_alive`):**

- Prefers `psutil.pid_exists()` if available
- Falls back to `os.kill(pid, 0)` signal check
- Returns `True` if process exists and responds to signal 0

#### Message (`message.py`)

**Responsibilities:**

- Serialize/deserialize messages to/from msgpack
- Validate message structure
- Sanitize data for cross-language interoperability

**Message schema:**

All messages have:
- `app`: `"comlink_ipc_v4"`
- `id`: UUID v4 string
- `type`: One of CALL, RESPONSE, ERROR, STDOUT, STDERR, HEARTBEAT, SHUTDOWN
- `timestamp`: Unix epoch seconds (float)
- `metadata`: Optional dict for tracing

**Sanitization (`_sanitize_for_msgpack`):**

Handles edge cases for cross-language interop:

- `NaN`/`Infinity` → `null`
- Integer overflow → clamp to int64 range
- Non-string dict keys → string conversion
- Binary data → `msgpack.ExtType`

#### SyncWrapper (`sync_wrapper.py`)

**Responsibilities:**

- Provides synchronous API over async ParentWorker
- Manages event loop in background thread
- Handles thread-safety for async calls

**Architecture:**

- `SyncWrapper`: Manages event loop and runs async calls
- `SyncProxy`: Proxy object with method access (`worker.sync.method()`)
- `AsyncProxy`: Proxy object with async method access (`worker.call.method()`)

**Note:** In v4, prefer using `worker.handle_sync()` instead of `worker.sync.*`:

```python
# v4 recommended
handle = worker.handle_sync()
handle.start()
result = handle.call.add(1, 2)
handle.stop()

# Legacy (still works)
worker.sync.start()
result = worker.sync.call.add(1, 2)
worker.sync.close()
```

#### ParentHandle (`parent.py`)

**Responsibilities:**

- Provides async lifecycle and call interface
- Delegates to ParentWorker's internal methods
- Supports async context manager protocol

**Key methods:**

```python
class ParentHandle:
    def __init__(self, worker: ParentWorker):
        self._worker = worker
        self.call = CallProxy(worker)  # Remote call proxy
    
    async def start(self) -> None: ...
    async def stop(self) -> None: ...
    
    # Context manager support
    async def __aenter__(self) -> ParentHandle: ...
    async def __aexit__(self, *args) -> None: ...
```

#### ParentHandleSync (`parent.py`)

**Responsibilities:**

- Provides synchronous lifecycle and call interface
- Wraps async operations with blocking calls
- Supports sync context manager protocol

**Key methods:**

```python
class ParentHandleSync:
    def __init__(self, worker: ParentWorker):
        self._worker = worker
        self.call = CallProxy(worker)  # Remote call proxy
    
    def start(self) -> None: ...  # Blocking
    def stop(self) -> None: ...   # Blocking
    
    # Context manager support
    def __enter__(self) -> ParentHandleSync: ...
    def __exit__(self, *args) -> None: ...
```

**Event loop management:**

- Created in daemon thread if not exists
- Started via `run_forever()` when needed
- Stopped via `loop.stop()` with thread join
- Thread-safe via `asyncio.run_coroutine_threadsafe()`

#### Metrics (`metrics.py`)

**Responsibilities:**

- Track request latency (p50, p95, p99)
- Count successes/failures
- Monitor queue depth
- Track circuit breaker events
- Monitor heartbeat RTT

**Thread-safety:**

- All counters protected by `threading.Lock()`
- Circular buffers for latency samples (`deque(maxlen=...)`)
- Snapshot operation locks entire state

**Percentile calculation:**

```python
sorted_latencies = sorted(latencies)
n = len(sorted_latencies)
p50_idx = int(n * 0.50)
p95_idx = int(n * 0.95)
p99_idx = int(n * 0.99)
```

#### Logging (`logging.py`)

**Responsibilities:**

- Structured JSON logging with correlation IDs
- Pluggable output handlers
- Context fields (worker_id, service_id, request_id, etc.)

**LogEntry structure:**

```python
@dataclass
class LogEntry:
    event: str                    # e.g., "request_start"
    level: str                    # "debug", "info", "warn", "error"
    message: str
    correlation_id: Optional[str]
    request_id: Optional[str]
    function: Optional[str]
    duration_ms: Optional[float]
    success: Optional[bool]
    error: Optional[str]
    metrics: Optional[Dict]       # Optional metrics snapshot
```

**Built-in handlers:**

- `default_json_handler`: Prints JSON to stdout
- `default_pretty_handler`: Human-readable format

## ZeroMQ Implementation

### Socket Types

**Parent (DEALER):**
- Sends with empty delimiter frame: `[b"", message_data]`
- Receives with empty delimiter frame: `[b"", message_data]`
- Uses `zmq.asyncio.Context` and `zmq.asyncio.Socket`
- Timeout: 100ms send/recv

**Child (ROUTER):**
- Sends with sender envelope: `[sender_id, b"", message_data]`
- Receives with sender envelope: `[sender_id, b"", message_data]`
- Supports multiple parents (one sender_id per parent)
- Timeout: 100ms recv, 1000ms linger

### Async Integration

**ParentWorker:**
- Uses `zmq.asyncio.Context()` for async context
- All operations are async (`await socket.recv_multipart()`)
- Event loop is the main asyncio loop (no executor needed)

**ChildWorker:**
- Uses regular `zmq.Context()` for sync socket
- Event loop is in daemon thread (separate from main thread)

## Data Interop

### msgpack Encoding

- Top-level data structure is a dict
- Use `msgpack.packb(data, use_bin_type=True)` for sending
- Use `msgpack.unpackb(data, raw=False)` for receiving
- Size limits for DoS protection:
  - Max bin length: 10MB
  - Max string length: 10MB
  - Max array length: 100,000
  - Max map length: 100,000

### Python Type Mapping

| Python Type | msgpack Encoding | Notes |
|-------------|------------------|-------|
| `str` | UTF-8 string | |
| `bytes` | Bin type | |
| `int` | Int64 | Clamped to ±2^63-1 |
| `float` | Float64 | NaN/Inf → null |
| `list` | Array | |
| `dict` | Map | Keys converted to str |
| `None` | null | |
| `bool` | bool | |
| Custom objects | msgpack.ExtType | Requires pack/unpack methods |

## Process Management

### Child Process Spawn

```python
# Parent creates child with environment
env = os.environ.copy()
env["COMLINK_ZMQ_PORT"] = str(port)
env["COMLINK_WORKER_MODE"] = "1"

self.process = subprocess.Popen(
    [executable, script_path, "--worker"],
    env=env,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)
```

**Environment variables:**

- `COMLINK_ZMQ_PORT`: Port for child to connect to (spawn mode)
- `COMLINK_WORKER_MODE`: Set to `"1"` by parent (detect spawn vs connect)

**Signal handling:**

- SIGINT/SIGTERM forwarded to child
- Child handles in `_handle_signals()` if in main thread
- Graceful shutdown via `_stop()` method

## Error Handling

### Error Types

**RemoteCallError:**
- Raised when remote call fails (child error)
- Includes error message from child (with traceback)

**CircuitOpenError:**
- Raised when circuit breaker is open
- Cannot make calls while open

**TimeoutError:**
- Raised when call exceeds timeout
- Default: `default_timeout` config

### Error Propagation

**From child to parent:**

1. Child creates ERROR message with traceback
2. Child sends via ROUTER envelope
3. Parent unpacks message
4. Parent sets exception on Future
5. Parent logs error

**From child to stdout/stderr:**

1. `ZMQWriter` intercepts writes
2. Calls `_send_output()` with retry logic
3. Retries up to 2 times with 1ms delay
4. Logs warnings on failures

## Reliability Features

### Circuit Breaker

**States:**
- `closed`: Normal operation
- `open`: Too many failures, rejecting calls
- `half-open`: Testing recovery (not implemented in Python)

**Trip conditions:**
- `max_restart_attempts` consecutive failures (default: 5)
- `heartbeat_max_misses` consecutive heartbeat timeouts (default: 3)
- Child process exit

**Reset conditions:**
- Successful call after failures
- Manual recovery (not implemented)

### Auto-Restart

**Configurable:**
- `auto_restart`: Enable/disable (default: False)
- `max_restart_attempts`: Max attempts before giving up

**Behavior:**
- Increments `restart_count` on each attempt
- Logs restart attempts
- Stops if `restart_count >= max_restart_attempts`

### Resource Cleanup

**ParentWorker cleanup (`close` and `__del__`):**

1. Cancel all tasks (`_heartbeat_task`, `_zmq_task`)
2. Clear pending requests with exception
3. Close ZMQ socket (linger=0)
4. Terminate child process with timeout
5. Terminate ZMQ context

**ChildWorker cleanup (`_stop`):**

1. Set `_running = False`
2. Unregister from service registry
3. Restore stdout/stderr
4. Stop event loop with timeout
5. Close socket and context

**__del__ safety:**

- `__del__` performs minimal synchronous cleanup
- Uses `hasattr()` checks to safely access attributes
- Avoids relying on event loop during GC
- Prevents race conditions where cleanup may never run

## Testing

### Test Coverage

Located in `python/tests/`:

- `test_v3.py`: Main IPC protocol tests
- `test_observability.py`: Metrics and logging tests

### Key test scenarios:

- Spawn mode parent-child communication
- Connect mode service discovery
- Circuit breaker trip/reset
- Heartbeat monitoring
- Concurrent calls
- Async method handling
- Error propagation
- Service registry locking

## Dependencies

**Required:**

- `msgpack>=1.1.2`: Serialization
- `psutil>=7.2.2`: Process management
- `zmq>=0.0.0`: ZeroMQ bindings (pyzmq)

**Python version:** >= 3.10

## Performance Characteristics

**Latency:**

- Heartbeat RTT: Typically 1-5ms on localhost
- Function call: + RTT (usually < 10ms for local workers)
- Async overhead: Minimal (run_coroutine_threadsafe)

**Concurrency:**

- Parent can issue unlimited concurrent calls (asyncio handles this)
- Child processes messages sequentially (one at a time)
- No per-call thread overhead (asyncio event loop)

**Memory:**

- Parent: ~1-5MB per worker (socket + pending requests)
- Child: ~2-10MB (depends on worker implementation)
- No per-call memory allocation (reuses event loop)

## Comparison to JavaScript Implementation

| Feature | Python | JavaScript |
|---------|--------|------------|
| Async support | Yes (asyncio) | Yes (async/await) |
| Sync API | Yes (SyncWrapper) | No (async-only) |
| Child async handling | Dedicated event loop | Event loop in worker |
| Service registry | File-based with locking | File-based with locking |
| Metrics | Thread-safe (threading.Lock) | Async (asyncio.Lock) |
| Logging | Structured JSON, pluggable | Structured JSON, pluggable |
| Process handling | subprocess.Popen | child_process.spawn |
| ZeroMQ | zmq.asyncio | zmq.asyncio |
| Default timeout | Configurable (ms) | Configurable (ms) |
| Heartbeat | Yes (RTT tracking) | Yes (RTT tracking) |
| Circuit breaker | Yes | Yes |

## Migration Guide

### From v3 to v4 (Worker → Handle Pattern)

The v4 API separates process definition (Worker) from runtime interface (Handle):

```python
# OLD (v3)
worker = ParentWorker.spawn("worker.py")
await worker.start()
result = await worker.acall.add(1, 2)  # confusing naming!
await worker.stop()

# NEW (v4) - Async
worker = ParentWorker.spawn("worker.py")
handle = worker.handle()
await handle.start()
result = await handle.call.add(1, 2)  # clean!
await handle.stop()

# NEW (v4) - Sync
worker = ParentWorker.spawn("worker.py")
handle = worker.handle_sync()
handle.start()
result = handle.call.add(1, 2)
handle.stop()

# NEW (v4) - Context manager
async with worker.handle() as h:
    result = await h.call.add(1, 2)
```

**Key changes:**
- `worker.acall.*` → `handle.call.*` (async handle)
- `worker.sync.call.*` → `handle.call.*` (sync handle)
- `worker.start()` → `handle.start()`
- `worker.stop()` → `handle.stop()`
- Context manager support on handle, not worker

### From JavaScript to Python

If you have a JavaScript parent calling Python child:

1. Install Python dependencies: `uv pip install -e .`
2. Use async API (recommended):
   ```python
   from multifrost import ParentWorker

   worker = ParentWorker.spawn("worker.py")
   handle = worker.handle()
   await handle.start()
   result = await handle.call.myFunction(arg1, arg2)
   await handle.stop()
   ```
3. Or use sync handle:
   ```python
   from multifrost import ParentWorker

   worker = ParentWorker.spawn("worker.py")
   handle = worker.handle_sync()
   handle.start()
   result = handle.call.myFunction(arg1, arg2)
   handle.stop()
   ```

### From Python to JavaScript

If you have a Python parent calling JavaScript child:

1. Ensure Node.js worker uses `--worker` flag
2. Use same async API:
   ```javascript
   const worker = await ParentWorker.connect("my-service", 5);
   await worker.start();
   const result = await worker.call.myFunction(arg1, arg2);
   await worker.close();
   ```

## Common Patterns

### Singleton Service Pattern

```python
# worker.py
class MyWorker(ChildWorker):
    def __init__(self):
        super().__init__(service_id="my-service")

worker = MyWorker()
worker.run()
```

### Worker Pool Pattern

```python
# parent.py
from concurrent.futures import ThreadPoolExecutor

async def parallel_call(func, args_list):
    async with ThreadPoolExecutor(max_workers=5) as executor:
        tasks = [
            worker.acall.call(func, *args)
            for args in args_list
        ]
        return await asyncio.gather(*tasks)
```

### Context Manager Pattern

```python
# Both parent and child support context managers
async with ParentWorker.spawn("worker.py") as worker:
    result = await worker.acall.add(1, 2)
# Automatic cleanup
```

## Troubleshooting

### Child process exits immediately

- Check `COMLINK_ZMQ_PORT` environment variable
- Verify port is in range 1024-65535
- Check stderr for error messages

### Async functions don't work

- Verify `asyncio.iscoroutinefunction(func)` returns True
- Check if event loop is running: `self._async_loop.is_running()`
- Ensure timeout is set (default: 30s)

### Circuit breaker trips unexpectedly

- Check heartbeat configuration
- Verify child process is responding
- Review metrics for error patterns

### Service registry lock contention

- Check if another process holds the lock
- Verify lock file exists at `~/.multifrost/registry.lock`
- Wait up to 10s for lock acquisition

### Messages not arriving

- Verify ZeroMQ socket is bound/connecting to correct port
- Check `app` ID matches `"comlink_ipc_v4"`
- Ensure `namespace` matches child's `namespace` attribute
- Check for port conflicts (EADDRINUSE)
