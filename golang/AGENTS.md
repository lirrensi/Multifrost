# Multifrost Go Implementation - Agent Guide

## Architecture Overview

The Go implementation uses goroutines for concurrent execution and reflection-based method dispatch. Unlike Python's async/await, Go uses goroutines and channels for concurrency.

```
ParentWorker (DEALER)          ChildWorker (ROUTER)
├─ Spawn mode                  ├─ Method dispatch
├─ Connect mode                ├─ Reflection-based calls
├─ Circuit breaker             ├─ Output forwarding
├─ Heartbeat monitoring        ├─ Heartbeat response
├─ Metrics collection          └─ Signal handling
└─ Retry logic
```

**Key Patterns:**
- **No async/await**: Go doesn't have async/await syntax. Concurrent operations use goroutines and channels.
- **Reflection-based dispatch**: Methods must be exported (capitalized) to be callable.
- **msgpack serialization**: Uses vmihailenco/msgpack/v5 for message serialization.
- **ZeroMQ**: Pure Go implementation (github.com/go-zeromq/zmq4) with DEALER/ROUTER socket types.

## Concurrency Model

```go
// Goroutines run concurrently:
// - ParentWorker: messageLoop(), heartbeatLoop(), CallFunction()
// - ChildWorker: messageLoop(), forwardOutput()

// Channels provide type-safe communication between goroutines
// sync.RWMutex protects shared state (pending requests, metrics)
```

## Build & Test Commands

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

## Code Style Guidelines

### Imports
- Group imports: Standard library → Third-party → Local
- Sort alphabetically within each group

### Formatting
- Use `gofmt` (automatic, no config needed)
- No custom formatting required

### Type Safety
- Strong typing throughout
- Use `any` (Go 1.18+) for dynamic values
- Prefer `Result<T, E>` over `Option<T>` for error handling

### Naming Conventions
- **Exported names**: PascalCase (e.g., `ParentWorker`, `CallFunction`)
- **Private names**: camelCase (e.g., `messageLoop`, `config`)
- **Constants**: SCREAMING_SNAKE_CASE (e.g., `LockTimeout`)

### Error Handling
- Use explicit errors: `if err != nil { return err }`
- Wrap errors with `%w` for error chain preservation
- Return typed errors: `*CircuitOpenError`, `*RemoteCallError`
- Use `defer` for resource cleanup (locks, file handles)

### Documentation
- Use Go documentation comments: `//` for package/file, `///` for types/methods
- Document exported types and methods
- Include example usage in comments

### Code Patterns
- Use `defer` for cleanup
- Use `context.Context` for cancellation
- Use table-driven tests with `testify`
- Use atomic operations for simple counters
- Use mutexes for complex shared state

## Implementation Notes

1. **Reflection**: Only exported methods (capitalized) can be called. Private methods are rejected.

2. **Context**: Used for cancellation, not async operations.

3. **Socket types**: DEALER (parent) ↔ ROUTER (child) with multipart message framing.

4. **Heartbeat**: Spawn mode only - parent sends periodic heartbeats to child.

5. **Circuit breaker**: Tracks consecutive failures and opens after threshold.

6. **Metrics**: Thread-safe using `sync.RWMutex`.

## API Surface

```go
// ParentWorker
Spawn(scriptPath string, executable ...string) *ParentWorker
Connect(ctx context.Context, serviceID string, timeout ...time.Duration) (*ParentWorker, error)
CallFunction(ctx context.Context, functionName string, args ...any) (any, error)

// ChildWorker
NewChildWorkerWithService(serviceID string) *ChildWorker
ListFunctions() []string
```
