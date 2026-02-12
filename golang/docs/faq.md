# Multifrost Go Implementation FAQ

**Version**: 1.0
**Last Updated**: 2026-02-12
**Go Version**: 1.18+

---

## Table of Contents

1. [Getting Started](#getting-started)
2. [Goroutine-Based Concurrency](#goroutine-based-concurrency)
3. [Spawn vs Connect Mode](#spawn-vs-connect-mode)
4. [Reflection-Based Method Dispatch](#reflection-based-method-dispatch)
5. [Common Gotchas and Pitfalls](#common-gotchas-and-pitfalls)
6. [Performance Considerations](#performance-considerations)
7. [Debugging Tips](#debugging-tips)
8. [Troubleshooting](#troubleshooting)
9. [Best Practices](#best-practices)
10. [Cross-Language Communication](#cross-language-communication)

---

## Getting Started

### Q: What are the system requirements for the Go implementation?

**A:** The Go implementation requires:

- **Go version**: 1.18 or higher (for `any` type support)
- **ZeroMQ**: Pure Go implementation via `github.com/go-zeromq/zmq4` (no native ZMQ libraries needed)
- **msgpack**: Version 5.x via `github.com/vmihailenco/msgpack/v5`
- **Operating System**: Cross-platform (Linux, macOS, Windows)

### Q: How do I install the Multifrost Go package?

**A:** Add the package to your `go.mod` file:

```bash
go get github.com/yourusername/multifrost@latest
```

Or manually update your `go.mod`:

```go
module your-project

go 1.18

require (
    github.com/yourusername/multifrost v0.1.0
)
```

### Q: What's the basic usage pattern?

**A:** Here's a complete example showing both spawn and connect modes:

```go
package main

import (
    "context"
    "fmt"
    "log"

    "github.com/yourusername/multifrost"
)

// Child worker methods (exported with capital letters)
func (w *Calculator) Add(a, b int) int {
    return a + b
}

func (w *Calculator) Multiply(a, b int) int {
    return a * b
}

func main() {
    // --- Spawn Mode ---
    fmt.Println("Starting in spawn mode...")

    // Create parent worker
    parent, err := multifrost.Spawn("./calculator.go", "go", "run")
    if err != nil {
        log.Fatal(err)
    }
    defer parent.Close()

    // Start the worker
    if err := parent.Start(); err != nil {
        log.Fatal(err)
    }

    // Call remote methods
    result, err := parent.CallFunction(context.Background(), "Add", 10, 20)
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Add(10, 20) = %v\n", result)

    // --- Connect Mode ---
    fmt.Println("\nStarting in connect mode...")

    // Create child worker
    child := multifrost.NewChildWorkerWithService("calculator-service")
    if err := child.Start(); err != nil {
        log.Fatal(err)
    }
    defer child.Stop()

    // Parent discovers and connects
    parent2, err := multifrost.Connect(context.Background(), "calculator-service")
    if err != nil {
        log.Fatal(err)
    }
    defer parent2.Close()

    // Call remote methods
    result, err = parent2.CallFunction(context.Background(), "Multiply", 5, 6)
    if err != nil {
        log.Fatal(err)
    }
    fmt.Printf("Multiply(5, 6) = %v\n", result)
}
```

### Q: What are the differences between ParentWorker and ChildWorker?

**A:**

| Aspect | ParentWorker | ChildWorker |
|--------|--------------|-------------|
| **Role** | Client that calls methods | Server that provides methods |
| **Socket** | DEALER (sends requests) | ROUTER (receives requests) |
| **Primary Methods** | `CallFunction()`, `Spawn()`, `Connect()` | `Start()`, `Run()`, ListFunctions() |
| **Heartbeat** | Sends heartbeats (spawn mode) | Responds to heartbeats |
| **Metrics** | Collects request metrics | Tracks own health |
| **Lifecycle** | Starts child process or connects | Binds port and registers service |

---

## Goroutine-Based Concurrency

### Q: How does the Go implementation handle concurrency?

**A:** Unlike Python's asyncio, Go uses goroutines and channels. The implementation creates concurrent goroutines for:

1. **ParentWorker**:
   - `messageLoop()`: Receives ZMQ messages
   - `heartbeatLoop()`: Sends periodic heartbeats (spawn mode only)
   - Each `CallFunction()` call runs synchronously but non-blocking via channels

2. **ChildWorker**:
   - `messageLoop()`: Handles incoming requests
   - `forwardOutput()`: Goroutines for stdout/stderr pipes

```go
// Example: Parent worker with multiple goroutines
func (pw *ParentWorker) Start() error {
    // Start ZMQ socket
    pw.socket = zmq.NewDealer(context.Background())
    if err := pw.socket.Listen(fmt.Sprintf("tcp://*:%d", pw.port)); err != nil {
        return err
    }

    // Start goroutines
    go pw.messageLoop()    // Handle incoming messages
    go pw.heartbeatLoop()  // Send heartbeats (spawn mode)

    pw.running = true
    return nil
}
```

### Q: Does Go have async/await like Python?

**A:** **No**. Go doesn't have async/await syntax. Concurrent operations use:

- **Goroutines**: Lightweight threads managed by Go runtime
- **Channels**: Type-safe communication between goroutines
- **Select**: For multiplexing channels

```go
// ❌ Wrong - Go doesn't have async/await
// await CallFunction(ctx, "Method", args...)

// ✅ Correct - Synchronous call (blocking)
result, err := worker.CallFunction(ctx, "Method", args...)

// ✅ Correct - Non-blocking via goroutine
go func() {
    result, err := worker.CallFunction(context.Background(), "Method", args...)
    if err != nil {
        log.Printf("Error: %v", err)
    }
    // Handle result
}()
```

### Q: How do I handle multiple concurrent calls?

**A:** Use goroutines for each call:

```go
// Execute multiple calls concurrently
results := make(chan Result, len(operations))

for _, op := range operations {
    go func(operation Operation) {
        result, err := worker.CallFunction(context.Background(), "Operation", operation)
        if err != nil {
            results <- Result{Error: err}
            return
        }
        results <- Result{Value: result}
    }(op)
}

// Collect results
for i := 0; i < len(operations); i++ {
    result := <-results
    if result.Error != nil {
        log.Printf("Error in operation %d: %v", i, result.Error)
    }
}
```

### Q: How are shared resources protected?

**A:** Use `sync.RWMutex` for read-write locks:

```go
type Metrics struct {
    mu sync.RWMutex

    requestsTotal int
    requestsSuccess int
    queueDepth int
}

func (m *Metrics) IncrementRequests() {
    m.mu.Lock()
    defer m.mu.Unlock()
    m.requestsTotal++
    m.queueDepth++
}

func (m *Metrics) GetQueueDepth() int {
    m.mu.RLock()
    defer m.mu.RUnlock()
    return m.queueDepth
}
```

---

## Spawn vs Connect Mode

### Q: What is spawn mode?

**A:** Spawn mode creates a child process from a script:

```go
// Spawn a child process from a Go script
worker, err := multifrost.Spawn("./math.go", "go", "run")
if err != nil {
    log.Fatal(err)
}
defer worker.Close()

if err := worker.Start(); err != nil {
    log.Fatal(err)
}

// Call methods on the child process
result, err := worker.CallFunction(context.Background(), "Calculate", 5, 10)
```

**Key characteristics:**
- Parent creates child process using `exec.Command()`
- Child sets `COMLINK_ZMQ_PORT` environment variable
- Child connects back to parent (ROUTER → DEALER)
- Parent sends heartbeats to monitor child health

### Q: What is connect mode?

**A:** Connect mode discovers services and connects:

```go
// Child registers a service
child := multifrost.NewChildWorkerWithService("math-service")
if err := child.Start(); err != nil {
    log.Fatal(err)
}
defer child.Stop()

// Parent discovers and connects
parent, err := multifrost.Connect(context.Background(), "math-service")
if err != nil {
    log.Fatal(err)
}
defer parent.Close()

// Call methods as usual
result, err := parent.CallFunction(context.Background(), "Calculate", 5, 10)
```

**Key characteristics:**
- Child binds to a port and registers in `~/.multifrost/services.json`
- Parent discovers port via service registry
- Uses file locking for thread-safe registration
- No heartbeat from parent to child

### Q: What are the differences between spawn and connect?

**A:**

| Feature | Spawn Mode | Connect Mode |
|---------|------------|--------------|
| **Process Creation** | Parent spawns child process | Child runs independently |
| **Service Discovery** | Not needed (direct connection) | Registry-based discovery |
| **Heartbeat** | Parent sends to child | No heartbeat |
| **Output Forwarding** | Stdout/stderr forwarded | No output forwarding |
| **Use Case** | Testing, quick prototypes | Long-running services |
| **Lifecycle** | Parent manages child | Independent processes |

### Q: Which mode should I use?

**A:**

**Use Spawn Mode when:**
- Testing your code
- Running short-lived tasks
- Need to debug child process output
- Want to restart child on failure (auto-restart enabled)

**Use Connect Mode when:**
- Deploying production services
- Need multiple instances of same service
- Want service discovery and load balancing
- Child process should run independently

---

## Reflection-Based Method Dispatch

### Q: How does method dispatch work in Go?

**A:** Go uses reflection to call methods dynamically based on their names:

```go
func (w *ChildWorker) handleFunctionCall(msg *ComlinkMessage, senderID []byte) {
    // Find method by name using reflection
    method := reflect.ValueOf(w).MethodByName(msg.Function)
    if !method.IsValid() {
        // Method not found
        return
    }

    // Convert args to reflect.Values
    args := make([]reflect.Value, len(msg.Args))
    for i, arg := range msg.Args {
        args[i] = reflect.ValueOf(arg)
    }

    // Call the method
    results := method.Call(args)

    // Handle results
    // ...
}
```

### Q: What methods can I call?

**A:** Only **exported** (capitalized) methods can be called:

```go
// ✅ Exported - can be called remotely
func (w *Worker) Add(a, b int) int {
    return a + b
}

// ❌ Unexported - cannot be called remotely
func (w *worker) subtract(a, b int) int {
    return a - b
}
```

### Q: How do I return multiple values?

**A:** Go supports multiple return values. The last value is treated as the error:

```go
// Return result and error
func (w *Calculator) Divide(a, b int) (int, error) {
    if b == 0 {
        return 0, fmt.Errorf("division by zero")
    }
    return a / b, nil
}

// Call method
result, err := worker.CallFunction(context.Background(), "Divide", 10, 2)
if err != nil {
    log.Fatal(err)
}
fmt.Printf("Result: %v\n", result) // 5
```

### Q: How do I pass different types?

**A:** Use `any` type for dynamic values:

```go
// Child worker accepts any type
func (w *Processor) ProcessData(data any) (any, error) {
    switch v := data.(type) {
    case string:
        return strings.ToUpper(v), nil
    case int:
        return v * 2, nil
    case []int:
        sum := 0
        for _, num := range v {
            sum += num
        }
        return sum, nil
    default:
        return nil, fmt.Errorf("unsupported type: %T", v)
    }
}

// Call with different types
result, _ := worker.CallFunction(context.Background(), "ProcessData", "hello")
fmt.Printf("String: %v\n", result) // "HELLO"

result, _ = worker.CallFunction(context.Background(), "ProcessData", 42)
fmt.Printf("Int: %v\n", result) // 84

result, _ = worker.CallFunction(context.Background(), "ProcessData", []int{1, 2, 3})
fmt.Printf("Slice: %v\n", result) // 6
```

### Q: Are there limitations to reflection?

**A:** Yes:

1. **Only exported methods**: Private methods cannot be called
2. **Runtime type checking**: Method signatures are checked at runtime
3. **No async methods**: All methods must be synchronous
4. **No pointer receivers**: Methods must take the value, not the pointer

```go
// ❌ Not supported - pointer receiver
func (w *Worker) process() error {
    // Cannot be called remotely
}

// ✅ Supported - value receiver
func (w Worker) process() error {
    // Can be called remotely
}
```

---

## Common Gotchas and Pitfalls

### Q: Why is my method not being called?

**A:** Common reasons:

1. **Method not exported**:
```go
// ❌ Wrong
func (w *Worker) add(a, b int) int { ... }

// ✅ Correct
func (w *Worker) Add(a, b int) int { ... }
```

2. **Wrong method name**:
```go
// Check with ListFunctions()
functions := worker.ListFunctions()
fmt.Printf("Available methods: %v\n", functions)

// Ensure you're calling the exact name
result, _ := worker.CallFunction(context.Background(), "Add", 1, 2)
```

3. **Wrong number/type of arguments**:
```go
// If method expects (int, int), don't pass (float64)
result, err := worker.CallFunction(context.Background(), "Add", 1.5, 2.5)
// This will fail or produce unexpected results
```

### Q: Why is my circuit breaker always open?

**A:** Check the consecutive failures count:

```go
if worker.CircuitOpen() {
    fmt.Printf("Circuit open after %d failures\n",
        worker.Metrics().GetConsecutiveFailures())
}
```

**Common causes:**
- Child process crashes frequently
- Network issues
- Heartbeat timeout exceeded
- Method throws unhandled errors

### Q: How do I handle timeouts?

**A:** Use `CallFunctionWithTimeout()`:

```go
// Timeout after 5 seconds
ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
defer cancel()

result, err := worker.CallFunctionWithTimeout(ctx, "SlowOperation", 5*time.Second, args...)
if err != nil {
    if errors.Is(err, context.DeadlineExceeded) {
        log.Println("Operation timed out")
    } else {
        log.Printf("Error: %v", err)
    }
}
```

### Q: Why is my child process not responding?

**A:** Check these things:

1. **Port binding**: Ensure the child can bind to the port
2. **Environment variables**: Set `COMLINK_ZMQ_PORT` in spawn mode
3. **Heartbeat monitoring**: Check if heartbeats are being sent/received
4. **Output forwarding**: Check if stdout/stderr is being redirected

```go
// Check worker health
if !worker.IsHealthy() {
    log.Println("Worker is not healthy")
}

// Check last heartbeat RTT
rtt := worker.LastHeartbeatRttMs()
fmt.Printf("Last heartbeat RTT: %.2fms\n", rtt)
```

### Q: How do I handle errors properly?

**A:** Check error types:

```go
result, err := worker.CallFunction(ctx, "Method", args...)
if err != nil {
    switch {
    case errors.Is(err, &multifrost.CircuitOpenError{}):
        log.Println("Circuit breaker is open")
    case errors.Is(err, context.DeadlineExceeded):
        log.Println("Call timed out")
    default:
        log.Printf("Call failed: %v", err)
    }
}
```

---

## Performance Considerations

### Q: How efficient are goroutines?

**A:** Goroutines are very lightweight (approx. 2KB stack). You can run thousands efficiently:

```go
// Create 1000 concurrent calls
for i := 0; i < 1000; i++ {
    go func(id int) {
        result, _ := worker.CallFunction(context.Background(), "Process", id)
        // Handle result
    }(i)
}
```

### Q: What's the message serialization overhead?

**A:** Uses msgpack which is binary and compact:

```go
// Message size: ~50-100 bytes per call (depending on data)
// Typical overhead: 1-5ms for serialization/deserialization
```

### Q: How does the circuit breaker affect performance?

**A:**

- **When closed**: No performance impact
- **When open**: Calls return immediately with `CircuitOpenError`
- **Recovery**: After successful calls, circuit resets automatically

```go
// Circuit breaker is fast when open (no network call)
if worker.CircuitOpen() {
    return &CircuitOpenError{ConsecutiveFailures: worker.Metrics().GetConsecutiveFailures()}
}
```

### Q: How do I optimize for high throughput?

**A:**

1. **Use connection pooling** (if multiple workers)
2. **Batch multiple calls** (reduce round trips)
3. **Increase heartbeat interval** (if monitoring isn't critical)
4. **Disable metrics** if not needed:
```go
worker, _ := multifrost.Spawn("./worker.go", "go", "run")
worker.DisableMetrics() // Save memory and CPU
```

### Q: What about memory usage?

**A:**

- Goroutine stack: ~2KB each
- Message buffers: Managed by msgpack
- Metrics: Circular buffer (configurable size)
- No significant memory overhead for typical use cases

---

## Debugging Tips

### Q: How do I debug the child process?

**A:** Spawn mode allows you to see child output:

```go
// Child stdout/stderr are forwarded to parent
parent, _ := multifrost.Spawn("./debug_worker.go", "go", "run")

// Output from child appears in your terminal
```

### Q: How can I see all available methods?

**A:** Use `ListFunctions()`:

```go
child := multifrost.NewChildWorkerWithService("my-service")
child.Start()

functions := child.ListFunctions()
fmt.Printf("Available methods: %v\n", functions)
```

### Q: How do I check metrics?

**A:** Access the metrics struct:

```go
// Enable metrics
worker, _ := multifrost.Spawn("./worker.go")
worker.EnableMetrics()

// Get snapshot
snapshot := worker.Metrics().Snapshot()

fmt.Printf("Total requests: %d\n", snapshot.RequestsTotal)
fmt.Printf("Success rate: %.2f%%\n",
    float64(snapshot.RequestsSuccess)/float64(snapshot.RequestsTotal)*100)
fmt.Printf("Avg latency: %.2fms\n", snapshot.AvgLatencyMs)
```

### Q: How do I monitor heartbeat status?

**A:** Check heartbeat metrics:

```go
if !worker.IsHealthy() {
    log.Println("Worker is not healthy")

    if worker.CircuitOpen() {
        log.Printf("Circuit breaker open after %d failures",
            worker.Metrics().GetConsecutiveFailures())
    }

    misses := worker.Metrics().GetHeartbeatMisses()
    if misses > 0 {
        log.Printf("Heartbeat missed: %d times", misses)
    }
}
```

### Q: How do I see the ZMQ messages?

**A:** Enable verbose logging:

```go
// Set environment variable
os.Setenv("MULTIFROST_DEBUG", "true")

parent, _ := multifrost.Spawn("./worker.go")
```

---

## Troubleshooting

### Q: "Failed to bind to port: address already in use"

**A:** Port is already in use. Options:

1. **Use different port**:
```go
worker, _ := multifrost.Spawn("./worker.go")
// Spawn finds free port automatically
```

2. **Kill existing process**:
```bash
# On Linux/macOS
lsof -ti :5555 | xargs kill -9

# On Windows
netstat -ano | findstr :5555
taskkill /PID <PID> /F
```

3. **Check for zombie processes**:
```bash
# Check for lingering ZMQ sockets
sudo lsof -i -P -n | grep LISTEN
```

### Q: "Connection refused"

**A:** Check:

1. **Child process is running**:
```bash
# Check if child process exists
ps aux | grep worker
```

2. **Port is correct**:
```go
port := worker.GetPort()
fmt.Printf("Connecting to port: %d\n", port)
```

3. **Network connectivity**:
```bash
# Test connection
telnet localhost 5555
```

### Q: "Method not found"

**A:** Verify:

1. **Method is exported** (capitalized)
2. **Method name matches exactly** (case-sensitive)
3. **Child is running**:
```go
if !child.IsRunning() {
    log.Fatal("Child worker is not running")
}
```

### Q: "Heartbeat timeout"

**A:** Increase timeout or check child health:

```go
// Increase heartbeat timeout
parent, _ := multifrost.Spawn("./worker.go")
parent.SetHeartbeatTimeout(10 * time.Second) // Default: 3s
```

**Common causes:**
- Child process overloaded
- Network latency
- Child process crashed

### Q: "Service discovery timeout"

**A:** For connect mode:

```go
// Increase discovery timeout
parent, err := multifrost.Connect(
    context.Background(),
    "service-name",
    10*time.Second, // Wait up to 10 seconds
)
```

**Check:**
1. Service is registered in `~/.multifrost/services.json`
2. No other service with same ID
3. Port is correct

### Q: "Circuit breaker always open"

**A:** Check failure reasons:

```go
// Check consecutive failures
fmt.Printf("Failures: %d\n", worker.Metrics().GetConsecutiveFailures())

// Check last errors
if snapshot, ok := worker.Metrics().GetLastError(); ok {
    log.Printf("Last error: %v", snapshot.LastError)
}
```

**Common causes:**
- Method throws unhandled errors
- Child process crashes
- Network issues
- Timeout exceeded

---

## Best Practices

### Q: How should I structure my worker code?

**A:**

```go
package main

import (
    "context"
    "fmt"

    "github.com/yourusername/multifrost"
)

// Worker struct holds state
type Calculator struct {
    // fields
}

// Exported methods (capitalized)
func (w *Calculator) Add(a, b int) int {
    return a + b
}

func (w *Calculator) Divide(a, b int) (int, error) {
    if b == 0 {
        return 0, fmt.Errorf("division by zero")
    }
    return a / b, nil
}

func (w *Calculator) ProcessData(data any) (any, error) {
    // implementation
}

// Main function for child worker
func main() {
    // Set environment for spawn mode
    if os.Getenv("COMLINK_ZMQ_PORT") != "" {
        // Spawn mode
        worker := multifrost.NewChildWorker()
        if err := worker.Start(); err != nil {
            log.Fatal(err)
        }
        worker.Run() // Blocks until signal received
    } else {
        // Connect mode
        worker := multifrost.NewChildWorkerWithService("calculator")
        if err := worker.Start(); err != nil {
            log.Fatal(err)
        }
        worker.Run()
    }
}
```

### Q: How do I handle errors gracefully?

**A:**

```go
// 1. Check for errors explicitly
result, err := worker.CallFunction(ctx, "Method", args...)
if err != nil {
    // Handle specific error types
    if errors.Is(err, context.DeadlineExceeded) {
        log.Println("Timeout")
    } else if errors.Is(err, &multifrost.CircuitOpenError{}) {
        log.Println("Circuit breaker open")
    } else {
        log.Printf("Error: %v", err)
    }
    return
}

// 2. Use typed errors
type ValidationError struct {
    Field string
    Msg   string
}

func (e *ValidationError) Error() string {
    return fmt.Sprintf("%s: %s", e.Field, e.Msg)
}

// 3. Log errors with context
if err != nil {
    log.Printf("Failed to call Method: %v (args: %v)", err, args)
}
```

### Q: Should I use auto-restart?

**A:** Yes, for production:

```go
// Enable auto-restart with circuit breaker
parent, _ := multifrost.Spawn("./worker.go")
parent.SetAutoRestart(true)
parent.SetMaxRestartAttempts(5)

// Or configure during creation
parent, _ := multifrost.Spawn("./worker.go",
    multifrost.WithAutoRestart(true),
    multifrost.WithMaxRestartAttempts(5),
)
```

### Q: How do I manage multiple workers?

**A:**

```go
// Create pool of workers
workers := make([]*multifrost.ParentWorker, 3)

for i := 0; i < 3; i++ {
    workers[i], _ = multifrost.Spawn("./worker.go")
    workers[i].Start()
}

// Round-robin or random selection
for _, worker := range workers {
    result, _ := worker.CallFunction(ctx, "Process", data)
    // Handle result
}

// Cleanup
for _, worker := range workers {
    worker.Close()
}
```

### Q: How do I test my worker code?

**A:** Use table-driven tests:

```go
func TestCalculator(t *testing.T) {
    tests := []struct {
        name     string
        a, b     int
        expected int
    }{
        {"add positive", 1, 2, 3},
        {"add negative", -1, -2, -3},
        {"add zero", 0, 5, 5},
    }

    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            calc := &Calculator{}
            result := calc.Add(tt.a, tt.b)
            if result != tt.expected {
                t.Errorf("Add(%d, %d) = %d; want %d",
                    tt.a, tt.b, result, tt.expected)
            }
        })
    }
}
```

---

## Cross-Language Communication

### Q: Can Go communicate with Python workers?

**A:** Yes! Multifrost uses a language-agnostic wire protocol:

```go
// Go parent calling Python child
import (
    "github.com/yourusername/multifrost"
)

// Python child
# worker.py
def add(a, b):
    return a + b

# Go client
worker, _ := multifrost.Spawn("./worker.py", "python", "worker.py")
result, _ := worker.CallFunction(ctx, "add", 10, 20)
fmt.Println(result) // 30
```

### Q: What about Go → JavaScript → Go?

**A:** Yes, as long as you use the same protocol:

```
Go (Parent) ←→ JavaScript (Child) ←→ Go (Parent)
```

All implementations use the same message format, socket types (DEALER/ROUTER), and msgpack serialization.

### Q: What data types are supported across languages?

**A:** msgpack supports:
- Primitives: int, float, bool, string
- Collections: []any, map[string]any
- Structs: With msgpack tags

```go
// Go child
type Request struct {
    UserID int    `msgpack:"user_id"`
    Data   string `msgpack:"data"`
}

func (w *Worker) Process(req Request) (Response, error) {
    // ...
}

// Python client
response = worker.Process({
    "user_id": 123,
    "data": "hello"
})
```

### Q: How do I handle errors across languages?

**A:** All implementations support error messages:

```go
// Go child
func (w *Worker) Divide(a, b int) (int, error) {
    if b == 0 {
        return 0, fmt.Errorf("division by zero")
    }
    return a / b, nil
}

// Python client
result = worker.Divide(10, 0)  # Raises: RemoteCallError: division by zero
```

### Q: Are there any Go-specific limitations?

**A:** Yes:

1. **Only exported methods**: Private methods cannot be called from other languages
2. **No async methods**: All methods must be synchronous
3. **Reflection limitations**: Method signatures checked at runtime
4. **Type system**: More strict than Python/JavaScript

```go
// ✅ Supported across languages
func (w *Worker) Add(a, b int) int { ... }

// ❌ Not supported
func (w *worker) add(a, b int) int { ... } // Private
```

---

## Additional Resources

### Q: Where can I find more examples?

**A:**
- See `examples/` directory in the repository
- Check test files in `golang/` directory
- Review architecture documentation in `golang/docs/arch.md`

### Q: How do I contribute to the Go implementation?

**A:** Follow the contributing guide in `CONTRIBUTING.md`:

1. Fork the repository
2. Create a feature branch
3. Write tests for new features
4. Ensure all tests pass
5. Submit a pull request

### Q: What's the license?

**A:** See `LICENSE` file. The implementation follows the same license as the Multifrost project.

---

## Appendix: Quick Reference

### Common Commands

```bash
# Build
go build ./...

# Test
go test ./... -v

# Run with coverage
go test ./... -cover

# Vet
go vet ./...

# Install dependencies
go mod tidy
go mod download
```

### Common Patterns

```go
// Spawn a worker
worker, _ := multifrost.Spawn("./worker.go")
defer worker.Close()
worker.Start()

// Connect to a service
worker, _ := multifrost.Connect(ctx, "service-name")
defer worker.Close()

// Make a call
result, err := worker.CallFunction(ctx, "MethodName", args...)
if err != nil {
    log.Fatal(err)
}

// Check health
if worker.IsHealthy() {
    fmt.Println("Worker is healthy")
}

// Get metrics
snapshot := worker.Metrics().Snapshot()
```

### Error Handling

```go
result, err := worker.CallFunction(ctx, "Method", args...)
if err != nil {
    if errors.Is(err, context.DeadlineExceeded) {
        // Handle timeout
    } else if errors.Is(err, &multifrost.CircuitOpenError{}) {
        // Handle circuit breaker
    } else {
        // Handle other error
    }
}
```

---

**End of FAQ**
