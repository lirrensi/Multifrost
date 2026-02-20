# Multifrost Go Implementation Architecture

This document details how the Go implementation of Multifrost navigates the language-agnostic specification defined in `docs/arch.md`. It covers Go-specific design decisions, concurrency patterns, and implementation details.

## Overview

The Go implementation provides full parity with Python Multifrost v4, including all reliability features: circuit breakers, heartbeat monitoring, metrics collection, and structured logging. It uses Go's idiomatic patterns while maintaining wire protocol compatibility.

```
┌─────────────────────────────────────────────────────────────┐
│                    Go Implementation                         │
├─────────────────────────────────────────────────────────────┤
│  ParentWorker (DEALER)          ChildWorker (ROUTER)        │
│  ├─ Spawn mode                  ├─ Method dispatch          │
│  ├─ Connect mode                ├─ Reflection-based calls    │
│  ├─ Circuit breaker             ├─ Output forwarding         │
│  ├─ Heartbeat monitoring        ├─ Heartbeat response        │
│  ├─ Metrics collection          └─ Signal handling           │
│  └─ Retry logic                                              │
└─────────────────────────────────────────────────────────────┘
                            │
                    ZeroMQ over TCP
                      msgpack v5
```

## Core Design Principles

### 1. Goroutine-Based Concurrency

Unlike Python's asyncio, Go uses goroutines and channels for concurrency:

```go
// ParentWorker runs multiple goroutines:
// 1. messageLoop() - receives ZMQ messages
// 2. heartbeatLoop() - sends periodic heartbeats (spawn mode only)
// 3. CallFunction() - each call is synchronous but non-blocking via channels

// ChildWorker runs:
// 1. messageLoop() - handles incoming requests
// 2. forwardOutput() - goroutines for stdout/stderr pipes
```

**Key differences from Python:**
- No event loop needed - goroutines are lightweight threads managed by Go runtime
- Synchronous code is idiomatic; async/await is not used
- Channels provide type-safe communication between goroutines
- `sync.RWMutex` protects shared state (pending requests, metrics)

### 2. Type Safety

Go's static type system influences the implementation:

```go
// Strongly typed message structure
type ComlinkMessage struct {
    App        string         `msgpack:"app"`
    ID         string         `msgpack:"id"`
    Type       string         `msgpack:"type"`
    Timestamp  float64        `msgpack:"timestamp"`
    Function   string         `msgpack:"function,omitempty"`
    Args       []any          `msgpack:"args,omitempty"`
    // ...
}

// Type-safe error handling
type CircuitOpenError struct {
    ConsecutiveFailures int
}

func (e *CircuitOpenError) Error() string {
    return fmt.Sprintf("circuit breaker open after %d consecutive failures", e.ConsecutiveFailures)
}
```

### 3. Reflection for Method Dispatch

Go uses reflection to call methods dynamically:

```go
func (w *ChildWorker) handleFunctionCall(msg *ComlinkMessage, senderID []byte) {
    // Find method by name using reflection
    method := reflect.ValueOf(w).MethodByName(msg.Function)
    if !method.IsValid() {
        // Send error response
        return
    }

    // Convert args to reflect.Values
    args := make([]reflect.Value, len(msg.Args))
    for i, arg := range msg.Args {
        args[i] = reflect.ValueOf(arg)
    }

    // Call the method
    results := method.Call(args)
    // Handle results...
}
```

