# Multifrost Python FAQ

## Table of Contents

- [Getting Started](#getting-started)
- [API Usage Patterns](#api-usage-patterns)
- [Common Gotchas](#common-gotchas)
- [Performance Considerations](#performance-considerations)
- [Debugging Tips](#debugging-tips)
- [Troubleshooting](#troubleshooting)
- [Best Practices](#best-practices)
- [Cross-Language Communication](#cross-language-communication)

---

## Getting Started

### What are the minimum requirements for using Multifrost Python?

- **Python version**: Python ≥ 3.10
- **Dependencies**:
  - `msgpack>=1.1.2`: For message serialization
  - `psutil>=7.2.2`: For process management
  - `zmq>=0.0.0`: ZeroMQ bindings (pyzmq)

Install dependencies:

```bash
# Using pip
pip install msgpack psutil pyzmq

# Using make
make install-python

# Or install from source
uv pip install -e python/
```

### What's the difference between spawn and connect modes?

**Spawn mode** (default):
- Parent starts child process with `ParentWorker.spawn()`
- Child connects to parent automatically
- Best for local or short-lived processes
- No service registry required

```python
# Spawn mode
async with ParentWorker.spawn("worker.py") as worker:
    result = await worker.acall.add(1, 2)
```

**Connect mode**:
- Parent discovers child via service registry
- Child registers itself and waits for connections
- Best for long-running services
- Uses atomic file locking for concurrency

```python
# Connect mode
worker = ParentWorker.connect("my-service")
await worker.start()
result = await worker.call.add(1, 2)
await worker.close()
```

### How do I create a simple worker?

Create a Python file (e.g., `worker.py`):

```python
from multifrost import ChildWorker

class MyWorker(ChildWorker):
    def __init__(self):
        super().__init__(service_id="my-service")

    def add(self, a: int, b: int) -> int:
        """Add two numbers together."""
        return a + b

    async def async_add(self, a: int, b: int) -> int:
        """Async version of add."""
        # Simulate async work
        await asyncio.sleep(0.1)
        return a + b

if __name__ == "__main__":
    worker = MyWorker()
    worker.run()
```

Run the worker:

```bash
python worker.py
```

### What's the recommended way to initialize the worker?

For spawn mode:

```python
# worker.py
class MyWorker(ChildWorker):
    def __init__(self):
        super().__init__(service_id="my-service")
        # Initialize your resources here
```

For connect mode:

```python
# worker.py
class MyWorker(ChildWorker):
    def __init__(self):
        super().__init__(service_id="my-service")
        # Initialize your resources here

worker = MyWorker()
worker.run()
```

---

## API Usage Patterns

### Should I use async or sync API?

**Use async API** (recommended):
- Non-blocking I/O
- Better performance for concurrent operations
- Modern Python best practice
- Full access to asyncio ecosystem

```python
from multifrost import ParentWorker

async with ParentWorker.spawn("worker.py") as worker:
    result = await worker.acall.factorial(10)
    print(result)  # 3628800
```

**Use sync API** (legacy):
- For codebases not yet ready for async/await
- Simpler migration path
- Slightly higher overhead

```python
from multifrost import ParentWorker

worker = ParentWorker.spawn("worker.py")
worker.sync.start()
result = worker.sync.call.factorial(10)
worker.sync.close()
print(result)  # 3628800
```

### How does the sync wrapper work internally?

The `SyncWrapper` runs an event loop in a background daemon thread:

1. Creates event loop in daemon thread when `start()` is called
2. Runs event loop with `run_forever()` until `close()` is called
3. Uses `asyncio.run_coroutine_threadsafe()` to submit async calls
4. Waits for results via `future.result(timeout=...)`

**Important**: Always call `start()` before making calls and `close()` after:

```python
worker = ParentWorker.spawn("worker.py")
worker.sync.start()

# Make multiple calls
result1 = worker.sync.call.add(1, 2)
result2 = worker.sync.call.multiply(3, 4)

worker.sync.close()
```

### Can a child worker have both sync and async methods?

Yes! Child workers support both:

```python
class MyWorker(ChildWorker):
    # Sync method
    def add(self, a: int, b: int) -> int:
        return a + b

    # Async method
    async def fetch_data(self, url: str) -> dict:
        import aiohttp
        async with aiohttp.ClientSession() as session:
            async with session.get(url) as resp:
                return await resp.json()
```

**How it works**:
- Sync methods execute directly in the worker thread
- Async methods use the dedicated event loop in a daemon thread
- Parent sends calls to appropriate handler based on `asyncio.iscoroutinefunction(func)`

### What's the difference between `acall` and `call`?

- **`worker.acall.method()`**: Async API (recommended)
  - Non-blocking
  - Returns `asyncio.Future`
  - Use `await` keyword

- **`worker.call.method()`**: Sync API (via sync wrapper)
  - Blocking
  - Returns result directly
  - Requires sync wrapper to be started

```python
# Async API (preferred)
async with ParentWorker.spawn("worker.py") as worker:
    result = await worker.acall.add(1, 2)

# Sync API
worker = ParentWorker.spawn("worker.py")
worker.sync.start()
result = worker.sync.call.add(1, 2)
worker.sync.close()
```

---

## Common Gotchas

### Why do I get a "Circuit breaker is open" error?

The circuit breaker trips when:

1. **`max_restart_attempts` consecutive failures** (default: 5)
2. **`heartbeat_max_misses` consecutive heartbeat timeouts** (default: 3)
3. **Child process exits unexpectedly**

**Prevention**:
- Ensure child process is running and responsive
- Check heartbeat configuration
- Monitor metrics for error patterns

```python
# Enable auto-restart
worker = ParentWorker.spawn(
    "worker.py",
    auto_restart=True,
    max_restart_attempts=3
)
```

### Why is my async method not working?

Common issues:

1. **Method is not actually async**:
   ```python
   # Wrong - missing async keyword
   def fetch_data(self, url):
       return requests.get(url)

   # Correct
   async def fetch_data(self, url):
       async with aiohttp.ClientSession() as session:
           async with session.get(url) as resp:
               return await resp.json()
   ```

2. **Event loop not running**:
   - For child workers: Event loop runs in daemon thread automatically
   - For parent workers: Event loop runs in main asyncio loop

3. **Timeout too short**:
   ```python
   # Increase timeout for slow operations
   result = await worker.acall.fetch_data(
       url,
       timeout=60.0  # seconds
   )
   ```

### Why doesn't my worker respond immediately?

1. **Startup time**: Worker needs to initialize and bind ZeroMQ socket
2. **Message routing**: Parent sends call, waits for response
3. **Circuit breaker**: If trips, calls are rejected

**Check worker status**:
```python
# Check if worker is responding
async with ParentWorker.spawn("worker.py") as worker:
    # Check heartbeat
    rtt = await worker.heartbeat()
    print(f"RTT: {rtt:.3f}ms")

    # Make a test call
    result = await worker.acall.ping()
```

### Why does my message raise a "method not found" error?

Ensure method is:
1. Defined in your worker class
2. Public (not starting with `_`)
3. Callable

```python
class MyWorker(ChildWorker):
    # Good - public method
    def add(self, a, b):
        return a + b

    # Bad - private method (won't be called remotely)
    def _internal_add(self, a, b):
        return a + b
```

---

## Performance Considerations

### What's the typical latency?

- **Heartbeat RTT**: 1-5ms on localhost
- **Function call**: + RTT (usually < 10ms for local workers)
- **Async overhead**: Minimal (run_coroutine_threadsafe)

```python
import time

async with ParentWorker.spawn("worker.py") as worker:
    # Warm up
    await worker.acall.ping()

    # Measure latency
    start = time.time()
    for _ in range(100):
        await worker.acall.add(1, 2)
    elapsed = time.time() - start

    print(f"Average latency: {elapsed/100*1000:.2f}ms")
```

### How many concurrent calls can I make?

**Unlimited** - asyncio handles concurrent calls efficiently:

```python
async with ParentWorker.spawn("worker.py") as worker:
    # Make many concurrent calls
    tasks = [
        worker.acall.add(i, i)
        for i in range(100)
    ]
    results = await asyncio.gather(*tasks)
    print(results)
```

**Child processes** process messages sequentially, but:
- Parent can issue unlimited concurrent calls
- No per-call thread overhead (asyncio event loop)
- Each child process is independent

### What's the memory footprint?

- **Parent**: ~1-5MB per worker (socket + pending requests)
- **Child**: ~2-10MB (depends on worker implementation)
- **No per-call memory allocation** (reuses event loop)

### How do I optimize for performance?

1. **Use async API** for non-blocking operations
2. **Batch operations** when possible
3. **Reuse worker connections** instead of spawning new ones
4. **Tune timeouts** for your workload
5. **Monitor metrics** for bottlenecks

```python
# Optimized pattern
async with ParentWorker.spawn("worker.py") as worker:
    # Make multiple related calls in parallel
    results = await asyncio.gather(
        worker.acall.fetch_user(1),
        worker.acall.fetch_user(2),
        worker.acall.fetch_user(3),
    )
```

---

## Debugging Tips

### How do I enable structured logging?

Enable JSON logging with correlation IDs:

```python
from multifrost.core.logging import setup_logging

setup_logging(
    level="DEBUG",
    output="stdout",
    enable_correlation_ids=True
)

async with ParentWorker.spawn("worker.py") as worker:
    result = await worker.acall.add(1, 2)
```

**Log output includes**:
- Event type (request_start, request_complete, etc.)
- Correlation ID for tracing
- Request ID
- Function name
- Duration
- Success/failure status
- Error messages (if any)

### How do I monitor metrics?

Access metrics via `worker.metrics`:

```python
async with ParentWorker.spawn("worker.py") as worker:
    # Make some calls
    for i in range(10):
        await worker.acall.add(i, i)

    # Get metrics snapshot
    metrics = worker.metrics.snapshot()

    print(f"Total calls: {metrics.total_calls}")
    print(f"Success rate: {metrics.success_rate:.2%}")
    print(f"P50 latency: {metrics.latency_p50:.3f}ms")
    print(f"P95 latency: {metrics.latency_p95:.3f}ms")
    print(f"P99 latency: {metrics.latency_p99:.3f}ms")
```

### How do I debug child process issues?

1. **Check stderr output**:
   ```python
   # Worker redirects stdout/stderr to parent
   # Check logs in parent console
   ```

2. **Enable verbose logging**:
   ```python
   from multifrost.core.logging import setup_logging
   setup_logging(level="DEBUG")
   ```

3. **Check child process status**:
   ```python
   # In worker
   import psutil

   def check_process():
       return psutil.pid_exists(self.process.pid)
   ```

4. **Use environment variables**:
   ```bash
   COMLINK_LOG_LEVEL=DEBUG python worker.py
   ```

### How do I troubleshoot circuit breaker trips?

Check circuit breaker state and metrics:

```python
async with ParentWorker.spawn("worker.py") as worker:
    # Check circuit breaker state
    print(f"Circuit state: {worker.circuit_state}")
    print(f"Consecutive failures: {worker._consecutive_failures}")
    print(f"Restart count: {worker.restart_count}")

    # Check metrics
    metrics = worker.metrics.snapshot()
    print(f"Failed calls: {metrics.failed_calls}")
    print(f"Circuit breaker trips: {metrics.circuit_breaker_trips}")

    # Check heartbeat
    rtt = await worker.heartbeat()
    print(f"RTT: {rtt:.3f}ms")
```

---

## Troubleshooting

### Child process exits immediately

**Check**:
1. `COMLINK_ZMQ_PORT` environment variable
2. Port is in range 1024-65535
3. stderr for error messages

**Solution**:
```bash
# Set port explicitly
export COMLINK_ZMQ_PORT=5555
python worker.py
```

### Async functions don't work

**Check**:
1. Method is actually async
2. Event loop is running
3. Timeout is sufficient

**Solution**:
```python
# Verify async method
async def my_async_method(self):
    # ...
    pass

print(asyncio.iscoroutinefunction(my_async_method))
# Should print: True
```

### Circuit breaker trips unexpectedly

**Check**:
1. Heartbeat configuration
2. Child process is responding
3. Review metrics for error patterns

**Solution**:
```python
# Tune heartbeat settings
worker = ParentWorker.spawn(
    "worker.py",
    heartbeat_interval=3.0,  # Send every 3s
    heartbeat_timeout=2.0,   # Wait 2s for response
    heartbeat_max_misses=2   # Trip after 2 misses
)
```

### Service registry lock contention

**Check**:
1. Another process holds the lock
2. Lock file exists at `~/.multifrost/registry.lock`
3. Wait up to 10s for lock acquisition

**Solution**:
```bash
# Check lock file
ls -la ~/.multifrost/registry.lock

# Kill holding process (if needed)
ps aux | grep worker
kill <PID>
```

### Messages not arriving

**Check**:
1. ZeroMQ socket is bound/connecting to correct port
2. `app` ID matches `"comlink_ipc_v3"`
3. `namespace` matches child's `namespace` attribute
4. Port conflicts (EADDRINUSE)

**Solution**:
```python
# Verify connection
async with ParentWorker.spawn("worker.py") as worker:
    print(f"Socket address: {worker.socket}")
    print(f"Process PID: {worker.process.pid}")
```

### Timeout errors

**Check**:
1. Timeout is sufficient for operation
2. Child process is running
3. No infinite loops in child code

**Solution**:
```python
# Increase timeout
result = await worker.acall.long_operation(
    timeout=60.0  # seconds
)
```

### Cross-language communication issues

**Common problems**:

1. **Type incompatibility**:
   - Python `None` → msgpack `null`
   - Python `int` → msgpack Int64 (clamped)
   - Python `float` → msgpack Float64 (NaN/Inf → `null`)

2. **Binary data**:
   - Python `bytes` → msgpack bin type
   - Use `msgpack.ExtType` for custom objects

3. **Message size limits**:
   - Max bin length: 10MB
   - Max string length: 10MB
   - Max array length: 100,000
   - Max map length: 100,000

**Solution**:
```python
# Sanitize data for msgpack
from multifrost.core.message import Message

# NaN/Infinity → null
data = {"value": float("nan")}
sanitized = Message._sanitize_for_msgpack(data)
# Result: {"value": None}
```

---

## Best Practices

### When should I use spawn vs connect?

**Use spawn when**:
- Running short-lived tasks
- Local processes
- Need quick setup
- No need for service discovery

**Use connect when**:
- Running long-running services
- Multiple parents need to connect to same worker
- Need to discover workers by name
- Want to manage worker lifecycle separately

### How should I handle errors?

**From parent to child**:

```python
async with ParentWorker.spawn("worker.py") as worker:
    try:
        result = await worker.acall.add(1, 2)
    except RemoteCallError as e:
        # Child raised an exception
        print(f"Remote error: {e.error}")
        print(f"Traceback: {e.traceback}")
```

**In child worker**:

```python
class MyWorker(ChildWorker):
    def add(self, a: int, b: int) -> int:
        try:
            return a + b
        except Exception as e:
            # Log error
            self.logger.error(f"Add failed: {e}")
            # Re-raise to parent
            raise
```

### How do I implement graceful shutdown?

**Parent shutdown**:

```python
async def main():
    async with ParentWorker.spawn("worker.py") as worker:
        result = await worker.acall.add(1, 2)
        # Automatic cleanup on exit
```

**Child shutdown**:

```python
class MyWorker(ChildWorker):
    def __init__(self):
        super().__init__(service_id="my-service")
        self._running = True

    def run(self):
        # Main loop
        while self._running:
            # Handle requests
            pass

    def _stop(self):
        self._running = False
        # Cleanup resources
        super()._stop()
```

### How should I implement worker pools?

Use `ThreadPoolExecutor` for parallel processing:

```python
async def parallel_processing(worker):
    from concurrent.futures import ThreadPoolExecutor

    # List of tasks
    tasks = [
        worker.acall.process_item(i)
        for i in range(10)
    ]

    # Execute in parallel
    async with ThreadPoolExecutor(max_workers=5) as executor:
        results = await asyncio.gather(*tasks, return_exceptions=True)

    # Handle results
    for result in results:
        if isinstance(result, Exception):
            print(f"Error: {result}")
        else:
            print(f"Result: {result}")
```

### How do I monitor worker health?

**Heartbeat monitoring**:

```python
async with ParentWorker.spawn("worker.py") as worker:
    # Check RTT
    rtt = await worker.heartbeat()
    print(f"RTT: {rtt:.3f}ms")

    # Monitor circuit breaker
    if worker.circuit_state == "open":
        print("WARNING: Circuit breaker is open!")
```

**Metrics monitoring**:

```python
async with ParentWorker.spawn("worker.py") as worker:
    # Check success rate
    metrics = worker.metrics.snapshot()
    if metrics.success_rate < 0.9:
        print("WARNING: Low success rate!")

    # Check latency
    if metrics.latency_p95 > 100:
        print("WARNING: High latency!")
```

### How should I handle authentication?

Use message metadata for authentication:

```python
# In parent
async with ParentWorker.spawn("worker.py") as worker:
    result = await worker.acall.add(
        1, 2,
        metadata={"user_id": "123", "token": "abc"}
    )

# In child
class MyWorker(ChildWorker):
    def add(self, a: int, b: int, metadata: dict = None):
        # Validate authentication
        if not metadata or "user_id" not in metadata:
            raise ValueError("Unauthorized")

        user_id = metadata["user_id"]
        return a + b
```

### How do I implement retries?

Use `asyncio.wait_for` with retry logic:

```python
from tenacity import retry, stop_after_attempt, wait_exponential

class MyWorker(ChildWorker):
    @retry(
        stop=stop_after_attempt(3),
        wait=wait_exponential(multiplier=1, min=1, max=10)
    )
    def add(self, a: int, b: int) -> int:
        # May raise RemoteCallError
        return a + b
```

Or in parent:

```python
from multifrost import ParentWorker

async def call_with_retry(worker, func, *args, max_retries=3):
    for attempt in range(max_retries):
        try:
            return await worker.acall(func, *args)
        except RemoteCallError:
            if attempt == max_retries - 1:
                raise
            await asyncio.sleep(1.0)
```

---

## Cross-Language Communication

### What data types are supported?

| Python Type | msgpack Encoding | Notes |
|-------------|------------------|-------|
| `str` | UTF-8 string | |
| `bytes` | Bin type | |
| `int` | Int64 | Clamped to ±2^63-1 |
| `float` | Float64 | NaN/Inf → `null` |
| `list` | Array | |
| `dict` | Map | Keys converted to str |
| `None` | `null` | |
| `bool` | bool | |
| Custom objects | `msgpack.ExtType` | Requires pack/unpack methods |

**Example**:

```python
# In Python child
class Person:
    def __init__(self, name: str, age: int):
        self.name = name
        self.age = age

    def msgpack_encode(self):
        return msgpack.ExtType(1, msgpack.packb(self.__dict__))

    @classmethod
    def msgpack_decode(cls, code, data):
        if code == 1:
            return cls(**msgpack.unpackb(data))
        raise ValueError(f"Unknown code: {code}")

# Use in worker
person = Person("Alice", 30)
```

### How does msgpack handle edge cases?

**NaN/Infinity**:
```python
# Python
data = {"value": float("nan"), "infinity": float("inf")}
# msgpack → {"value": null, "infinity": null}
```

**Integer overflow**:
```python
# Python
data = {"big": 2**63 + 1}
# msgpack → {"big": 9223372036854775807} (clamped)
```

**Binary data**:
```python
# Python
data = {"image": b"\x00\x01\x02\x03"}
# msgpack → bin type
```

### How do I handle large messages?

**Size limits**:
- Max bin length: 10MB
- Max string length: 10MB
- Max array length: 100,000
- Max map length: 100,000

**Solution**: Stream large data or split into chunks:

```python
# Chunk large data
chunk_size = 1024 * 1024  # 1MB

async with ParentWorker.spawn("worker.py") as worker:
    # Upload in chunks
    for i in range(0, len(data), chunk_size):
        chunk = data[i:i + chunk_size]
        await worker.acall.upload_chunk(chunk)
```

### How do I communicate between different languages?

**Key points**:
1. Use msgpack for serialization (cross-language compatible)
2. Use standard data types (no custom objects unless handled)
3. Follow type mapping rules
4. Sanitize data for msgpack

**Example: Python → JavaScript**:

```python
# Python child
class Calculator(ChildWorker):
    def calculate(self, a: int, b: int) -> int:
        return a + b

# JavaScript parent
const worker = await ParentWorker.connect("my-service", 5);
await worker.start();
const result = await worker.call.calculate(1, 2);
// result is 3
```

**Example: Python → Go**:

```python
# Python child
class DataProcessor(ChildWorker):
    def process(self, data: dict) -> dict:
        return {"result": "processed"}

# Go parent
worker, err := ParentWorker.Connect("my-service", 5)
if err != nil {
    log.Fatal(err)
}
defer worker.Close()

result, err := worker.Call.Process(map[string]interface{}{
    "input": "test",
})
```

### How do I handle encoding/decoding errors?

**In child**:

```python
from multifrost.core.message import Message

class MyWorker(ChildWorker):
    def process(self, data: bytes):
        try:
            decoded = msgpack.unpackb(data, raw=False)
            return decoded
        except msgpack.ExtraData as e:
            self.logger.error(f"Extra data in message: {e}")
            raise RemoteCallError("Invalid message format")
```

**In parent**:

```python
async with ParentWorker.spawn("worker.py") as worker:
    try:
        result = await worker.acall.process(data)
    except RemoteCallError as e:
        # Handle error from child
        print(f"Remote error: {e.error}")
```

---

## Additional Resources

- **Architecture docs**: `python/docs/arch.md`
- **API reference**: `python/multifrost/__init__.py`
- **Source code**: `python/multifrost/core/`
- **Tests**: `python/tests/`
- **Issues**: Report bugs on GitHub
- **Discussions**: Join the community forum

---

## Version Information

This FAQ covers Multifrost Python implementation. For version-specific changes, check the CHANGELOG.

**Last updated**: 2026-02-12
