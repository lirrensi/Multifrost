### HIGH — Python child output forwarding is framed incorrectly and can recurse into self-failure
**Location**: `python/src/multifrost/core/child.py:172`, `python/src/multifrost/core/child.py:181`
**Problem**: `ChildWorker._send_output()` writes raw payload bytes on a `ROUTER` socket instead of sending a `[sender_id, empty, payload]` envelope, and its error path prints to `sys.stderr` after `sys.stderr` has already been replaced by the ZMQ-backed writer.
**Impact**: stdout/stderr forwarding is unreliable or broken, and any output-send failure can recursively re-enter `_send_output()` until the worker crashes.
**Fix**: Track parent identities explicitly, send output with proper ROUTER multipart framing, and log transport failures to the original stderr or another non-forwarded sink.

### HIGH — Python timed-out async RPCs keep running in the background
**Location**: `python/src/multifrost/core/child.py:322`
**Problem**: async handlers are submitted with `run_coroutine_threadsafe()`, but timeout handling only stops waiting on `future.result(timeout=30.0)` and never cancels the coroutine.
**Impact**: timed-out requests can continue mutating state and holding resources after the caller already received an error.
**Fix**: cancel the submitted future on timeout/error and ensure cancellation is awaited or drained before returning.

### HIGH — Python auto-restart revives the child but not the parent receive loop
**Location**: `python/src/multifrost/core/async_worker.py:653`, `python/src/multifrost/core/async_worker.py:833`
**Problem**: `_zmq_event_loop()` breaks immediately after `_handle_child_exit()`, while `_attempt_restart()` only spawns a new child and never starts a new receive task.
**Impact**: auto-restart leaves the worker permanently deaf; the child may restart, but responses and heartbeats are no longer consumed.
**Fix**: restart the full transport lifecycle, including the message loop, after a successful child restart.

### HIGH — JavaScript heartbeat timeout uses seconds as milliseconds
**Location**: `javascript/src/multifrost.ts:615`
**Problem**: `heartbeatTimeout` is configured in seconds everywhere else, but `setTimeout()` receives the raw value without multiplying by 1000.
**Impact**: the default `3.0` times out after roughly 3ms, creating false heartbeat failures and bogus circuit trips.
**Fix**: convert heartbeat timeout to milliseconds before passing it to `setTimeout()`.

### HIGH — JavaScript auto-restart has the same dead-receiver failure mode as Python
**Location**: `javascript/src/multifrost.ts:408`, `javascript/src/multifrost.ts:669`
**Problem**: `startMessageLoop()` breaks after `_handleChildExit()`, and `_attemptRestart()` spawns a replacement child without recreating the receive loop.
**Impact**: a restarted child comes back up with no active parent consumer, so the worker stays functionally dead.
**Fix**: restart the message loop together with the child process, or keep the original loop alive across restart.

### HIGH — JavaScript MessagePack handling is not int64-safe across languages
**Location**: `javascript/src/multifrost.ts:14`, `javascript/src/multifrost.ts:150`, `javascript/src/multifrost.ts:157`
**Problem**: encoding/decoding uses default `msgpackr` behavior and the sanitizer does not enforce the documented int64-safe range or BigInt-aware mode.
**Impact**: large integers can be truncated or lose precision when crossing JS/Python/Go/Rust boundaries.
**Fix**: use explicit int64-safe msgpack configuration and sanitize numeric values to the documented cross-language range.

### HIGH — Go child shutdown can hang forever on signals
**Location**: `golang/child_worker.go:497`, `golang/child_worker.go:548`
**Problem**: `Stop()` never closes `w.done`, while `Run()` blocks on `<-w.done` after starting the worker.
**Impact**: SIGINT/SIGTERM shutdowns can stall indefinitely, forcing the parent to kill the process instead of getting a clean exit.
**Fix**: close `w.done` exactly once during shutdown and make `Run()` exit on that signal.

### HIGH — Go reflection dispatch can crash the whole child on malformed calls
**Location**: `golang/child_worker.go:440`, `golang/child_worker.go:454`
**Problem**: `handleFunctionCall()` does not validate argument count and calls `reflect.Call()` without `recover()`.
**Impact**: a bad cross-language request or a panic inside worker code tears down the entire child instead of returning an `ERROR` message and continuing the loop.
**Fix**: validate arity/types before invocation and wrap method execution with panic recovery that converts failures into protocol errors.

### HIGH — Go parent does not reliably detect child crashes
**Location**: `golang/parent_worker.go:543`, `golang/parent_worker.go:658`
**Problem**: crash detection relies on `ProcessState`, but `ProcessState` is not populated until `Wait()` runs, and no background waiter exists until shutdown.
**Impact**: dead children are not detected promptly, pending calls are not failed on crash, and auto-restart logic cannot behave correctly.
**Fix**: start a dedicated `Wait()` monitor for spawned children and route exits through one crash-handling path.

### HIGH — Rust leaks timed-out RPC state in the pending map
**Location**: `rust/src/parent.rs:351`, `rust/src/parent.rs:375`, `rust/src/parent.rs:379`
**Problem**: timed-out calls return early without removing their `msg_id` from `pending`.
**Impact**: repeated timeouts create unbounded growth in the pending-request map and leak oneshot senders until shutdown.
**Fix**: remove the pending entry on every timeout/cancellation path before returning.

