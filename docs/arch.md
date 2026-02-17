# Multifrost Architecture

A language-agnostic IPC library for parent-child process communication over ZeroMQ.

## Overview

Multifrost enables a **Parent** process to call methods on a **Child** worker process as if they were local function calls. The library works across languages - a Python parent can call a Node.js child, and vice versa.

```
┌─────────────┐                      ┌─────────────┐
│   Parent    │  ───── CALL ──────>  │    Child    │
│  (any lang) │  <──── RESPONSE ───  │  (any lang) │
└─────────────┘                      └─────────────┘
     DEALER                             ROUTER
        │                                  │
        └──────── ZeroMQ over TCP ─────────┘
                 msgpack encoded
```

## Core Concepts

### Roles

| Role | Responsibility | Socket Type |
|------|---------------|-------------|
| **Parent** | Initiates calls, manages child lifecycle | DEALER |
| **Child** | Exposes callable methods, handles requests | ROUTER |

### Lifecycle Modes

| Mode | Who Binds | Who Connects | Use Case |
|------|-----------|--------------|----------|
| **Spawn** | Parent binds, Child connects | Parent owns child process | Parent controls worker lifetime |
| **Connect** | Child binds, Parent connects | Child registers with discovery | Long-running services |

> **Spawn** creates a full OS process. The executable can be any language runtime (python, node, etc.).

## Wire Protocol

### Transport

- **Protocol**: ZeroMQ over TCP
- **Pattern**: ROUTER (Child) <-> DEALER (Parent)
- **Encoding**: msgpack (map/dict at top level)
- **App ID**: `comlink_ipc_v4`

### Multipart Framing

```
Parent sends:    [empty_frame, message_bytes]
Child receives:  [sender_id, empty_frame, message_bytes]
Child responds:  [sender_id, empty_frame, message_bytes]
Parent receives: [empty_frame, message_bytes]
```

### Message Schema

All messages share core fields:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `app` | string | Yes | Must be `comlink_ipc_v4` |
| `id` | string | Yes | UUID v4, correlates request/response |
| `type` | string | Yes | Message type (see below) |
| `timestamp` | number | Yes | Unix epoch seconds (float) |

### Message Types

#### CALL
```json
{
  "app": "comlink_ipc_v4",
  "id": "<uuid>",
  "type": "call",
  "timestamp": 1234.5,
  "function": "methodName",
  "args": [arg1, arg2],
  "namespace": "default",
  "client_name": "optional"
}
```

- `client_name`: In connect mode, helps parent identify which worker to use without manually starting it

#### RESPONSE
```json
{
  "app": "comlink_ipc_v4",
  "id": "<same-as-call>",
  "type": "response",
  "timestamp": 1234.6,
  "result": <any-value>
}
```

#### ERROR
```json
{
  "app": "comlink_ipc_v4",
  "id": "<same-as-call>",
  "type": "error",
  "timestamp": 1234.6,
  "error": "Error message with optional traceback"
}
```

#### STDOUT / STDERR
```json
{
  "app": "comlink_ipc_v4",
  "type": "stdout",
  "timestamp": 1234.5,
  "output": "printed text"
}
```

> **Note**: These are broadcast to all connected parents in multi-parent scenarios.

#### HEARTBEAT / SHUTDOWN
```json
{
  "app": "comlink_ipc_v4",
  "id": "<uuid>",
  "type": "heartbeat",
  "timestamp": 1234.5
}
```

- **HEARTBEAT**: Reserved for future use (connection health monitoring)
- **SHUTDOWN**: Child should stop processing when received

## Lifecycle Flows

### Spawn Mode

```
1. Parent finds free port
2. Parent binds DEALER to tcp://*:<port>
3. Parent spawns child with COMLINK_ZMQ_PORT=<port> env
4. Child creates ROUTER, connects to tcp://localhost:<port>
5. Parent sends CALL [empty, payload]
6. Child receives [sender_id, empty, payload]
7. Child responds [sender_id, empty, payload]
8. Parent matches id, resolves promise/future
9. On stop: Parent closes socket, terminates child
```

### Connect Mode

```
1. Child registers service_id in ~/.multifrost/services.json
2. Child binds ROUTER to tcp://*:<port>
3. Parent discovers service_id, gets port
4. Parent connects DEALER to tcp://localhost:<port>
5. Parent sends CALL; Child responds
6. Child unregisters on shutdown (if PID matches)
```

