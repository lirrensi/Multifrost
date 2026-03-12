# Plan: Cross-Language Consistency Fixes
_Fix registry locking, heartbeat RTT, Windows process checks, and restart flags for cross-language interoperability._

---

# Checklist
- [x] Step 1: Standardize registry lock file name to `services.lock`
- [x] Step 2: Add file locking to Rust service registry
- [x] Step 3: Fix Go Windows process alive check
- [x] Step 4: Fix JavaScript heartbeat to use `metadata.hb_timestamp`
- [x] Step 5: Add `--worker` flag to Python restart command
- [x] Step 6: Fix JavaScript multi-parent output tracking
- [x] Step 7: Update `docs/arch.md` documentation
- [x] Step 8: Update `docs/protocol.md` documentation

---

## Context

Fixing issues identified in `agent_chat/Anubis_Findings_2026_02_23.md`:

1. **Rust registry has no file locking** — TOCTOU race condition can corrupt registry
2. **Lock file names differ** — Python uses `registry.lock`, Go uses `services.lock`
3. **Go Windows process check broken** — `os.FindProcess()` always returns non-nil on Windows
4. **JS heartbeat uses `args[0]`** — Others use `metadata.hb_timestamp`, breaking cross-language RTT
5. **Python restart missing `--worker`** — Initial spawn passes it, restart doesn't
6. **JS `_lastSenderId` overwritten** — Multi-parent output only goes to most recent sender

**Relevant files:**
- `rust/src/registry.rs` — Rust service registry (no locking)
- `python/src/multifrost/core/service_registry.py` — Python registry (has locking)
- `golang/service_registry.go` — Go registry (has locking)
- `golang/utils.go` — Go utilities
- `javascript/src/multifrost.ts` — JS parent/child workers
- `python/src/multifrost/core/async_worker.py` — Python async worker
- `docs/arch.md` — Architecture documentation
- `docs/protocol.md` — Wire protocol specification

---

## Prerequisites

- All language implementations are installed and buildable
- Test suites pass before changes
- Git repository is clean or changes are committed

---

## Scope Boundaries

**OUT OF SCOPE:**
- Test files (do not modify tests unless step explicitly says so)
- Example files
- Build configuration (Cargo.toml, package.json, go.mod)
- Documentation beyond `docs/arch.md` and `docs/protocol.md`
- Message schema changes (only implementation changes)

---

## Steps

### Step 1: Standardize registry lock file name to `services.lock`

Change Python's lock file name from `registry.lock` to `services.lock` to match Go.

**File:** `python/src/multifrost/core/service_registry.py`

Find line 29:
```python
LOCK_PATH = Path.home() / ".multifrost" / "registry.lock"
```

Replace with:
```python
LOCK_PATH = Path.home() / ".multifrost" / "services.lock"
```

✅ Success: File saved, `LOCK_PATH` now points to `services.lock`
❌ If failed: Stop and report the exact error message

---

### Step 2: Add file locking to Rust service registry

Add atomic file locking using `O_CREAT | O_EXCL` pattern to Rust registry, matching Python/Go approach.

**File:** `rust/src/registry.rs`

**2a. Add lock path constant and helper functions**

After line 28 (after `registry_path()` function), add:

```rust
    fn lock_path() -> PathBuf {
        Self::get_home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".multifrost")
            .join("services.lock")
    }

    async fn acquire_lock() -> Result<std::fs::File> {
        let lock_path = Self::lock_path();
        Self::ensure_registry_dir()?;
        
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        
        loop {
            // Try atomic file creation (O_CREAT | O_EXCL)
            match std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&lock_path)
            {
                Ok(file) => return Ok(file),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    // Lock held by another process, check if stale
                    if let Ok(metadata) = std::fs::metadata(&lock_path) {
                        if let Ok(modified) = metadata.modified() {
                            if modified.elapsed().unwrap_or_default() > Duration::from_secs(10) {
                                // Stale lock, try to remove
                                let _ = std::fs::remove_file(&lock_path);
                            }
                        }
                    }
                    if tokio::time::Instant::now() >= deadline {
                        return Err(MultifrostError::IoError(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "Lock acquisition timeout",
                        )));
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(e) => return Err(MultifrostError::IoError(e)),
            }
        }
    }

    fn release_lock(_lock_file: std::fs::File) -> Result<()> {
        let lock_path = Self::lock_path();
        // File is closed when dropped, then remove lock file
        drop(_lock_file);
        let _ = std::fs::remove_file(lock_path);
        Ok(())
    }
```

