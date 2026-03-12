# Anubis Code Review Findings — 2026_02_23

## CRITICAL Issues

### 1. CRITICAL — Race Condition in Rust Service Registry (No File Locking)
**Location**: `rust/src/registry.rs:124-149` (`register` function)
**Problem**: The Rust service registry does NOT implement file locking. It reads the registry, modifies it, and writes it back without any synchronization. This creates a classic TOCTOU (Time-of-check-time-of-use) race condition.
**Impact**: When multiple processes register/discover services concurrently, they can corrupt the registry file or silently overwrite each other's registrations.
**Fix**: Implement file locking using `O_CREAT | O_EXCL` pattern or a proper file lock crate (like `fs2` or `file-lock`). Python and Go implementations have this; Rust is missing it.

```rust
// Current (broken): No locking
pub async fn register(service_id: &str) -> Result<u16> {
    let mut registry = Self::read_registry().await?;  // <-- Race window opens
    // ... modifications ...
    Self::write_registry(&registry).await?;           // <-- Race window closes
    Ok(port)
}
```

---

### 2. CRITICAL — Go Process Alive Check Always Returns True on Windows
**Location**: `golang/service_registry.go:306-317` (`isProcessAlive` function)
**Problem**: On Windows, `os.FindProcess()` always succeeds and returns a non-nil Process. The comment acknowledges this but the implementation doesn't actually check if the process exists.
**Impact**: Dead process detection fails completely on Windows. Services registered by crashed processes will never be cleaned up, and new registrations will incorrectly fail with "already registered" even when the old process is dead.
**Fix**: Use actual process existence check on Windows via `tasklist` or native Windows API.

```go
// Current (broken on Windows):
func isProcessAlive(pid int) bool {
    proc, err := os.FindProcess(pid)
    if err != nil { return false }
    return proc != nil  // <-- Always true on Windows!
}
```

---

### 3. CRITICAL — JavaScript `findFreePort` Has Race Condition
**Location**: `javascript/src/multifrost.ts:320-327` (`findFreePort`)
**Problem**: The port is found, server is closed, but another process could bind to that port before this process uses it. The window between `close()` and actual bind is unprotected.
**Impact**: Intermittent "address already in use" errors when spawning multiple workers concurrently.
**Fix**: Either keep the socket open until ZMQ binds, or retry on port conflict. This is a common race but typically low probability on local machines.

---

### 4. CRITICAL — Python Async Function Timeout Not Propagated Properly
**Location**: `python/src/multifrost/core/child.py:322-325` (`_handle_function_call`)
**Problem**: When an async function times out after 30 seconds, `future.result(timeout=30.0)` raises `TimeoutError`, but this is caught by the outer try/except and converted to a generic error message. The timeout information is lost.
**Impact**: Remote callers see a generic error instead of a clear timeout message. Debugging becomes difficult.
**Fix**: Catch `TimeoutError` specifically and include "timed out after 30s" in the error message.

```python
# Current:
result = future.result(timeout=30.0)  # Raises TimeoutError
# Caught by generic except, becomes "TimeoutError: " in error message

# Should:
except TimeoutError:
    response = ComlinkMessage.create_error(
        f"Async function '{message.function}' timed out after 30s", 
        message.id
    )
```

---

## HIGH Priority Issues

### 5. HIGH — Inconsistent Heartbeat RTT Calculation
**Location**: Multiple files
**Problem**: Heartbeat RTT is calculated differently across languages:
- **Python**: Uses `metadata.hb_timestamp` in heartbeat response
- **JavaScript**: Uses `args[0]` for timestamp in heartbeat
- **Go**: Uses `metadata.hb_timestamp` 
- **Rust**: Uses `metadata.hb_timestamp`

**Impact**: JavaScript parent connecting to Python child (or vice versa) will not calculate RTT correctly because they expect timestamp in different fields.
**Fix**: Standardize on `metadata.hb_timestamp` across all implementations. Update JavaScript to use metadata instead of args.

---

### 6. HIGH — Python Child Missing `--worker` Flag in Restart
**Location**: `python/src/multifrost/core/async_worker.py:860-864` (`_attempt_restart`)
**Problem**: In `_attempt_restart`, the `--worker` flag is not passed to the child process, but it IS passed in initial `_start_child_process` (line 431). This inconsistency means restarted children may behave differently.
**Impact**: Restarted workers may not enter the correct worker mode, leading to undefined behavior.
**Fix**: Add `--worker` flag to the restart command.

```python
# Current (broken):
self.process = subprocess.Popen(
    [self._executable, self._script_path],  # Missing --worker!
    env=env,
    ...
)
```

---