### HIGH — Rust cannot target non-default namespaces
**Location**: `rust/src/message.rs:148`, `rust/src/parent.rs:334`, `rust/src/parent.rs:346`
**Problem**: the Rust parent hardcodes `namespace = "default"` and exposes no per-call override.
**Impact**: Rust parents cannot call workers that rely on namespace routing, which violates the cross-language API contract.
**Fix**: add per-call namespace options and pass them through message creation and logging.

### HIGH — Rust serializes protocol payloads through `serde_json::Value`
**Location**: `rust/src/message.rs:58`, `rust/src/message.rs:64`, `rust/src/message.rs:76`, `rust/src/message.rs:262`
**Problem**: args, results, and metadata are decoded into JSON values instead of a lossless MessagePack value model.
**Impact**: binary/ext payloads and other non-JSON-safe MessagePack values cannot round-trip correctly across languages.
**Fix**: use a msgpack-native value representation for wire data, then convert to typed application values at the boundary.

### MEDIUM — Python heartbeat miss handling uses the wrong trip threshold
**Location**: `python/src/multifrost/core/async_worker.py:447`, `python/src/multifrost/core/async_worker.py:794`
**Problem**: heartbeat exhaustion calls `_record_failure()`, which opens the circuit based on `max_restart_attempts` instead of `heartbeat_max_misses`.
**Impact**: the worker can keep accepting calls after exceeding the configured missed-heartbeat limit.
**Fix**: trip the circuit directly when consecutive misses reach `heartbeat_max_misses`.

### MEDIUM — Python sync handle path is wired to the wrong API surface
**Location**: `python/src/multifrost/core/sync_wrapper.py:177`, `python/src/multifrost/core/async_worker.py:249`
**Problem**: `SyncWrapper.call()` invokes `self._worker.call(...)`, but `ParentWorker.call` is a proxy property, not an async call method.
**Impact**: the documented sync-handle path fails at runtime instead of executing the remote call.
**Fix**: route sync calls to the worker’s actual async call implementation rather than the proxy property.

### MEDIUM — JavaScript child retains every parent routing ID forever
**Location**: `javascript/src/multifrost.ts:860`, `javascript/src/multifrost.ts:977`
**Problem**: `_connectedParentIds` only grows; disconnected or dead parents are never removed.
**Impact**: long-lived connect-mode workers accumulate stale buffers and keep retrying output sends to parents that no longer exist.
**Fix**: expire dead parent IDs or maintain membership from active socket traffic with cleanup on send failure/disconnect.

### MEDIUM — Go rewrites NaN/Infinity to `0` instead of `null`
**Location**: `golang/message.go:142`, `golang/message.go:170`
**Problem**: deserialization coerces invalid floats to zero.
**Impact**: cross-language payloads silently change meaning instead of following the documented null-conversion rule.
**Fix**: map NaN/Infinity to `nil`/null-compatible values, not numeric zero.

### MEDIUM — Go registry locking is not guaranteed on a fresh machine
**Location**: `golang/service_registry.go:106`, `golang/service_registry.go:194`, `golang/service_registry.go:177`
**Problem**: lock acquisition happens before ensuring `~/.multifrost/` exists; registration then logs a warning and continues without a lock.
**Impact**: first-run service registration can race and corrupt the shared registry.
**Fix**: create the registry directory before lock acquisition and fail registration when atomic locking cannot be established.

### MEDIUM — Rust leaks timed-out heartbeats
**Location**: `rust/src/parent.rs:912`, `rust/src/parent.rs:934`, `rust/src/parent.rs:938`
**Problem**: heartbeat timeout paths increment miss counters but never remove the corresponding entry from `pending_heartbeats`.
**Impact**: unhealthy workers slowly accumulate stale heartbeat state and unnecessary memory.
**Fix**: remove the heartbeat entry on every timeout/error path before recording the miss.

### MEDIUM — Rust does not implement the documented stdout/stderr forwarding contract
**Location**: `rust/src/parent.rs:288`, `rust/src/child.rs:279`
**Problem**: spawned children have stdout/stderr redirected to `null`, and the child implementation does not forward local output as protocol `stdout`/`stderr` messages.
**Impact**: Rust workers drop diagnostics that other implementations expose to parents, reducing observability and breaking feature parity.
**Fix**: capture child output and forward it over the wire using the same message types as the other runtimes.

### LOW — JavaScript spawn mode omits the documented worker-mode environment hint
**Location**: `javascript/src/multifrost.ts:367`
**Problem**: spawned children receive `COMLINK_ZMQ_PORT` but not `COMLINK_WORKER_MODE=1`.
**Impact**: implementations that rely on the documented spawn hint can diverge from expected startup behavior when invoked by the JS parent.
**Fix**: set the same worker-mode environment contract used by the other runtimes.

### Coverage
- **Analyzed**: Canonical docs in `docs/product.md`, `docs/arch.md`, `docs/msgpack_interop.md`, plus active implementations in `python/src/multifrost/core/`, `javascript/src/`, `golang/`, and `rust/src/`.
- **Not analyzed**: Security posture beyond obvious transport/interop implications, legacy Python code under `python/legacy/`, and example applications in depth. Python tests were not runnable in this environment because `pytest` was unavailable; Rust library review used source inspection plus `cargo test`, which currently fails while compiling examples.
- **Confidence**: Medium