**2b. Wrap `register` function with lock**

Find the `register` function (line 124-150). Wrap the entire body (after `let mut registry = ...`) with lock acquisition:

Replace the current implementation:
```rust
    pub async fn register(service_id: &str) -> Result<u16> {
        let mut registry = Self::read_registry().await?;
        // ... rest of function
    }
```

With:
```rust
    pub async fn register(service_id: &str) -> Result<u16> {
        let lock_file = Self::acquire_lock().await?;
        let result = Self::register_internal(service_id).await;
        let _ = Self::release_lock(lock_file);
        result
    }

    async fn register_internal(service_id: &str) -> Result<u16> {
        let mut registry = Self::read_registry().await?;

        // Check if service already running
        if let Some(existing) = registry.get(service_id) {
            if Self::is_process_alive(existing.pid) {
                return Err(MultifrostError::ServiceAlreadyRunning(
                    service_id.to_string(),
                ));
            }
            // Dead process - will be overwritten
        }

        let port = Self::find_free_port().await?;

        registry.insert(
            service_id.to_string(),
            ServiceRegistration {
                port,
                pid: process::id(),
                started: chrono_lite_timestamp(),
            },
        );

        Self::write_registry(&registry).await?;
        Ok(port)
    }
```

**2c. Wrap `unregister` function with lock**

Find the `unregister` function (line 172-184). Apply same pattern:

Replace with:
```rust
    pub async fn unregister(service_id: &str) -> Result<()> {
        let lock_file = Self::acquire_lock().await?;
        let result = Self::unregister_internal(service_id).await;
        let _ = Self::release_lock(lock_file);
        result
    }

    async fn unregister_internal(service_id: &str) -> Result<()> {
        let mut registry = Self::read_registry().await?;
        let current_pid = process::id();

        if let Some(reg) = registry.get(service_id) {
            if reg.pid == current_pid {
                registry.remove(service_id);
                Self::write_registry(&registry).await?;
            }
        }

        Ok(())
    }
```

**2d. Wrap `discover` read operations with lock**

The `discover` function only reads, but should still acquire lock for consistency. Find the `discover` function (line 152-170) and wrap:

```rust
    pub async fn discover(service_id: &str, timeout_ms: u64) -> Result<u16> {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);

        loop {
            let lock_file = Self::acquire_lock().await?;
            let result = Self::discover_internal(service_id).await;
            let _ = Self::release_lock(lock_file);
            
            match result {
                Ok(port) => return Ok(port),
                Err(MultifrostError::ServiceNotFound(_)) => {
                    if tokio::time::Instant::now() >= deadline {
                        return Err(MultifrostError::ServiceNotFound(service_id.to_string()));
                    }
                    sleep(Duration::from_millis(100)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn discover_internal(service_id: &str) -> Result<u16> {
        let registry = Self::read_registry().await?;

        if let Some(reg) = registry.get(service_id) {
            if Self::is_process_alive(reg.pid) {
                return Ok(reg.port);
            }
        }

        Err(MultifrostError::ServiceNotFound(service_id.to_string()))
    }
```

✅ Success: `cargo build` in `rust/` directory compiles without errors
❌ If failed: Run `cargo build 2>&1` and report the exact error output

---

### Step 3: Fix Go Windows process alive check

Fix `isProcessAlive` in Go to actually check process existence on Windows.

**File:** `golang/service_registry.go`

Find the `isProcessAlive` function (line 306-317):

```go
func isProcessAlive(pid int) bool {
	if pid <= 0 {
		return false
	}
	proc, err := os.FindProcess(pid)
	if err != nil {
		return false
	}
	// On Windows, FindProcess always succeeds, so we need to check differently
	// On Unix, sending signal 0 checks if process exists
	return proc != nil
}
```

