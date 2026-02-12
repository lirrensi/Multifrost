# Multifrost Go Quick Examples

Get started with the Go implementation of Multifrost IPC library.

## Installation

```bash
# Create Go module
go mod init github.com/yourusername/multifrost-go

# Download dependencies
go get github.com/go-zeromq/zmq4
go get github.com/google/uuid
go get github.com/vmihailenco/msgpack/v5
go get github.com/stretchr/testify

# Build the library
go build ./...
```

## Quick Start: Parent-Child Example

Create a child worker with callable methods:

```go
// math_worker.go
package main

import (
    "fmt"
    "github.com/yourusername/multifrost-go/multifrost"
)

type MathWorker struct {
    multifrost.ChildWorker
}

// Add two integers
func (w *MathWorker) Add(a, b int) int {
    return a + b
}

// Multiply two integers
func (w *MathWorker) Multiply(a, b int) int {
    return a * b
}

// Fibonacci calculation
func (w *MathWorker) Fibonacci(n int) int {
    if n <= 1 {
        return n
    }
    return w.Fibonacci(n-1) + w.Fibonacci(n-2)
}

func main() {
    worker := &MathWorker{}
    worker.Run()
}
```

Create a parent that calls the child:

```go
// parent.go
package main

import (
    "context"
    "fmt"
    "log"
    "github.com/yourusername/multifrost-go/multifrost"
)

func main() {
    // Spawn the child worker
    worker, err := multifrost.Spawn("./math_worker.go")
    if err != nil {
        log.Fatalf("Failed to spawn worker: %v", err)
    }

    // Start the worker
    if err := worker.Start(); err != nil {
        log.Fatalf("Failed to start worker: %v", err)
    }
    defer worker.Close()

    // Call methods synchronously
    result1, err := worker.CallFunction(context.Background(), "Add", 5, 3)
    if err != nil {
        log.Fatalf("Add failed: %v", err)
    }
    fmt.Printf("5 + 3 = %d\n", result1.(int))

    result2, err := worker.CallFunction(context.Background(), "Multiply", 4, 7)
    if err != nil {
        log.Fatalf("Multiply failed: %v", err)
    }
    fmt.Printf("4 * 7 = %d\n", result2.(int))

    result3, err := worker.CallFunction(context.Background(), "Fibonacci", 10)
    if err != nil {
        log.Fatalf("Fibonacci failed: %v", err)
    }
    fmt.Printf("Fibonacci(10) = %d\n", result3.(int))
}
```

Run the example:

```bash
# Start the child worker in one terminal
go run math_worker.go

# In another terminal, run the parent
go run parent.go
```

## Sync API (Idiomatic Go)

Go uses synchronous calls with goroutines for concurrency - there's no async/await.

```go
package main

import (
    "context"
    "fmt"
    "log"
    "github.com/yourusername/multifrost-go/multifrost"
)

func main() {
    worker, err := multifrost.Spawn("./worker.go")
    if err != nil {
        log.Fatalf("Failed to spawn worker: %v", err)
    }
    defer worker.Close()

    // Synchronous calls
    result, err := worker.CallFunction(context.Background(), "MethodName", arg1, arg2)
    if err != nil {
        log.Fatalf("Call failed: %v", err)
    }

    fmt.Printf("Result: %v\n", result)
}
```

## Connect Mode

Register a service and connect from a parent:

```go
// worker.go
package main

import (
    "github.com/yourusername/multifrost-go/multifrost"
)

type MathWorker struct {
    multifrost.ChildWorker
}

func (w *MathWorker) Add(a, b int) int {
    return a + b
}

func main() {
    // Register service ID
    worker := &MathWorker{ChildWorker: multifrost.NewChildWorkerWithService("math-service")}
    worker.Run()
}
```

