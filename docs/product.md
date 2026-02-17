# Multifrost Product Specification

**Version**: 1.0  
**Protocol ID**: `comlink_ipc_v3`  
**Status**: Canonical

This document defines what Multifrost is, what every feature does, and how it behaves. It is the authoritative specification. If code and this document disagree, this document wins.

---

## 1. Product Overview

Multifrost is a multilanguage IPC (Inter-Process Communication) library that enables a **Parent** process to call methods on a **Child** worker process as if they were local function calls.

### Core Value Proposition

- **Cross-language**: A parent in one language can call a child in any other supported language
- **Transparent RPC**: Remote method calls look like local function calls
- **Process isolation**: Child runs in separate OS process with independent memory and lifecycle
- **Two modes**: Spawn (parent owns child) or Connect (connect to existing service)

### Problem Solved

| Without Multifrost | With Multifrost |
|--------------------|-----------------|
| Manual socket management | ZeroMQ abstraction |
| Custom serialization | msgpack auto-handled |
| Language-specific IPC | Cross-language interoperability |
| No fault tolerance | Circuit breaker, heartbeat, auto-restart |
| No observability | Built-in metrics and structured logging |

---

## 2. Supported Languages

| Language | Minimum Version | Package Name | Language Docs |
|----------|-----------------|--------------|---------------|
| **Python** | 3.10+ | `multifrost` | `python/docs/arch.md` |
| **JavaScript** | Node.js 18+ | `multifrost` | `javascript/docs/arch.md` |
| **Go** | 1.21+ | `multifrost` | `golang/docs/arch.md` |
| **Rust** | 1.70+ | `multifrost` | `rust/docs/arch.md` |

All implementations share the same wire protocol and are fully interoperable.

---

## 3. Core Concepts

### 3.1 Roles

| Role | Responsibility | Socket Type |
|------|----------------|-------------|
| **Parent** | Initiates calls, manages child lifecycle (in spawn mode) | DEALER |
| **Child** | Exposes callable methods, handles requests | ROUTER |

A process can be both a parent (calling other workers) and a child (exposing methods to other parents).

### 3.2 Lifecycle Modes

| Mode | Who Binds | Who Connects | Use Case |
|------|-----------|--------------|----------|
| **Spawn** | Parent binds, Child connects | Parent owns child process | Parent controls worker lifetime |
| **Connect** | Child binds, Parent connects | Child registers with discovery | Long-running services, multiple parents |

**Spawn mode**: Parent creates a full OS process. The executable can be any language runtime (python, node, go, etc.). Parent is responsible for child lifecycle.

**Connect mode**: Child registers itself in a service registry. Parent discovers and connects. Child may serve multiple parents. Child controls its own lifecycle.

### 3.3 Namespaces

Namespaces route messages to specific method groups within a child.

- Default namespace: `"default"`
- Child ignores calls with mismatched namespace
- Parent can specify namespace per-call

### 3.4 Service Registry

Location: `~/.multifrost/services.json`

Used in connect mode for service discovery. Contains:

```json
{
  "service-id": {
    "port": 5555,
    "pid": 12345,
    "started": "2026-02-11T12:00:00"
  }
}
```

Rules:
- Registration fails if service_id exists with live PID
- Dead PIDs are overwritten
- Unregister only removes if PID matches
- Atomic file locking prevents race conditions

---

## 4. API Surface (Language-Agnostic)

### 4.1 ParentWorker

#### Factory Methods

| Method | Description |
|--------|-------------|
| `ParentWorker.spawn(scriptPath, executable?, options?)` | Create parent that owns child process |
| `ParentWorker.connect(serviceId, timeout?)` | Connect to existing service by ID |

#### Options (Spawn Mode)

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `autoRestart` | boolean | false | Restart child on crash |
| `maxRestartAttempts` | number | 5 | Max restart attempts before giving up |
| `defaultTimeout` | number | none | Default call timeout in milliseconds |
| `heartbeatInterval` | number | 5.0 | Seconds between heartbeats |
| `heartbeatTimeout` | number | 3.0 | Seconds to wait for heartbeat response |
| `heartbeatMaxMisses` | number | 3 | Consecutive misses before circuit trips |