Replace with:

```go
func isProcessAlive(pid int) bool {
	if pid <= 0 {
		return false
	}

	if runtime.GOOS == "windows" {
		// On Windows, os.FindProcess always succeeds, so we use tasklist
		cmd := exec.Command("tasklist", "/NH", "/FI", fmt.Sprintf("PID eq %d", pid))
		output, err := cmd.Output()
		if err != nil {
			return false
		}
		// tasklist returns "INFO: No tasks are running..." if PID not found
		return !bytes.Contains(output, []byte("No tasks are running"))
	}

	// On Unix, sending signal 0 checks if process exists without killing it
	proc, err := os.FindProcess(pid)
	if err != nil {
		return false
	}
	err = proc.Signal(syscall.Signal(0))
	return err == nil
}
```

Add required imports at top of file if missing:
```go
import (
	"bytes"
	"os/exec"
	"runtime"
	"syscall"
)
```

✅ Success: `go build ./...` in `golang/` directory compiles without errors
❌ If failed: Run `go build ./... 2>&1` and report the exact error output

---

### Step 4: Fix JavaScript heartbeat to use `metadata.hb_timestamp`

Change JavaScript parent to send heartbeat timestamp in `metadata.hb_timestamp` instead of `args[0]`, matching other languages.

**File:** `javascript/src/multifrost.ts`

**4a. Add `metadata` field to ComlinkMessage class**

Find the `ComlinkMessageData` interface (line 53-65). Add `metadata` field:

```typescript
export interface ComlinkMessageData {
    app: string;
    id: string;
    type: string;
    timestamp: number;
    function?: string;
    args?: unknown[];
    namespace?: string;
    result?: unknown;
    error?: string;
    output?: string;
    clientName?: string;
    metadata?: Record<string, unknown>;
}
```

Find the `ComlinkMessage` class (line 67-78). Add `metadata` property:

```typescript
export class ComlinkMessage {
    public app: string = APP_NAME;
    public id: string;
    public type: string;
    public timestamp: number;
    public function?: string;
    public args?: unknown[];
    public namespace?: string;
    public result?: unknown;
    public error?: string;
    public output?: string;
    public clientName?: string;
    public metadata?: Record<string, unknown>;
```

Find the `toDict()` method (line 128-145). Add metadata to output:

```typescript
    toDict(): ComlinkMessageData {
        const result: ComlinkMessageData = {
            app: this.app,
            id: this.id,
            type: this.type,
            timestamp: this.timestamp,
        };

        if (this.function !== undefined) result.function = this.function;
        if (this.args !== undefined) result.args = this.args;
        if (this.namespace !== undefined) result.namespace = this.namespace;
        if (this.result !== undefined) result.result = this.result;
        if (this.error !== undefined) result.error = this.error;
        if (this.output !== undefined) result.output = this.output;
        if (this.clientName !== undefined) result.clientName = this.clientName;
        if (this.metadata !== undefined) result.metadata = this.metadata;

        return result;
    }
```

**4b. Fix parent heartbeat sending to use metadata**

Find the heartbeat creation in `_heartbeatLoop` (line 591-597):

```typescript
const heartbeatId = randomUUID();
const heartbeat = new ComlinkMessage({
    type: MessageType.HEARTBEAT,
    id: heartbeatId,
    args: [Date.now()], // Send timestamp for RTT calculation
});
```

Replace with:

```typescript
const heartbeatId = randomUUID();
const heartbeat = new ComlinkMessage({
    type: MessageType.HEARTBEAT,
    id: heartbeatId,
    metadata: { hb_timestamp: Date.now() / 1000 }, // Unix seconds, matching other languages
});
```

**4c. Fix parent heartbeat response handling to use metadata**

Find the heartbeat response handling in `handleMessage` (line 431-446):