```go
// parent.go
package main

import (
    "context"
    "fmt"
    "log"
    "github.com/yourusername/multifrost-go/multifrost"
)

func main() {
    // Connect to existing service
    ctx := context.Background()
    worker, err := multifrost.Connect(ctx, "math-service", 5*time.Second)
    if err != nil {
        log.Fatalf("Failed to connect: %v", err)
    }

    if err := worker.Start(); err != nil {
        log.Fatalf("Failed to start worker: %v", err)
    }
    defer worker.Close()

    result, err := worker.CallFunction(ctx, "Add", 5, 3)
    if err != nil {
        log.Fatalf("Add failed: %v", err)
    }

    fmt.Printf("5 + 3 = %d\n", result.(int))
}
```

## Common Patterns

### Error Handling

```go
package main

import (
    "context"
    "fmt"
    "log"
    "github.com/yourusername/multifrost-go/multifrost"
)

func main() {
    worker, err := multifrost.Spawn("./worker.go")
    if err != nil {
        log.Fatalf("Failed to spawn worker: %v", err)
    }
    defer worker.Close()

    // Explicit error handling
    result, err := worker.CallFunction(context.Background(), "Add", 1, 2)
    if err != nil {
        // Check for specific error types
        if circuitErr, ok := err.(*multifrost.CircuitOpenError); ok {
            log.Printf("Circuit breaker open after %d failures", circuitErr.ConsecutiveFailures)
        } else {
            log.Printf("Call failed: %v", err)
        }
        return
    }

    fmt.Printf("Result: %v\n", result)
}
```

### Custom Timeout

```go
// Call with custom timeout
ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
defer cancel()

result, err := worker.CallFunctionWithTimeout(ctx, "MethodName", 10*time.Second, arg1, arg2)
if err != nil {
    log.Printf("Call with timeout failed: %v", err)
}
```

### List Available Methods

```go
package main

import (
    "context"
    "fmt"
    "log"
    "github.com/yourusername/multifrost-go/multifrost"
)

func main() {
    worker, err := multifrost.Spawn("./worker.go")
    if err != nil {
        log.Fatalf("Failed to spawn worker: %v", err)
    }
    defer worker.Close()

    // List methods on the child
    methods := worker.ListFunctions()
    fmt.Printf("Available methods: %v\n", methods)
}
```

### Metrics Collection

```go
package main

import (
    "context"
    "fmt"
    "log"
    "github.com/yourusername/multifrost-go/multifrost"
)

func main() {
    worker, err := multifrost.Spawn("./worker.go")
    if err != nil {
        log.Fatalf("Failed to spawn worker: %v", err)
    }
    defer worker.Close()

    // Get metrics snapshot
    metrics := worker.Metrics()
    snapshot := metrics.Snapshot()

    fmt.Printf("Total requests: %d\n", snapshot.RequestsTotal)
    fmt.Printf("Success count: %d\n", snapshot.RequestsSuccess)
    fmt.Printf("Failed count: %d\n", snapshot.RequestsFailed)

    if snapshot.RequestsTotal > 0 {
        successRate := float64(snapshot.RequestsSuccess) / float64(snapshot.RequestsTotal)
        fmt.Printf("Success rate: %.2f%%\n", successRate*100)
    }

    if len(snapshot.Latencies) > 0 {
        avgLatency := calculateAverage(snapshot.Latencies)
        fmt.Printf("Average latency: %.2fms\n", avgLatency)
    }
}

func calculateAverage(latencies []float64) float64 {
    if len(latencies) == 0 {
        return 0
    }
    var sum float64
    for _, lat := range latencies {
        sum += lat
    }
    return sum / float64(len(latencies))
}
```

### Health Checks

```go
package main

import (
    "context"
    "fmt"
    "log"
    "github.com/yourusername/multifrost-go/multifrost"
)

func main() {
    worker, err := multifrost.Spawn("./worker.go")
    if err != nil {
        log.Fatalf("Failed to spawn worker: %v", err)
    }
    defer worker.Close()

    // Check if worker is healthy
    fmt.Printf("Healthy: %v\n", worker.IsHealthy())
    fmt.Printf("Circuit open: %v\n", worker.CircuitOpen())
    fmt.Printf("Last heartbeat RTT: %.2fms\n", worker.LastHeartbeatRttMs())

    // Get metrics
    metrics := worker.Metrics()
    snapshot := metrics.Snapshot()
    fmt.Printf("Queue depth: %d\n", snapshot.QueueDepth)
}
```