#### Lifecycle Methods

| Method | Description |
|--------|-------------|
| `start()` | Start the worker (bind socket, spawn child or connect) |
| `close()` / `stop()` | Stop the worker (close socket, terminate child if spawned) |

#### Remote Calls

| Syntax | Description |
|--------|-------------|
| `parent.call.methodName(...args)` | Call remote method (language-specific async/sync) |
| `parent.call.withOptions({timeout?, namespace?}).methodName(...args)` | Call with per-call options |

#### Properties

| Property | Type | Description |
|----------|------|-------------|
| `isHealthy` | boolean | Circuit breaker not tripped |
| `circuitOpen` | boolean | Circuit breaker state |
| `lastHeartbeatRttMs` | number \| undefined | Last heartbeat round-trip time |
| `metrics` | Metrics | Performance metrics collector |

### 4.2 ChildWorker

#### Constructor

| Syntax | Description |
|--------|-------------|
| `ChildWorker(serviceId?)` | Create child worker, optionally with service ID for connect mode |

#### Lifecycle Methods

| Method | Description |
|--------|-------------|
| `run()` | Blocking run loop with signal handling |
| `start()` | Start the worker (non-blocking where applicable) |
| `stop()` | Stop the worker |

#### Introspection

| Method | Description |
|--------|-------------|
| `listFunctions()` | List all callable method names |

### 4.3 Per-Language Syntax

#### ParentWorker Creation

| Language | Spawn | Connect |
|----------|-------|---------|
| Python | `ParentWorker.spawn("worker.py", "python")` | `await ParentWorker.connect("my-service")` |
| JavaScript | `ParentWorker.spawn("worker.ts", "tsx")` | `await ParentWorker.connect("my-service")` |
| Go | `multifrost.Spawn("worker", "go")` | `multifrost.Connect("my-service")` |
| Rust | `ParentWorker::spawn("worker.rs", "cargo")` | `ParentWorker::connect("my-service").await` |

#### Remote Call Syntax

| Language | Async Call | Sync Call |
|----------|------------|-----------|
| Python | `await worker.acall.method(args)` | `worker.sync.call.method(args)` |
| JavaScript | `await worker.call.method(args)` | N/A (async-only) |
| Go | `worker.Call("method", args...)` | Same (goroutines for concurrency) |
| Rust | `worker.call("method", args...).await` | `worker.call_blocking("method", args...)` |

#### ChildWorker Definition

| Language | Syntax |
|----------|--------|
| Python | `class MyWorker(ChildWorker): def method(self, ...): ...` |
| JavaScript | `class MyWorker extends ChildWorker { method() { ... } }` |
| Go | Implement `Worker` interface with methods |
| Rust | Implement `Worker` trait |

---

## 5. Lifecycle Modes

### 5.1 Spawn Mode

Parent owns the child process lifecycle.

**Flow:**

1. Parent finds free port
2. Parent binds DEALER to `tcp://*:<port>`
3. Parent spawns child with `COMLINK_ZMQ_PORT=<port>` env
4. Child creates ROUTER, connects to `tcp://localhost:<port>`
5. Parent sends CALL messages
6. Child processes and responds
7. On stop: Parent closes socket, terminates child

**Environment Variables (set by parent):**

| Variable | Description |
|----------|-------------|
| `COMLINK_ZMQ_PORT` | Port for child to connect to |
| `COMLINK_WORKER_MODE` | Set to `"1"` (hint for child) |

**Child behavior on invalid port:**
- Port outside 1024-65535: Child exits immediately
- Missing port env: Child exits immediately

### 5.2 Connect Mode

Child registers itself; parent discovers and connects.

**Flow:**

1. Child registers service_id in `~/.multifrost/services.json`
2. Child binds ROUTER to `tcp://*:<port>`
3. Parent discovers service_id, gets port from registry
4. Parent connects DEALER to `tcp://localhost:<port>`
5. Parent sends CALL; Child responds
6. Child unregisters on shutdown (if PID matches)