```typescript
} else if (message.type === MessageType.HEARTBEAT) {
    // Handle heartbeat response
    const heartbeat = this._pendingHeartbeats.get(message.id);
    if (heartbeat) {
        this._pendingHeartbeats.delete(message.id);

        // Calculate RTT from timestamp in args if present
        if (message.args && message.args.length > 0) {
            const sentTime = message.args[0] as number;
            this._lastHeartbeatRttMs = Date.now() - sentTime * 1000;
        }

        // Reset consecutive misses on successful response
        this._consecutiveHeartbeatMisses = 0;
        heartbeat.resolve(true);
    }
}
```

Replace with:

```typescript
} else if (message.type === MessageType.HEARTBEAT) {
    // Handle heartbeat response
    const heartbeat = this._pendingHeartbeats.get(message.id);
    if (heartbeat) {
        this._pendingHeartbeats.delete(message.id);

        // Calculate RTT from timestamp in metadata (cross-language compatible)
        if (message.metadata && typeof message.metadata.hb_timestamp === "number") {
            const sentTime = message.metadata.hb_timestamp;
            this._lastHeartbeatRttMs = (Date.now() / 1000 - sentTime) * 1000;
        }

        // Reset consecutive misses on successful response
        this._consecutiveHeartbeatMisses = 0;
        heartbeat.resolve(true);
    }
}
```

**4d. Fix child heartbeat response to use metadata**

Find `handleHeartbeat` in ChildWorker (line 1044-1059):

```typescript
private async handleHeartbeat(message: ComlinkMessage, senderId: Buffer): Promise<void> {
    // Echo back the heartbeat with the same ID and args (for RTT calculation)
    const response = new ComlinkMessage({
        type: MessageType.HEARTBEAT,
        id: message.id,
        args: message.args, // Preserve timestamp for RTT calculation
    });
```

Replace with:

```typescript
private async handleHeartbeat(message: ComlinkMessage, senderId: Buffer): Promise<void> {
    // Echo back the heartbeat with the same ID and metadata (for RTT calculation)
    const response = new ComlinkMessage({
        type: MessageType.HEARTBEAT,
        id: message.id,
        metadata: message.metadata, // Preserve timestamp for RTT calculation
    });
```

✅ Success: `npm run build` in `javascript/` directory compiles without errors
❌ If failed: Run `npm run build 2>&1` and report the exact error output

---

### Step 5: Add `--worker` flag to Python restart command

Fix the restart function to pass `--worker` flag, matching initial spawn.

**File:** `python/src/multifrost/core/async_worker.py`

Find the `_attempt_restart` method (line 860-864):

```python
self.process = subprocess.Popen(
    [self._executable, self._script_path],
    env=env,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)
```

Replace with:

```python
# Detect if this is a compiled binary (same logic as _start_child_process)
is_compiled_binary = self._executable == self._script_path or (
    self._script_path and self._script_path.lower().endswith((".exe", ".dll"))
)

if is_compiled_binary:
    cmd = [self._script_path]
else:
    cmd = [self._executable, self._script_path, "--worker"]

self.process = subprocess.Popen(
    cmd,
    env=env,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)
```

✅ Success: File saved, restart now passes `--worker` flag for scripts
❌ If failed: Stop and report the exact error message

---

### Step 6: Fix JavaScript multi-parent output tracking

Change `_lastSenderId` to track all connected parent IDs, and broadcast output to all.

**File:** `javascript/src/multifrost.ts`

**6a. Change from single sender to Set of senders**

Find the `_lastSenderId` field in `ChildWorker` class (line 857):

```typescript
private _lastSenderId?: Buffer;
```

Replace with:

```typescript
private _connectedParentIds: Set<Buffer> = new Set();
```

**6b. Update message loop to track all parents**

Find the message loop where `_lastSenderId` is set (line 970-971):

```typescript
// Track sender for output forwarding
this._lastSenderId = senderId as Buffer;
```

Replace with:

```typescript
// Track all connected parents for output forwarding
this._connectedParentIds.add(senderId as Buffer);
```

**6c. Update `_sendOutput` to broadcast to all parents**

Find the `_sendOutput` method (line 924-951):