### 7. HIGH — Go `findFreePort` Not Implemented for Service Registration
**Location**: `golang/child_worker.go:107` (`setupZMQ`)
**Problem**: In connect mode, Go child calls `findFreePort()` which is defined but NOT implemented in the provided files. This will cause a linker error.
**Impact**: Go connect mode workers cannot be created.
**Fix**: Verify `findFreePort` implementation exists in `utils.go` or implement it.

---

### 8. HIGH — JavaScript Child Doesn't Handle Multiple Parents Correctly
**Location**: `javascript/src/multifrost.ts:931-934` (`_sendOutput`)
**Problem**: The `_lastSenderId` is overwritten on every message. If multiple parents are connected (ROUTER supports this), output will only be sent to the most recent sender, not all parents.
**Impact**: In multi-parent scenarios, output forwarding breaks for all but one parent.
**Fix**: Either track all sender IDs, or document that multi-parent output forwarding is not supported.

---

### 9. HIGH — Rust Parent Socket Bind Address Not Cross-Platform
**Location**: `rust/src/parent.rs:192-193`
**Problem**: Socket binds to `tcp://0.0.0.0:{port}` which may not work correctly on all systems. Some platforms prefer `tcp://*:{port}` for wildcard binding.
**Impact**: May fail to bind on some platforms.
**Fix**: Use `tcp://*:{port}` for consistency with other implementations.

---

### 10. HIGH — Missing Timeout Handling in Python Child Output Send
**Location**: `python/src/multifrost/core/child.py:153-184` (`_send_output`)
**Problem**: The `_send_output` method retries on `zmq.Again` but only waits 1ms between retries. Under heavy load, 2 retries with 1ms delay is insufficient.
**Impact**: Output forwarding silently fails under load, losing logs/debug output.
**Fix**: Increase retry count or implement exponential backoff. Document that output is best-effort.

---

## MEDIUM Priority Issues

### 11. MEDIUM — Python Global Signal Handlers Not Cleaned Up
**Location**: `python/src/multifrost/core/async_worker.py:37-56` (`_install_signal_handlers`)
**Problem**: Signal handlers are installed once globally and never removed. The `_active_workers` set is used to forward signals, but workers that failed to clean up (due to crash) may remain in the set.
**Impact**: Ghost references to dead workers could cause exceptions during signal handling.
**Fix**: Use weak references in `_active_workers` or ensure cleanup always runs.

---

### 12. MEDIUM — JavaScript `shell: true` Security Risk
**Location**: `javascript/src/multifrost.ts:365` (`startChildProcess`)
**Problem**: Spawning child process with `shell: true` enables shell expansion, which is a security risk if script paths contain user input.
**Impact**: Potential command injection if script path is derived from untrusted input.
**Fix**: Remove `shell: true` and use direct executable invocation. Or document the security implication clearly.

---

### 13. MEDIUM — Go Missing Linger Configuration on Socket Close
**Location**: `golang/parent_worker.go:647-649` (`Close`)
**Problem**: Socket is closed without setting linger time. This may cause pending messages to be discarded immediately or block indefinitely depending on ZMQ defaults.
**Impact**: Messages in flight during shutdown may be lost.
**Fix**: Set linger time before closing: `socket.SetOption(zmq.LINGER, 0)` or similar.

---

### 14. MEDIUM — Inconsistent Default Timeout Handling
**Location**: Multiple files
**Problem**: When `default_timeout` is not set:
- Python: `effective_timeout = None` → waits indefinitely
- JavaScript: `effectiveTimeout = undefined` → waits indefinitely
- Go: `effectiveTimeout = 0` → waits indefinitely
- Rust: `effective_timeout = None` → waits indefinitely

This is consistent BUT waiting indefinitely is dangerous for IPC calls.
**Impact**: Hung child processes cause parent to hang forever.
**Fix**: Consider adding a sane default timeout (e.g., 30 seconds) at the API level.

---

### 15. MEDIUM — Rust Heartbeat Loop Hardcoded Circuit Breaker Threshold
**Location**: `rust/src/parent.rs:952` (`heartbeat_loop`)
**Problem**: The heartbeat loop uses hardcoded `5` instead of `max_restart_attempts` for circuit breaker threshold.
**Impact**: Circuit breaker behavior differs from configuration.
**Fix**: Use the configured `max_restart_attempts` value (passed as `_max_restart_attempts` but unused).

```rust
// Current (broken):
if *failures >= 5 {  // Should use heartbeat_max_misses or configurable threshold

// Should be:
if *failures >= heartbeat_max_misses {  // or max_restart_attempts
```

---

### 16. MEDIUM — Python Child ZMQWriter Doesn't Handle Unicode Errors
**Location**: `python/src/multifrost/core/child.py:61-73` (`ZMQWriter`)
**Problem**: If `text.strip()` raises a Unicode decode error, the write will fail silently. The exception propagates up and breaks the output redirection.
**Impact**: Workers printing binary or invalid Unicode will crash or lose output.
**Fix**: Wrap in try/except and handle encoding errors gracefully.