**Discovery behavior:**
- Polling interval: 100ms
- Default timeout: 5 seconds
- Lock timeout: 10 seconds max

**Multiple parents:**
- Child ROUTER supports multiple connected parents
- Each parent has unique sender_id
- Responses routed by sender_id

---

## 6. Remote Calls

### 6.1 Method Invocation

Parent calls methods on child as if local:

```
result = await parent.call.methodName(arg1, arg2)
```

**Behavior:**
- Method name must exist on child
- Arguments serialized via msgpack
- Response deserialized and returned
- Errors raised as `RemoteCallError`

### 6.2 Timeouts

| Mode | Behavior |
|------|----------|
| Per-call timeout | `parent.call.withOptions({timeout: 5000}).method()` |
| Default timeout | Set via `defaultTimeout` option on spawn |
| No timeout | Call waits indefinitely (not recommended) |

**On timeout:**
- Python: Raises `TimeoutError`
- JavaScript: Promise rejects with timeout error
- Go: Returns error
- Rust: Returns `Err`

### 6.3 Error Handling

| Condition | Response |
|-----------|----------|
| Method not found | ERROR: "Function X not found" |
| Method not callable | ERROR: "X is not callable" |
| Private method (`_foo`) | ERROR: "Cannot call private method X" |
| Exception in method | ERROR: message + traceback (if available) |
| Timeout | Parent raises TimeoutError |
| Child crash | Parent rejects pending with RemoteCallError |

### 6.4 Concurrent Calls

- Parent may issue concurrent calls
- Each call has unique `id` for correlation
- Responses may arrive out of order
- Parent matches response to pending request by `id`
- Child processes one message at a time (sequential)

---

## 7. Reliability Features

### 7.1 Circuit Breaker

Prevents cascading failures by stopping calls to unhealthy children.

**States:**
- **Closed**: Normal operation, calls proceed
- **Open**: Too many failures, calls rejected immediately

**Trip conditions:**
- `maxRestartAttempts` consecutive call failures (default: 5)
- `heartbeatMaxMisses` consecutive heartbeat timeouts (default: 3)
- Child process exit (spawn mode)

**Reset conditions:**
- Successful call after failures

**When open:**
- All calls raise `CircuitOpenError`
- No network traffic sent

### 7.2 Heartbeat Monitoring

Periodic health checks between parent and child.

**Configuration:**

| Option | Default | Description |
|--------|---------|-------------|
| `heartbeatInterval` | 5.0 seconds | Time between heartbeats |
| `heartbeatTimeout` | 3.0 seconds | Time to wait for response |
| `heartbeatMaxMisses` | 3 | Misses before circuit trips |

**Behavior:**
- Parent sends HEARTBEAT message
- Child echoes back immediately
- Parent calculates RTT from timestamp
- Consecutive misses trip circuit breaker

### 7.3 Auto-Restart

Automatically restart crashed child (spawn mode only).

**Configuration:**

| Option | Default | Description |
|--------|---------|-------------|
| `autoRestart` | false | Enable auto-restart |
| `maxRestartAttempts` | 5 | Max attempts before giving up |

**Behavior:**
- On child crash, parent restarts with same configuration
- Restart count tracked per-worker
- After max attempts, circuit breaker trips

### 7.4 Metrics

Built-in performance tracking.

**Available metrics:**

| Metric | Description |
|--------|-------------|
| `totalCalls` | Total number of calls made |
| `successfulCalls` | Calls that returned successfully |
| `failedCalls` | Calls that returned errors |
| `avgLatencyMs` | Average call latency |
| `p50LatencyMs` | 50th percentile latency |
| `p95LatencyMs` | 95th percentile latency |
| `p99LatencyMs` | 99th percentile latency |
| `circuitTrips` | Number of circuit breaker trips |
| `heartbeatRttMs` | Last heartbeat RTT |

---

## 8. Output Handling

### 8.1 STDOUT/STDERR Forwarding

Child process output is forwarded to parent.