```typescript
private async _sendOutput(output: string, msgType: MessageType, retries: number = 2): Promise<void> {
    if (!this.socket) {
        console.warn("Cannot send output: socket not initialized");
        return;
    }
    
    if (!this._lastSenderId) {
        // Buffer is undefined or empty - parent not connected yet
        return;
    }

    const message = ComlinkMessage.createOutput(output, msgType);

    for (let attempt = 0; attempt < retries; attempt++) {
        try {
            await this.socket.send([this._lastSenderId, Buffer.alloc(0), message.pack()]);
            return;
        } catch (error) {
            const errorMessage = error instanceof Error ? error.message : String(error);
            if (attempt < retries - 1) {
                await new Promise(resolve => setTimeout(resolve, 1));
                continue;
            }
            console.warn(`Warning: Failed to send output: ${errorMessage}`);
        }
    }
}
```

Replace with:

```typescript
private async _sendOutput(output: string, msgType: MessageType, retries: number = 2): Promise<void> {
    if (!this.socket) {
        console.warn("Cannot send output: socket not initialized");
        return;
    }
    
    if (this._connectedParentIds.size === 0) {
        // No parents connected yet
        return;
    }

    const message = ComlinkMessage.createOutput(output, msgType);

    // Broadcast to all connected parents
    for (const parentId of this._connectedParentIds) {
        for (let attempt = 0; attempt < retries; attempt++) {
            try {
                await this.socket.send([parentId, Buffer.alloc(0), message.pack()]);
                break; // Success, move to next parent
            } catch (error) {
                const errorMessage = error instanceof Error ? error.message : String(error);
                if (attempt < retries - 1) {
                    await new Promise(resolve => setTimeout(resolve, 1));
                    continue;
                }
                console.warn(`Warning: Failed to send output to parent: ${errorMessage}`);
            }
        }
    }
}
```

✅ Success: `npm run build` in `javascript/` directory compiles without errors
❌ If failed: Run `npm run build 2>&1` and report the exact error output

---

### Step 7: Update `docs/arch.md` documentation

Update architecture documentation to reflect lock file standardization and heartbeat RTT mechanism.

**File:** `docs/arch.md`

**7a. Update HEARTBEAT section**

Find the HEARTBEAT section (line 119-129):

```markdown
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
```

Replace with:

```markdown
#### HEARTBEAT / SHUTDOWN
```json
{
  "app": "comlink_ipc_v4",
  "id": "<uuid>",
  "type": "heartbeat",
  "timestamp": 1234.5,
  "metadata": {
    "hb_timestamp": 1234.5
  }
}
```

- **HEARTBEAT**: Used for connection health monitoring. Parent sends with `hb_timestamp` in metadata, child echoes back the same message. Parent calculates RTT from `(current_time - hb_timestamp) * 1000` milliseconds.
- **SHUTDOWN**: Child should stop processing when received
```

**7b. Update Service Registry section for lock file standardization**

Find the Service Registry section (line 159-179). Look for:

```markdown
 Rules:
- Registration fails if service_id exists with live PID
- Dead PIDs are overwritten
- Unregister only removes if PID matches
- **Atomic file locking**: Uses `O_CREAT | O_EXCL` for lock acquisition to prevent race conditions
- **Discovery polling**: 100ms interval, 5s default timeout
- **Lock timeout**: 10s max wait for registry lock
```

Replace with:

```markdown
 Rules:
- Registration fails if service_id exists with live PID
- Dead PIDs are overwritten
- Unregister only removes if PID matches
- **Lock file**: `~/.multifrost/services.lock` (consistent across all languages)
- **Atomic file locking**: Uses `O_CREAT | O_EXCL` for lock acquisition to prevent race conditions
- **Stale lock handling**: Locks older than 10 seconds are considered stale and removed
- **Discovery polling**: 100ms interval, 5s default timeout
- **Lock timeout**: 10s max wait for registry lock
```

✅ Success: `docs/arch.md` updated with heartbeat RTT mechanism and lock file standardization
❌ If failed: Stop and report the exact error message

---

### Step 8: Update `docs/protocol.md` documentation

Update protocol specification to document heartbeat mechanism and standardized registry locking.

**File:** `docs/protocol.md`

**8a. Update HEARTBEAT section**

Find line 93-94:

```
6.6 HEARTBEAT
6.6.1 Reserved for future; no behavioral contract in current canon.
```