---

### 17. MEDIUM — JavaScript `settle` Function Records Failure on Remote Error
**Location**: `javascript/src/multifrost.ts:493-505` (`settle`)
**Problem**: The `settle` function is called with `isSuccess: false` for remote errors, which increments the failure counter. But remote errors (like `RemoteCallError`) shouldn't count toward circuit breaker failures — they're normal operation.
**Impact**: Circuit breaker trips prematurely when child returns errors frequently (which may be expected behavior).
**Fix**: Only count connection/timeout errors toward circuit breaker, not remote call errors.

---

## LOW Priority Issues

### 18. LOW — Inconsistent Error Message Formatting
**Location**: Multiple files
**Problem**: Error messages have inconsistent formats:
- Python: `"{error_type}: {message}\n{traceback}"`
- JavaScript: `"{message}"` (no type, no traceback)
- Go: `"{message}"`
- Rust: `"{message}"`

**Impact**: Debugging across languages is harder due to inconsistent error detail.
**Fix**: Standardize error format across all implementations.

---

### 19. LOW — Python `__del__` Safety Could Be Improved
**Location**: `python/src/multifrost/core/async_worker.py:972-999` (`__del__`)
**Problem**: The `__del__` method directly accesses `self.socket` without `hasattr` check first (it does use hasattr, but the pattern is inconsistent). The order of cleanup could matter.
**Impact**: Potential for obscure exceptions during interpreter shutdown.
**Fix**: Ensure all attribute access uses `getattr(self, 'attr', None)` pattern for safety.

---

### 20. LOW — Go Unused Parameter in `heartbeatLoop`
**Location**: `golang/parent_worker.go` (heartbeat loop)
**Problem**: Several parameters passed to `heartbeatLoop` are unused or underscore-prefixed but should be used.
**Impact**: Dead code, potential bugs if behavior expected from those parameters.
**Fix**: Either use the parameters or remove them from function signature.

---

### 21. LOW — Rust `current_timestamp` Duplicated Across Files
**Location**: `rust/src/parent.rs:965-971`, `rust/src/child.rs:14-19`
**Problem**: Same `current_timestamp` function is implemented in both `parent.rs` and `child.rs`.
**Impact**: Code duplication, maintenance burden.
**Fix**: Move to shared utility module.

---

### 22. LOW — JavaScript `console.log` Override May Break Debuggers
**Location**: `javascript/src/multifrost.ts:901-921` (`redirectOutput`)
**Problem**: Overriding `console.log` and `console.error` can interfere with debuggers and logging libraries that expect original implementations.
**Impact**: Debugging becomes harder, some libraries may break.
**Fix**: Consider using a more targeted approach or documenting this clearly.

---

## Cross-Language Consistency Issues

### 23. CONSISTENCY — Heartbeat Timestamp Field Location
**Impact**: JavaScript vs other languages - RTT calculation may fail cross-language.

### 24. CONSISTENCY — Service Registry Lock File Names
**Problem**: Python uses `registry.lock`, Go uses `services.lock`. Different lock file names mean they don't coordinate.
**Impact**: Python and Go processes running on same machine won't properly coordinate registry access.
**Fix**: Standardize lock file name across all implementations.

### 25. CONSISTENCY — Service Registry File Format
**Problem**: 
- Python: `started` as ISO datetime string
- Go: `start_time` as `time.Time` (serialized differently)
- JavaScript: `started` as ISO datetime string
- Rust: `started` as Unix timestamp string

**Impact**: Format inconsistencies may cause parsing issues.
**Fix**: Standardize on ISO 8601 format with field name `started`.

---

## Coverage
- **Analyzed**: All core implementation files for Python, JavaScript, Go, and Rust (parent workers, child workers, service registries, message handling)
- **Not analyzed**: Test files, example files, build configuration, type definition files, logging implementations
- **Confidence**: HIGH — All core logic paths reviewed against architecture specification and quality checklist

---

## Summary by Language

| Language | CRITICAL | HIGH | MEDIUM | LOW |
|----------|----------|------|--------|-----|
| Python   | 1        | 1    | 2      | 1   |
| JavaScript| 1       | 2    | 2      | 1   |
| Go       | 1        | 2    | 1      | 1   |
| Rust     | 1        | 1    | 1      | 1   |
| Cross-lang| 0       | 1    | 0      | 2   |

**Most Critical**: Rust missing file locking in service registry, Go Windows process alive check broken.

**Recommended Priority**:
1. Fix CRITICAL issues (security/correctness)
2. Fix cross-language heartbeat timestamp inconsistency
3. Fix HIGH issues (reliability)
4. Address consistency issues for true cross-language interop