## Service Registry

Location: `~/.multifrost/services.json`

```json
{
  "my-service": {
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
- **Atomic file locking**: Uses `O_CREAT | O_EXCL` for lock acquisition to prevent race conditions
- **Discovery polling**: 100ms interval, 5s default timeout
- **Lock timeout**: 10s max wait for registry lock
- **Graceful failure**: Lock acquisition failures are logged but don't crash the process

## API Surface (Language-Agnostic)

### ParentWorker

```
// Factory methods
ParentWorker.spawn(scriptPath, executable?, options?) -> ParentWorker
ParentWorker.connect(serviceId, timeout?) -> ParentWorker

// Options (spawn mode)
options: {
    autoRestart?: boolean,
    maxRestartAttempts?: number,    // default: 5
    defaultTimeout?: number,        // ms
    heartbeatInterval?: number,     // seconds, default: 5.0
    heartbeatTimeout?: number,      // seconds, default: 3.0
    heartbeatMaxMisses?: number,    // default: 3
}

// Handle methods (lifecycle + call interface)
worker.handle() -> Handle          // async handle (default)
worker.handle_sync() -> Handle     // sync handle (Python only)

// Handle lifecycle (delegates to worker internals)
handle.start()                     // spawn/connect (mode: sync or async)
handle.stop()                      // cleanup (mode: sync or async)

// Handle remote calls
handle.call.methodName(...args)
handle.call.withOptions({timeout?, namespace?}).methodName(...args)

// Worker properties (introspection)
worker.isHealthy -> boolean         // circuit breaker not tripped
worker.circuitOpen -> boolean       // circuit breaker state
worker.lastHeartbeatRttMs -> number | undefined  // last heartbeat RTT
worker.metrics -> Metrics           // metrics collector (Python/JS)
```

> **Pattern**: Worker holds state/config. Handle is a lightweight interface that delegates to worker's internal lifecycle. One handle type = one execution mode.

### ChildWorker

```
// Constructor
ChildWorker(serviceId?) -> ChildWorker

// Lifecycle
child.run() -> void  // blocking with signal handling
child.start() -> Promise<void>
child.stop() -> void