Replace with:

```
6.6 HEARTBEAT
6.6.1 Parent sends HEARTBEAT with `metadata.hb_timestamp` (Unix seconds, float).
6.6.2 Child echoes the HEARTBEAT back unchanged.
6.6.3 Parent calculates RTT: `(current_timestamp - hb_timestamp) * 1000` milliseconds.
6.6.4 Used for connection health monitoring in spawn mode.
6.6.5 Cross-language compatible: all implementations use `metadata.hb_timestamp`.
```

**8b. Update registry lock file path**

Find line 163:

```
11.2 Lock path: ~/.multifrost/registry.lock (Python).
```

Replace with:

```
11.2 Lock path: ~/.multifrost/services.lock (consistent across all languages).
```

**8c. Update registry locking section**

Find lines 168-171:

```
11.8 Python uses file locking (fcntl/msvcrt).
11.9 JS uses proper-lockfile for locking.
11.10 JS PID check uses pidusage; Python uses psutil if installed else os.kill(0).
```

Replace with:

```
11.8 All implementations use atomic file locking via `O_CREAT | O_EXCL` pattern.
11.9 Stale locks (older than 10 seconds) are automatically removed.
11.10 Lock timeout is 10 seconds maximum.
11.11 Windows uses msvcrt.locking (Python) or tasklist (Go); Unix uses fcntl (Python) or signal(0) (Go).
11.12 Rust uses std::fs::OpenOptions::create_new for atomic lock acquisition.
11.13 PID check: Python uses psutil or os.kill(0); Go uses signal(0) on Unix, tasklist on Windows; Rust uses kill -0 on Unix, tasklist on Windows.
```

**8d. Add metadata field to message schema**

Find line 111-117 (after RESPONSE fields):

```
7.3 RESPONSE fields:
7.3.1 result: any (optional in practice but SHOULD be present).
7.4 ERROR fields:
7.4.1 error: string (MUST be present).
7.5 OUTPUT fields:
7.5.1 output: string (MUST be present).
```

Replace with:

```
7.3 RESPONSE fields:
7.3.1 result: any (optional in practice but SHOULD be present).
7.4 ERROR fields:
7.4.1 error: string (MUST be present).
7.5 OUTPUT fields:
7.5.1 output: string (MUST be present).
7.6 HEARTBEAT fields:
7.6.1 metadata: map (MUST contain hb_timestamp for RTT calculation).
7.6.2 hb_timestamp: number (Unix seconds, float).
```

**8e. Add metadata to field constraints**

Find line 282-283 (after output field constraint):

```
22.11 Field=output; Type=string; Constraint=MUST be present for STDOUT/STDERR
```

Add after it:

```
22.12 Field=metadata; Type=map; Constraint=OPTIONAL; used for hb_timestamp in HEARTBEAT
22.13 Field=hb_timestamp; Type=number; Constraint=MUST be present in HEARTBEAT metadata
```

**8f. Update suggested extensions (now implemented)**

Find line 416:

```
38.1 Implement heartbeat to detect stale connections.
```

Replace with:

```
38.1 Heartbeat is implemented: Parent sends periodic HEARTBEAT, child echoes, parent calculates RTT.
```

✅ Success: `docs/protocol.md` updated with heartbeat mechanism and registry standardization
❌ If failed: Stop and report the exact error message

---

## Verification

Run all test suites to verify nothing is broken:

```bash
# Python tests
cd python && python -m pytest tests/ -v --tb=short

# JavaScript tests  
cd javascript && npm test

# Go tests
cd golang && go test ./... -v

# Rust tests
cd rust && cargo test -- --nocapture
```

Expected: All tests pass. If any test fails, report the full failure output.

---

## Rollback

If critical failure occurs, revert all changes:

```bash
git checkout -- python/src/multifrost/core/service_registry.py
git checkout -- python/src/multifrost/core/async_worker.py
git checkout -- rust/src/registry.rs
git checkout -- golang/service_registry.go
git checkout -- javascript/src/multifrost.ts
git checkout -- docs/arch.md
git checkout -- docs/protocol.md
```