**Behavior:**
- Child captures stdout/stderr writes
- Child sends OUTPUT message to parent
- Parent logs with context prefix: `[script_name STDOUT]: message`

**Reliability:**
- Output messages are best-effort
- May be dropped under high load
- Retries on send failure (up to 2 attempts)

**Language differences:**

| Language | STDOUT Forwarding | STDERR Forwarding |
|----------|-------------------|-------------------|
| Python | Yes (via ZMQWriter) | Yes (via ZMQWriter) |
| JavaScript | Yes (console override) | Yes (console override) |
| Go | Yes | Yes |
| Rust | Yes | Yes |

---

## 9. Error Handling

### 9.1 Error Types

| Error Type | Cause | Recoverable |
|------------|-------|-------------|
| `RemoteCallError` | Child returned error | Yes (fix child code) |
| `TimeoutError` | Call exceeded timeout | Yes (increase timeout or fix slow method) |
| `CircuitOpenError` | Circuit breaker tripped | Yes (wait for recovery or manual reset) |
| `ConnectionError` | Cannot connect to child | Yes (check child is running) |
| `RegistryError` | Service registry issue | Yes (check registry file) |

### 9.2 Error Propagation

**From child to parent:**

1. Child catches exception in method
2. Child creates ERROR message with error string
3. Child includes traceback if available
4. Parent receives ERROR, raises `RemoteCallError`

**Error message format:**

```
ErrorType: message
[Traceback: ...]
```

### 9.3 Recovery Behavior

| Scenario | Recovery |
|----------|----------|
| Single call failure | Return error to caller |
| Consecutive failures | Circuit breaker trips |
| Child crash | Auto-restart (if enabled) or circuit trips |
| Heartbeat miss | Increment miss counter |
| Max heartbeat misses | Circuit breaker trips |

---

## 10. Security Model

### 10.1 Trust Model

**The protocol is unauthenticated and unencrypted.**

| Threat | Mitigation |
|--------|------------|
| Eavesdropping | Use only on localhost or trusted networks |
| Man-in-the-middle | Use only on localhost or trusted networks |
| Unauthorized access | Do not expose ports to untrusted clients |
| Malicious input | Validate all inputs in worker methods |

### 10.2 Safe Usage Patterns

**Safe:**
- Parent and child on same machine (localhost)
- Parent and child on trusted private network
- Behind firewall with restricted access

**Unsafe:**
- Exposing child port to public internet
- Accepting calls from untrusted parents
- Processing unvalidated user input in child methods

### 10.3 Input Validation

All arguments passed to child methods are untrusted. Child methods MUST validate:

- Argument types
- Argument ranges
- Argument content (no injection attacks)

---

## 11. Cross-Language Interoperability

### 11.1 Wire Protocol

All languages use identical wire protocol:

- **Transport**: ZeroMQ over TCP
- **Pattern**: ROUTER (Child) <-> DEALER (Parent)
- **Encoding**: msgpack
- **App ID**: `comlink_ipc_v3`

See `docs/protocol.md` for full specification.

### 11.2 Type Compatibility

**Safe types (always work):**

| Type | Notes |
|------|-------|
| Strings (UTF-8) | Always safe |
| Integers in [-2^63, 2^63-1] | Safe across all languages |
| Floats (no NaN/Infinity) | Safe; convert NaN/Inf to null |
| Boolean | Safe |
| Null | Safe |
| Arrays | Safe |
| Objects with string keys | Safe |

**Problematic types:**

| Type | Issue | Solution |
|------|-------|----------|
| Integers > 2^63 | Overflow in Go/Rust | Clamp to int64 max |
| NaN / Infinity | Inconsistent handling | Convert to null |
| Non-string map keys | Breaks JS, confuses typed languages | Stringify keys |
| Binary data | String/bytes confusion | Use `use_bin_type=True` in Python |

See `docs/msgpack_interop.md` for detailed guidance.

### 11.3 Known Divergences