### Auto-Restart on Crash

```go
package main

import (
    "context"
    "log"
    "github.com/yourusername/multifrost-go/multifrost"
)

func main() {
    worker, err := multifrost.Spawn("./worker.go")
    if err != nil {
        log.Fatalf("Failed to spawn worker: %v", err)
    }

    // Configure auto-restart
    worker.Config.AutoRestart = true
    worker.Config.MaxRestartAttempts = 5
    worker.Config.DefaultTimeout = 30 * time.Second
    worker.Config.HeartbeatInterval = 5 * time.Second
    worker.Config.HeartbeatTimeout = 3 * time.Second
    worker.Config.HeartbeatMaxMisses = 3
    worker.Config.EnableMetrics = true

    if err := worker.Start(); err != nil {
        log.Fatalf("Failed to start worker: %v", err)
    }
    defer worker.Close()

    // Worker will automatically restart on crash up to max attempts
    // ... usage
}
```

## Key Concepts

### ParentWorker

- **Purpose**: Initiates calls and manages child lifecycle
- **Modes**: `Spawn()` (creates new process) or `Connect()` (connects to existing service)
- **API**: Synchronous calls via `CallFunction()`
- **Concurrency**: Goroutines handle background tasks (heartbeat, metrics)

### ChildWorker

- **Purpose**: Exposes callable methods and handles requests
- **Methods**: Must be exported (capitalized) to be callable
- **Modes**: Can register with `ServiceID` for connect mode

### Goroutine-Based Concurrency

Go uses goroutines and channels for concurrency:

```go
// ChildWorker runs multiple goroutines:
// 1. messageLoop() - handles incoming requests
// 2. forwardOutput() - goroutines for stdout/stderr pipes

// ParentWorker runs:
// 1. messageLoop() - receives ZMQ messages
// 2. heartbeatLoop() - sends periodic heartbeats (spawn mode only)
```

### Reflection for Method Dispatch

Go uses reflection to call methods dynamically:

```go
func (w *ChildWorker) handleFunctionCall(msg *ComlinkMessage, senderID []byte) {
    // Find method by name using reflection
    method := reflect.ValueOf(w).MethodByName(msg.Function)
    if !method.IsValid() {
        // Send error response
        return
    }

    // Convert args to reflect.Values and call
    args := make([]reflect.Value, len(msg.Args))
    for i, arg := range msg.Args {
        args[i] = reflect.ValueOf(arg)
    }

    results := method.Call(args)
    // Handle results...
}
```

**Important**: Methods must be exported (capitalized) to be callable.

### Circuit Breaker

- Tracks consecutive failures (default: 5)
- Opens circuit after threshold
- Resets on successful call
- Uses atomic operations for thread safety
- Prevents cascading failures

```go
// Check before making calls
func (pw *ParentWorker) CallFunction(ctx context.Context, functionName string, args ...any) (any, error) {
    if pw.circuitOpen {
        return nil, &CircuitOpenError{ConsecutiveFailures: pw.consecutiveFailures}
    }
    // ... proceed with call
}
```

### Heartbeat Monitoring

- Parent sends periodic heartbeats to child (spawn mode only)
- Calculates round-trip time (RTT)
- Trips circuit breaker on missed heartbeats (default: 3 consecutive misses)
- Runs in separate goroutine

### Context for Cancellation

Use `context.Context` for cancellation:

```go
// With timeout
ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
defer cancel()

result, err := worker.CallFunction(ctx, "MethodName", arg1, arg2)
```

## Go-Specific Implementation Details

### Type Safety with `any` Type

Go 1.18+ uses `any` for dynamic types:

```go
// Message structure with any type for args/results
type ComlinkMessage struct {
    App        string   `msgpack:"app"`
    ID         string   `msgpack:"id"`
    Type       string   `msgpack:"type"`
    Timestamp  float64  `msgpack:"timestamp"`
    Function   string   `msgpack:"function,omitempty"`
    Args       []any    `msgpack:"args,omitempty"`
    Result     any      `msgpack:"result,omitempty"`
}
```

### Explicit Error Handling

Go requires explicit error handling:

```go
// Always check errors
result, err := worker.CallFunction(ctx, "MethodName", arg1)
if err != nil {
    // Handle error
}

// Use errors.Is/As for error type checking
if errors.Is(err, context.DeadlineExceeded) {
    // Handle timeout
} else if circuitErr, ok := err.(*CircuitOpenError); ok {
    // Handle circuit breaker
}
```

### sync.RWMutex for Thread Safety

Shared state is protected with mutexes:

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
    }
}
```

### Resource Cleanup with defer

Use `defer` for cleanup:

```go
func (pw *ParentWorker) Start() error {
    defer func() {
        // Cleanup happens automatically
    }()

    // Setup...
    return nil
}
```

## Cross-Language Usage

Go parent calling Python child:

```go
// parent.go
package main

import (
    "context"
    "fmt"
    "log"
    "github.com/yourusername/multifrost-go/multifrost"
)

func main() {
    // Spawn Python worker
    worker, err := multifrost.Spawn("./math_worker.py", "python")
    if err != nil {
        log.Fatalf("Failed to spawn worker: %v", err)
    }

    if err := worker.Start(); err != nil {
        log.Fatalf("Failed to start worker: %v", err)
    }
    defer worker.Close()

    // Call Python method
    result, err := worker.CallFunction(context.Background(), "factorial", 10)
    if err != nil {
        log.Fatalf("Call failed: %v", err)
    }

    fmt.Printf("Factorial: %v\n", result.(int))
}
```

```python
# math_worker.py
from multifrost import ChildWorker

class MathWorker(ChildWorker):
    def factorial(self, n: int) -> int:
        if n <= 1:
            return 1
        return n * self.factorial(n - 1)

if __name__ == "__main__":
    worker = MathWorker()
    worker.run()
```

### Go child calling JavaScript parent

```go
// child.go
package main

import (
    "github.com/yourusername/multifrost-go/multifrost"
)

type MathWorker struct {
    multifrost.ChildWorker
}

func (w *MathWorker) Add(a, b int) int {
    return a + b
}

func main() {
    worker := &MathWorker{}
    worker.Run()
}
```

```typescript
// parent.ts
import { ParentWorker } from "./src/multifrost";

async function main() {
  const worker = ParentWorker.spawn("./child.go", "go");
  await worker.start();

  const result = await worker.call.add(5, 3);
  console.log(`5 + 3 = ${result}`);

  await worker.close();
}

main();
```

## Troubleshooting

**Child exits immediately:**
- Check `COMLINK_ZMQ_PORT` environment variable
- Verify port is in range 1024-65535
- Check stderr for error messages
- Ensure script path is correct

**Method not found:**
- Verify method name is exported (capitalized)
- Check method name matches exactly (case-sensitive)
- Use `ListFunctions()` to see available methods

**Circuit breaker trips:**
- Check heartbeat configuration
- Verify child process is responding
- Review metrics for error patterns
- Increase `MaxRestartAttempts` if needed

**Timeouts:**
- Check `DefaultTimeout` configuration
- Use `CallFunctionWithTimeout()` for custom timeouts
- Verify child can handle the request within timeout

**Port conflicts:**
- Check if port is already in use
- Use `findFreePort()` helper if needed
- Verify no other process is using the port

**Context cancellation:**
- Always check `context.Canceled` error
- Use `defer cancel()` to clean up
- Ensure goroutines respect the context

**Performance issues:**
- Check metrics for high latency
- Use goroutines for parallel calls
- Optimize message size (msgpack overhead)
- Monitor queue depth in metrics

For more details, see the [full architecture documentation](./arch.md).

## Build & Test

```bash
# Build the library
go build ./...

# Run tests
go test ./... -v

# Run with coverage
go test ./... -cover

# Vet for issues
go vet ./...

# Build with race detector
go test ./... -race
```

## Dependencies

- `github.com/go-zeromq/zmq4`: Pure Go ZMQ implementation
- `github.com/google/uuid`: UUID generation
- `github.com/vmihailenco/msgpack/v5`: msgpack serialization
- `github.com/stretchr/testify`: Testing utilities (dev dependency)