**Limitations:**
- Methods must be exported (capitalized) to be callable
- No support for async/await patterns (Go doesn't have async/await)
- Method signatures are checked at runtime, not compile time

### Automatic Type Conversion for Cross-Language IPC

The ChildWorker includes automatic type conversion to handle msgpack differences between languages:

```go
// convertArg handles type conversion from msgpack-decoded values
func convertArg(value reflect.Value, target reflect.Type) reflect.Value {
    // Unwrap interface{} from msgpack decoding
    value = unwrapInterface(value)
    
    // Handle int64 -> int conversion (Python sends int64)
    if isIntegerKind(srcKind) && isIntegerKind(target.Kind()) {
        return reflect.ValueOf(value.Int()).Convert(target)
    }
    
    // Handle slice/array conversion with element unwrapping
    // Handle map conversion with key/value unwrapping
    // ...
}

// unwrapInterface handles the msgpack "boxing" issue
func unwrapInterface(value reflect.Value) reflect.Value {
    if value.Kind() == reflect.Interface && value.Elem().IsValid() {
        return value.Elem()
    }
    return value
}
```

**Why this is needed:**
- Python sends integers as int64 via msgpack
- msgpack decodes arrays as `[]interface{}` with each element being the actual type wrapped
- Go methods expecting `[]int` would panic without unwrapping
- The conversion helper unwraps interface{} → extracts underlying value → converts to target type

This allows Go methods with typed parameters to work correctly when called from Python, JavaScript, or Rust:

## Wire Protocol Implementation

### ZMQ Socket Handling

Go uses `github.com/go-zeromq/zmq4` (pure Go implementation):

```go
// ParentWorker (DEALER socket)
func (pw *ParentWorker) Start() error {
    pw.socket = zmq.NewDealer(context.Background())
    
    if pw.isSpawnMode {
        // Bind in spawn mode
        endpoint := fmt.Sprintf("tcp://*:%d", pw.port)
        if err := pw.socket.Listen(endpoint); err != nil {
            return fmt.Errorf("failed to bind to %s: %w", endpoint, err)
        }
    } else {
        // Connect in connect mode
        endpoint := fmt.Sprintf("tcp://localhost:%d", pw.port)
        if err := pw.socket.Dial(endpoint); err != nil {
            return fmt.Errorf("failed to connect to %s: %w", endpoint, err)
        }
    }
    
    go pw.messageLoop()
    return nil
}

// ChildWorker (ROUTER socket)
func (w *ChildWorker) setupZMQ() error {
    w.socket = zmq.NewRouter(context.Background())
    
    if portEnv := os.Getenv("COMLINK_ZMQ_PORT"); portEnv != "" {
        // Spawn mode: connect to parent
        endpoint := fmt.Sprintf("tcp://localhost:%d", w.port)
        w.socket.Dial(endpoint)
    } else {
        // Connect mode: bind and register
        endpoint := fmt.Sprintf("tcp://*:%d", w.port)
        w.socket.Listen(endpoint)
        Register(w.ServiceID, w.port)
    }
}
```

### Multipart Message Framing

Go implementation follows the spec exactly:

```go
// Parent sends: [empty_frame, message_bytes]
data, _ := msg.Pack()
zmqMsg := zmq.NewMsgFrom([]byte{}, data)
socket.Send(zmqMsg)

// Child receives: [sender_id, empty_frame, message_bytes]
msg, _ := socket.Recv()
frames := msg.Frames
senderID := frames[0]        // Track for responses
messageData := frames[2]     // Actual message

// Child responds: [sender_id, empty_frame, message_bytes]
response := CreateResponse(result, msgID)
data, _ := response.Pack()
zmqMsg := zmq.NewMsgFrom(senderID, []byte{}, data)
socket.Send(zmqMsg)

// Parent receives: [empty_frame, message_bytes]
msg, _ := socket.Recv()
frames := msg.Frames
messageData := frames[1]     // Actual message
```

### msgpack Serialization

Uses `github.com/vmihailenco/msgpack/v5`:

```go
// Pack message to msgpack
func (m *ComlinkMessage) Pack() ([]byte, error) {
    return msgpack.Marshal(m)
}

// Unpack message from msgpack
func Unpack(data []byte) (*ComlinkMessage, error) {
    var msg ComlinkMessage
    err := msgpack.Unmarshal(data, &msg)
    if err != nil {
        return nil, err
    }
    return &msg, nil
}
```

**Go-specific considerations:**
- `any` type (Go 1.18+) used for args and results
- Struct tags map Go fields to msgpack keys
- Empty fields are omitted with `omitempty`

## Lifecycle Modes

### Spawn Mode

```go
// Parent creates worker
worker := Spawn("./math_worker.go", "go", "run")

// Spawn function creates ParentWorker with spawn configuration
func Spawn(scriptPath string, executable ...string) *ParentWorker {
    exe := "go"
    if len(executable) > 0 && executable[0] != "" {
        exe = executable[0]
    }
    
    port := findFreePort()
    return NewParentWorker(ParentWorkerConfig{
        ScriptPath:    scriptPath,
        Executable:    exe,
        Port:          port,
        EnableMetrics: true,
    })
}

// Parent starts child process
func (pw *ParentWorker) startChildProcess() error {
    cmd := exec.Command(pw.executable, pw.scriptPath)
    cmd.Env = append(os.Environ(),
        fmt.Sprintf("COMLINK_ZMQ_PORT=%d", pw.port),
        "COMLINK_WORKER_MODE=1",
    )
    
    cmd.Stdout = os.Stdout
    cmd.Stderr = os.Stderr
    
    if err := cmd.Start(); err != nil {
        return fmt.Errorf("failed to start process: %w", err)
    }
    
    pw.process = cmd
    return nil
}
```

### Connect Mode

```go
// Child registers service
worker := NewChildWorkerWithService("math-service")
worker.Start()  // Registers service and binds socket

// Parent discovers and connects
worker, err := Connect(context.Background(), "math-service")
if err != nil {
    log.Fatal(err)
}
worker.Start()

// Connect function discovers port and creates ParentWorker
func Connect(ctx context.Context, serviceID string, timeout ...time.Duration) (*ParentWorker, error) {
    t := DiscoveryTimeout
    if len(timeout) > 0 {
        t = timeout[0]
    }
    
    port, err := Discover(serviceID, t)
    if err != nil {
        return nil, fmt.Errorf("failed to discover service '%s': %w", serviceID, err)
    }
    
    return NewParentWorker(ParentWorkerConfig{
        ServiceID:     serviceID,
        Port:          port,
        EnableMetrics: true,
    }), nil
}
```

## Reliability Features

### Circuit Breaker

Go implementation uses atomic operations and mutex for thread safety:

```go
type ParentWorker struct {
    // Circuit breaker state
    consecutiveFailures int
    circuitOpen         bool
    mu                  sync.RWMutex
}

func (pw *ParentWorker) recordFailure() {
    pw.mu.Lock()
    defer pw.mu.Unlock()
    
    pw.consecutiveFailures++
    if pw.consecutiveFailures >= pw.MaxRestartAttempts {
        pw.circuitOpen = true
        if pw.metrics != nil {
            pw.metrics.RecordCircuitBreakerTrip()
        }
    }
}

func (pw *ParentWorker) recordSuccess() {
    pw.mu.Lock()
    defer pw.mu.Unlock()
    
    if pw.consecutiveFailures > 0 {
        pw.consecutiveFailures = 0
        if pw.circuitOpen {
            pw.circuitOpen = false
            if pw.metrics != nil {
                pw.metrics.RecordCircuitBreakerReset()
            }
        }
    }
}

// Check before making calls
func (pw *ParentWorker) CallFunction(ctx context.Context, functionName string, args ...any) (any, error) {
    if pw.circuitOpen {
        return nil, &CircuitOpenError{ConsecutiveFailures: pw.consecutiveFailures}
    }
    // ... proceed with call
}
```

### Heartbeat Monitoring

Spawn mode only - parent sends periodic heartbeats to child:

```go
func (pw *ParentWorker) heartbeatLoop() {
    // Wait for initial connection
    time.Sleep(1 * time.Second)
    
    for pw.running && pw.heartbeatRunning {
        // Create heartbeat message with timestamp
        heartbeat := CreateHeartbeat("")
        
        // Send heartbeat
        data, _ := heartbeat.Pack()
        zmqMsg := zmq.NewMsgFrom([]byte{}, data)
        pw.sendMessageWithRetry(zmqMsg, 3)
        
        // Wait for response with timeout
        select {
        case <-responseChan:
            // Success - RTT recorded in handleHeartbeatResponse
            pw.consecutiveHeartbeatMisses = 0
        case <-time.After(pw.HeartbeatTimeout):
            // Heartbeat missed
            pw.consecutiveHeartbeatMisses++
            if pw.consecutiveHeartbeatMisses >= pw.HeartbeatMaxMisses {
                pw.recordFailure()
                return
            }
        }
        
        time.Sleep(pw.HeartbeatInterval)
    }
}

// Child responds to heartbeats
func (w *ChildWorker) handleHeartbeat(msg *ComlinkMessage, senderID []byte) {
    // Extract original timestamp
    var originalTs float64
    if msg.Metadata != nil {
        if ts, ok := msg.Metadata["hb_timestamp"].(float64); ok {
            originalTs = ts
        }
    }
    
    // Create response with same ID and timestamp
    response := CreateHeartbeatResponse(msg.ID, originalTs)
    
    // Send response
    data, _ := response.Pack()
    zmqMsg := zmq.NewMsgFrom(senderID, []byte{}, data)
    w.socket.Send(zmqMsg)
}
```

### Metrics Collection

Thread-safe metrics using `sync.RWMutex`:

```go
type Metrics struct {
    mu sync.RWMutex
    
    // Counters
    requestsTotal   int
    requestsSuccess int
    requestsFailed  int
    
    // Latency samples (circular buffer)
    latencies []float64
    
    // Heartbeat tracking
    heartbeatRtts   []float64
    heartbeatMisses int
}

func (m *Metrics) StartRequest(requestID, function, namespace string) time.Time {
    m.mu.Lock()
    defer m.mu.Unlock()
    
    m.requestsTotal++
    m.queueDepth++
    if m.queueDepth > m.queueMaxDepth {
        m.queueMaxDepth = m.queueDepth
    }
    
    return time.Now()
}

func (m *Metrics) EndRequest(startTime time.Time, requestID string, success bool, errorMsg string) float64 {
    latencyMs := float64(time.Since(startTime).Milliseconds())
    
    m.mu.Lock()
    defer m.mu.Unlock()
    
    m.queueDepth--
    
    if success {
        m.requestsSuccess++
    } else {
        m.requestsFailed++
    }
    
    // Store latency sample (circular buffer)
    if len(m.latencies) >= m.maxLatencySamples {
        m.latencies = m.latencies[1:]
    }
    m.latencies = append(m.latencies, latencyMs)
    
    return latencyMs
}

// Snapshot returns point-in-time copy
func (m *Metrics) Snapshot() MetricsSnapshot {
    m.mu.RLock()
    defer m.mu.RUnlock()
    
    // Calculate percentiles from latencies slice
    // Return copy of all metrics
}
```

### Retry Logic

Send with retry for transient failures:

```go
func (pw *ParentWorker) sendMessageWithRetry(msg zmq.Msg, maxRetries int) error {
    for attempt := 0; attempt < maxRetries; attempt++ {
        if err := pw.socket.Send(msg); err == nil {
            return nil // Success
        } else {
            // Check if it's a retryable error (socket busy)
            if attempt < maxRetries-1 {
                time.Sleep(100 * time.Millisecond)
                continue
            }
            return fmt.Errorf("socket busy after %d retries: %w", maxRetries, err)
        }
    }
    return fmt.Errorf("failed to send message after %d retries", maxRetries)
}
```

## Service Registry

Atomic file locking using `O_CREAT | O_EXCL`:

```go
func acquireLock() (*os.File, error) {
    lockPath := getLockPath()
    deadline := time.Now().Add(LockTimeout)
    
    for time.Now().Before(deadline) {
        // Try to create lock file exclusively
        lockFile, err := os.OpenFile(lockPath, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0600)
        if err == nil {
            return lockFile, nil
        }
        
        // Check if file exists (lock held by another process)
        if os.IsExist(err) {
            // Check if lock file is stale
            if info, statErr := os.Stat(lockPath); statErr == nil {
                if time.Since(info.ModTime()) > LockTimeout {
                    _ = os.Remove(lockPath)
                    continue
                }
            }
            time.Sleep(100 * time.Millisecond)
            continue
        }
        
        return nil, fmt.Errorf("failed to acquire lock: %w", err)
    }
    
    return nil, fmt.Errorf("lock acquisition timeout after %v", LockTimeout)
}

func releaseLock(lockFile *os.File) error {
    if lockFile == nil {
        return nil
    }
    
    lockPath := getLockPath()
    lockFile.Close()
    os.Remove(lockPath)
    return nil
}
```

**Registry path:** `~/.multifrost/services.json` (cross-platform)

## Output Forwarding

Child redirects stdout/stderr to pipes, forwards over ZMQ:

```go
func (w *ChildWorker) setupOutputRedirection() error {
    // Save original stdout/stderr
    w.originalStdout = os.Stdout
    w.originalStderr = os.Stderr
    
    // Create pipes
    stdoutR, stdoutW, _ := os.Pipe()
    w.stdoutPipe = stdoutW
    
    stderrR, stderrW, _ := os.Pipe()
    w.stderrPipe = stderrW
    
    // Redirect stdout/stderr to pipes
    os.Stdout = stdoutW
    os.Stderr = stderrW
    
    // Start goroutines to forward output
    go w.forwardOutput(stdoutR, MessageTypeStdout)
    go w.forwardOutput(stderrR, MessageTypeStderr)
    
    return nil
}

func (w *ChildWorker) forwardOutput(pipe *os.File, msgType MessageType) {
    buf := make([]byte, 4096)
    for {
        n, err := pipe.Read(buf)
        if n > 0 {
            output := string(buf[:n])
            w.sendOutput(output, msgType)
        }
        if err != nil {
            break
        }
    }
}

func (w *ChildWorker) sendOutput(output string, msgType MessageType) {
    if w.socket == nil || len(w.lastSenderID) == 0 {
        return
    }
    
    output = strings.TrimRight(output, "\n")
    if output == "" {
        return
    }
    
    msg := CreateOutput(output, msgType)
    data, _ := msg.Pack()
    
    // Retry up to 2 times
    for attempt := 0; attempt < 2; attempt++ {
        zmqMsg := zmq.NewMsgFrom(w.lastSenderID, []byte{}, data)
        if err := w.socket.Send(zmqMsg); err == nil {
            return
        }
        if attempt < 1 {
            time.Sleep(1 * time.Millisecond)
        }
    }
}
```

## API Surface

### ParentWorker

```go
// Factory functions
func Spawn(scriptPath string, executable ...string) *ParentWorker
func Connect(ctx context.Context, serviceID string, timeout ...time.Duration) (*ParentWorker, error)
func NewParentWorker(config ParentWorkerConfig) *ParentWorker

// Configuration
 type ParentWorkerConfig struct {
    ScriptPath         string
    Executable         string
    ServiceID          string
    Port               int
    AutoRestart        bool
    MaxRestartAttempts int        // default: 5
    DefaultTimeout     time.Duration
    HeartbeatInterval  time.Duration  // default: 5s
    HeartbeatTimeout   time.Duration  // default: 3s
    HeartbeatMaxMisses int        // default: 3
    EnableMetrics      bool
}

// Lifecycle (on worker - for process management)
func (pw *ParentWorker) Start() error
func (pw *ParentWorker) Close() error

// Handle method (v4)
func (pw *ParentWorker) Handle() *Handle

// Health & metrics (introspection on worker)
func (pw *ParentWorker) IsHealthy() bool
func (pw *ParentWorker) CircuitOpen() bool
func (pw *ParentWorker) LastHeartbeatRttMs() float64
func (pw *ParentWorker) Metrics() *Metrics
```

### Handle (v4)

The Handle provides a clean interface for lifecycle and calls:

```go
type Handle struct {
    worker *ParentWorker
    call   *CallProxy
}

// Lifecycle
func (h *Handle) Start() error
func (h *Handle) Stop() error

// Remote calls
func (h *Handle) Call(ctx context.Context, functionName string, args ...any) (any, error)
func (h *Handle) CallWithTimeout(ctx context.Context, functionName string, timeout time.Duration, args ...any) (any, error)
func (h *Handle) CallWithContext(ctx context.Context, functionName string, args ...any) (any, error)
```

**Usage:**

```go
worker := multifrost.Spawn("./math_worker.go", "go", "run")
handle := worker.Handle()

if err := handle.Start(); err != nil {
    log.Fatal(err)
}
defer handle.Stop()

result, err := handle.Call(ctx, "Add", 5, 3)
```

### Legacy API (still available)

```go
// OLD (v3) - still works
worker := multifrost.Spawn("./math_worker.go", "go", "run")
worker.Start()
result, err := worker.CallFunction(ctx, "Add", 5, 3)
worker.Close()

// NEW (v4) - recommended
worker := multifrost.Spawn("./math_worker.go", "go", "run")
handle := worker.Handle()
handle.Start()
result, err := handle.Call(ctx, "Add", 5, 3)
handle.Stop()
```

### ChildWorker

```go
// Constructors
func NewChildWorker() *ChildWorker
func NewChildWorkerWithService(serviceID string) *ChildWorker

// Lifecycle
func (w *ChildWorker) Start() error
func (w *ChildWorker) Stop()
func (w *ChildWorker) Run()  // blocking with signal handling

// Introspection
func (w *ChildWorker) ListFunctions() []string

// State
func (w *ChildWorker) GetPort() int
func (w *ChildWorker) IsRunning() bool
func (w *ChildWorker) Done() <-chan struct{}
```

## Error Handling

Go's explicit error handling is used throughout:

```go
// Custom error types
type RemoteCallError struct {
    Message string
}

func (e *RemoteCallError) Error() string {
    return e.Message
}

type CircuitOpenError struct {
    ConsecutiveFailures int
}

func (e *CircuitOpenError) Error() string {
    return fmt.Sprintf("circuit breaker open after %d consecutive failures", e.ConsecutiveFailures)
}

// Usage in ParentWorker
result, err := worker.CallFunction(ctx, "Method", args...)
if err != nil {
    if circuitErr, ok := err.(*CircuitOpenError); ok {
        // Handle circuit breaker open
    } else if remoteErr, ok := err.(*RemoteCallError); ok {
        // Handle remote error
    } else {
        // Handle other errors (timeout, connection, etc.)
    }
}
```

## Testing

Comprehensive test suite using `testify`:

```go
// Table-driven tests
func TestParentWorker_Configuration(t *testing.T) {
    tests := []struct {
        name     string
        config   ParentWorkerConfig
        expected expectedValues
    }{
        {
            name: "default timeout",
            config: ParentWorkerConfig{
                DefaultTimeout: 30 * time.Second,
            },
            expected: expectedValues{
                timeout: 30 * time.Second,
            },
        },
        // ... more test cases
    }
    
    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            worker := NewParentWorker(tt.config)
            assert.Equal(t, tt.expected.timeout, worker.DefaultTimeout)
        })
    }
}

// Subtests for organization
func TestMetrics_RequestTracking(t *testing.T) {
    t.Run("start and end request", func(t *testing.T) {
        m := NewMetrics(1000, 60.0)
        startTime := m.StartRequest("req-1", "testFunc", "default")
        latency := m.EndRequest(startTime, "req-1", true, "")
        
        assert.Greater(t, latency, 0.0)
        snapshot := m.Snapshot()
        assert.Equal(t, 1, snapshot.RequestsTotal)
    })
    
    t.Run("failed request tracking", func(t *testing.T) {
        // ...
    })
}
```

## Cross-Language Compatibility

| Feature | Go | Python | JavaScript |
|---------|-----|--------|------------|
| App ID | `comlink_ipc_v4` | `comlink_ipc_v4` | `comlink_ipc_v4` |
| Socket types | DEALER/ROUTER | DEALER/ROUTER | DEALER/ROUTER |
| Concurrency | Goroutines | Asyncio | Promises/async-await |
| Method dispatch | Reflection | `getattr()` | Direct call |
| Type system | Static | Dynamic | Dynamic |
| Error handling | Explicit errors | Exceptions | Exceptions |
| Sync calls | Yes | Yes | No |
| Circuit breaker | Yes | Yes | Yes |
| Heartbeat | Yes | Yes | Yes |
| Metrics | Yes | Yes | Yes |

## Dependencies

```go
// go.mod
require (
    github.com/go-zeromq/zmq4 v0.16.0      // Pure Go ZMQ
    github.com/google/uuid v1.6.0          // UUID generation
    github.com/vmihailenco/msgpack/v5 v5.4.1  // msgpack serialization
    github.com/stretchr/testify v1.11.1    // Testing (dev)
)
```

## Build & Test

```bash
# Build
go build ./...

# Run tests
go test ./...

# Run with verbose output
go test ./... -v

# Run with coverage
go test ./... -cover

# Vet for issues
go vet ./...
```

## Implementation Notes

1. **No async/await**: Go doesn't have async/await syntax. Concurrent operations use goroutines and channels.

2. **Reflection limitations**: Only exported methods (capitalized) can be called. Private methods (lowercase) are rejected.

3. **Context usage**: Go's `context.Context` is used for cancellation, but not for async operations like in Python.

4. **Error wrapping**: Go 1.13+ error wrapping (`%w`) is used throughout for error chain preservation.

5. **Resource cleanup**: `defer` is used extensively for resource cleanup (locks, file handles, etc.).

6. **Signal handling**: ChildWorker uses `os/signal` and `syscall` for graceful shutdown on SIGINT/SIGTERM.

7. **Zero-allocation optimizations**: Not implemented; clarity preferred over micro-optimizations.

## Future Enhancements

- [ ] Streaming responses for large payloads
- [ ] Connection pooling for multiple workers
- [ ] TLS encryption for non-localhost deployments
- [ ] Pluggable serialization (JSON, protobuf)
- [ ] Middleware/interceptor support
- [ ] Load balancing across multiple child instances