| Divergence | Languages | Impact |
|------------|-----------|--------|
| ERROR traceback | Python includes, JS does not | Parent sees more detail from Python |
| `--worker` arg | Python adds, JS does not | Internal only, no interop impact |
| Sync API | Python has, JS does not | API difference, same wire protocol |
| STDOUT forwarding | All forward, implementation differs | Same behavior to parent |

---

## 12. Guarantees and Non-Guarantees

### 12.1 Guarantees

| Guarantee | Description |
|-----------|-------------|
| **Message delivery** | Messages delivered in order on same connection |
| **Response correlation** | Responses matched to requests by ID |
| **Error propagation** | Child errors surfaced to parent |
| **Clean shutdown** | Resources released on close |
| **Cross-language calls** | Any parent can call any child |
| **Circuit breaker** | Stops calls to failing children |
| **Heartbeat detection** | Detects unresponsive children |

### 12.2 Non-Guarantees

| Non-Guarantee | Reason |
|---------------|--------|
| **Exactly-once delivery** | Network failures may cause duplicates or loss |
| **Message persistence** | Messages not persisted; lost on crash |
| **Ordered delivery across reconnects** | Order only guaranteed on single connection |
| **Real-time delivery** | No latency guarantees |
| **Security** | Protocol is unauthenticated and unencrypted |

### 12.3 Best-Effort Features

| Feature | Behavior |
|---------|----------|
| STDOUT/STDERR forwarding | May drop under load |
| Metrics accuracy | Approximate, not precise |
| Service registry | File-based, may have race conditions |

---

## 13. Versioning

### 13.1 Protocol Version

The protocol is identified by the `app` field in every message:

```
app: "comlink_ipc_v3"
```

**Rules:**
- Different app ID = hard fail (no negotiation)
- All implementations MUST use exact app ID
- Version changes require all participants to upgrade

### 13.2 Compatibility Rules

| Change Type | Compatibility |
|-------------|----------------|
| New optional field | Backward compatible |
| New message type | Backward compatible (ignored by old receivers) |
| Removed field | Breaking |
| Changed field type | Breaking |
| Changed app ID | Breaking |

### 13.3 Forward/Backward Compatibility

**Receivers MUST:**
- Ignore unknown fields
- Ignore unknown message types

**Senders MUST:**
- Include all required fields
- Not assume receivers understand new optional fields

---

## 14. Quick Reference

### 14.1 Minimal Parent (Spawn Mode)

```python
# Python
from multifrost import ParentWorker

async with ParentWorker.spawn("worker.py") as worker:
    result = await worker.acall.my_method(arg1, arg2)
```

```javascript
// JavaScript
import { ParentWorker } from 'multifrost';

const worker = await ParentWorker.spawn('worker.ts', 'tsx');
await worker.start();
const result = await worker.call.myMethod(arg1, arg2);
await worker.stop();
```

### 14.2 Minimal Child

```python
# Python
from multifrost import ChildWorker

class MyWorker(ChildWorker):
    def my_method(self, a, b):
        return a + b

MyWorker().run()
```

```javascript
// JavaScript
import { ChildWorker } from 'multifrost';

class MyWorker extends ChildWorker {
    myMethod(a, b) {
        return a + b;
    }
}

new MyWorker().run();
```

### 14.3 Connect Mode

```python
# Parent connects to existing service
worker = await ParentWorker.connect("my-service")
result = await worker.acall.my_method()
```

```python
# Child registers as service
class MyWorker(ChildWorker):
    pass

MyWorker(service_id="my-service").run()
```

---

## 15. Related Documentation

| Document | Purpose |
|----------|---------|
| `docs/arch.md` | Technical architecture, wire protocol, implementation requirements |
| `docs/protocol.md` | Normative wire protocol specification |
| `docs/support_matrix.md` | Feature parity across languages |
| `docs/msgpack_interop.md` | Cross-language serialization guide |
| `docs/api-vision.md` | Future API direction |
| `python/docs/arch.md` | Python-specific implementation |
| `javascript/docs/arch.md` | JavaScript-specific implementation |
| `golang/docs/arch.md` | Go-specific implementation |
| `rust/docs/arch.md` | Rust-specific implementation |