// Introspection
child.listFunctions() -> string[]
```

## Implementation Requirements

### Parent Must

- [ ] Create DEALER socket before sending calls
- [ ] Track pending requests keyed by `id`
- [ ] Resolve pending on RESPONSE, reject on ERROR
- [ ] Set `COMLINK_ZMQ_PORT` env in spawn mode
- [ ] Handle timeout with appropriate error
- [ ] Log STDOUT/STDERR with context prefix
- [ ] Retry send up to 5 times on socket busy (default)
- [ ] Reject all pending requests with error if child crashes
- [ ] Handle SIGINT/SIGTERM for graceful shutdown

### Child Must

- [ ] Create ROUTER socket
- [ ] Ignore messages where `app != comlink_ipc_v4`
- [ ] Ignore messages with mismatched `namespace`
- [ ] Reject calls to functions starting with `_`
- [ ] Send ERROR for missing/non-callable functions
- [ ] Continue loop after handling errors
- [ ] Validate port is in range 1024-65535, exit if invalid
 - [ ] Handle SIGINT/SIGTERM for graceful shutdown
 - [ ] Support both sync and async method handlers
 - [ ] Use dedicated event loop for async handlers (Python: run_coroutine_threadsafe)
 - [ ] Include timeout for async function calls (Python: 30s default)

### Both Must

- [ ] Use msgpack encoding with map top-level
- [ ] Generate UUID v4 for message ids
- [ ] Preserve `id` across request/response
- [ ] Ignore unknown message fields
- [ ] Support namespace filtering (default: `default`)
- [ ] Use default socket options unless specifically needed
- [ ] Send all messages (including STDOUT/STDERR) with proper multipart framing

 ### Optional Features

 - [ ] **Auto-restart**: Parent may auto-restart crashed child (configurable attempts)
 - [ ] **client_name**: For connect mode, helps parent identify its worker

### Reliability Improvements

The Python implementation includes several reliability enhancements:

#### Service Registry Locking
- Uses atomic file creation (`os.open()` with `O_CREAT | O_EXCL`) to prevent TOCTOU race conditions
- Lock file is removed after release to prevent stale locks
- Lock acquisition failures are logged gracefully without crashing

#### Async Function Handling
- ChildWorker maintains a dedicated event loop in a daemon thread for async method handlers
- Uses `asyncio.run_coroutine_threadsafe()` instead of creating new threads per call
- 30-second timeout on async function calls to prevent hangs
- Proper cleanup of async loop on shutdown

#### Output Forwarding
- `_send_output()` retries failed sends (2 attempts, 1ms delay)
- Distinguishes between retryable errors (`zmq.Again`) and fatal errors
- Logs warnings for send failures instead of silently ignoring

#### Resource Cleanup
- `__del__` performs minimal synchronous cleanup without relying on event loop
- Directly closes socket, terminates process, and terminates context
- Uses `hasattr()` checks to safely access attributes during garbage collection
- Prevents race conditions where cleanup tasks may never run if loop is closing

## Cross-Language Compatibility

The wire protocol is identical across implementations:

| Feature | Python | JavaScript |
|---------|--------|------------|
| App ID | `comlink_ipc_v4` | `comlink_ipc_v4` |
| Core fields | app, id, type, timestamp | app, id, type, timestamp |
| CALL fields | function, args, namespace | function, args, namespace |
| ERROR format | message + traceback | message only |
| STDOUT forwarding | Yes (multipart) | Yes (multipart) |
| Spawn arg | `--worker` added | No extra arg |
| Socket options | defaults | defaults |
| Send retries | 5 | 5 |
| Auto-restart | Yes | Yes |
| Circuit Breaker | Yes | Yes |
| Heartbeat Monitoring | Yes | Yes |
| Metrics | Yes | Yes |
| Structured Logging | Yes | Yes |
| Default Timeout | Yes | Yes |
| Sync Calls | Yes | No (async-only) |

> **Async Nature**: This library is async-native. Adapters should use their language's idiomatic async approach (asyncio, Promises, etc.) and support both sync and async method handlers in Child. JavaScript is async-only (no sync mode) due to Node.js's single-threaded nature.

### Example: Python Parent, JS Child

```python
# Python parent (async)
worker = ParentWorker.spawn("./math_worker.ts", "tsx.cmd")
handle = worker.handle()
await handle.start()
result = await handle.call.factorial(10)
await handle.stop()
```

```typescript
// JS child
class MathWorker extends ChildWorker {
  factorial(n: number): number { ... }
}
new MathWorker().run();
```

### Example: JS Parent, Python Child

```typescript
// JS parent
const worker = ParentWorker.spawn("./math_worker.py", "python");
const handle = worker.handle();
await handle.start();
const result = await handle.call.factorial(10);
await handle.stop();
```

```python
# Python child
class MathWorker(ChildWorker):
    def factorial(self, n): ...
MathWorker().run()
```

## Error Handling

| Condition | Response |
|-----------|----------|
| Missing `function` field | ERROR: "Message missing function field" |
| Missing `id` field | ERROR: "Message missing id field" |
| Function not found | ERROR: "Function X not found" |
| Not callable | ERROR: "X is not callable" |
| Private method (`_foo`) | ERROR: "Cannot call private method X" |
| Invalid app id | Message ignored |
| Namespace mismatch | Message ignored |
 | Timeout | Parent raises TimeoutError / rejects Promise |
 | Child crash | Parent rejects pending with RemoteCallError |
 | Invalid port (spawn) | Child exits immediately |
 | Output send failure | Child retries up to 2 times with 1ms delay, logs warnings |
 | Registry lock failure | Logs warning, continues without crashing |

 ## Concurrency

 - Parent may issue concurrent calls (tracked by `id`)
 - Responses may arrive out of order (matched by `id`)
 - Child processes one message at a time in its loop
 - **Python async handling**: ChildWorker uses a dedicated event loop in a daemon thread for async function calls, avoiding thread-per-call overhead
 - **Cleanup**: `__del__` performs minimal synchronous cleanup without relying on event loop to prevent race conditions during garbage collection

## Security

- Protocol is unauthenticated and unencrypted
- Only use on localhost or trusted networks
- Do not expose ports to untrusted clients
- Validate inputs in worker methods (args are untrusted)

## Versioning

- App ID `comlink_ipc_v4` identifies protocol version
- Different app ID = hard fail (no negotiation)
- New fields must be optional; receivers ignore unknowns

## Large Payloads

- No chunking defined
- Message size is sender's responsibility
- Bounded by ZMQ and memory limits
