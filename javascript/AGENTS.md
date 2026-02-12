# Multifrost JavaScript/TypeScript Implementation

## Architecture Overview

The JavaScript implementation provides a Node.js-based IPC system using ZeroMQ DEALER/ROUTER sockets and msgpack serialization. Key design principles:

- **Async-Native**: All operations are Promise-based; no blocking I/O allowed
- **Proxy-Based Method Access**: Uses Proxy objects for fluent API (`worker.call.methodName(args)`)
- **TypeScript-First**: Full type safety with compile-time checking
- **Event-Driven**: Leverages Node.js event loop for efficient async operations
- **Dual Modes**: Supports both spawn mode (create new process) and connect mode (discover existing service)

## Core Components

### ParentWorker (DEALER socket)
- Manages child process lifecycle (spawn/connect)
- Handles message loop with async iteration
- Implements circuit breaker, heartbeat monitoring, metrics
- Provides proxy-based API for remote method calls

### ChildWorker (ROUTER socket)
- Runs in child process with its own event loop
- Dynamically dispatches method calls via `Record<string, unknown>`
- Redirects console output to parent
- Handles shutdown signals gracefully

## Communication Protocol

- **Socket Types**: DEALER (parent) ↔ ROUTER (child)
- **Message Format**: `[empty_frame, message_data]` for DEALER, `[sender_id, empty, message_data]` for ROUTER
- **Serialization**: msgpack v5 with deepSanitize for type safety
- **Message Types**: CALL, RESPONSE, ERROR, HEARTBEAT, STDOUT, STDERR, SHUTDOWN

## Commands

```bash
# Install dependencies
npm install

# Build TypeScript
npm run build
# or: npx tsc

# Run tests
npm test

# Lint and format
npm run lint
npm run format

# Watch mode (development)
npx tsc --watch
```

## Code Style Guidelines

### Import Organization
1. Standard library imports
2. Third-party dependencies (zeromq, msgpackr)
3. Local module imports

### Formatting
- Use Prettier with default settings
- No configuration needed

### Type Safety
- Use TypeScript with strict mode enabled
- Prefer explicit type annotations for public APIs
- Use `unknown` for dynamic values, `any` only when necessary
- Leverage interfaces for complex types

### Naming Conventions
- **Classes**: PascalCase (`ParentWorker`, `ChildWorker`)
- **Methods/Functions**: camelCase (`callFunction`, `handleMessage`)
- **Private members**: underscore prefix (`_pendingRequests`, `_consecutiveFailures`)
- **Constants**: SCREAMING_SNAKE_CASE (`APP_NAME`, `MAX_RETRIES`)
- **Interfaces**: PascalCase with `I` prefix (`IPendingRequest`, `IMetrics`)

### Error Handling
- Use custom error classes for domain errors
- Always catch and handle errors in async functions
- Provide descriptive error messages
- Use `throw new Error()` for unexpected errors
- Never suppress errors silently

### Documentation
- Use JSDoc comments for public APIs
- Include parameter types and return types
- Document async/await behavior
- Explain error conditions

```typescript
/**
 * Calls a remote method with optional timeout and namespace.
 * @param functionName - Name of the method to call
 * @param args - Arguments to pass to the method
 * @param timeout - Optional timeout in milliseconds
 * @param namespace - Optional namespace (default: "default")
 * @returns Promise resolving to the method result
 * @throws RemoteCallError if the call fails
 * @throws CircuitOpenError if the circuit breaker is open
 */
async callFunction<T = unknown>(
    functionName: string,
    args: unknown[] = [],
    timeout?: number,
    namespace: string = "default"
): Promise<T>
```

## Common Patterns

### Creating a Worker
```typescript
// Spawn mode
const worker = ParentWorker.spawn("./worker.js", "node", {
    autoRestart: true,
    defaultTimeout: 5000,
});

// Connect mode
const worker = await ParentWorker.connect("my-service");
```

### Calling Remote Methods
```typescript
// Fluent API
const result = await worker.call.factorial(10);

// With options
const result = await worker
    .withOptions({ timeout: 3000, namespace: "custom" })
    .add(1, 2);
```

### Error Handling
```typescript
try {
    const result = await worker.call.compute(100);
} catch (error) {
    if (error instanceof CircuitOpenError) {
        // Handle circuit breaker
    } else if (error instanceof RemoteCallError) {
        // Handle remote error
    }
}
```

## Cross-Language Notes

- **No Sync API**: JavaScript is single-threaded; all operations are async
- **Type Mapping**: JavaScript `number` → Python `float` (clamped to 2^53)
- **BigInt**: Converted to `float` (loses precision)
- **Error Messages**: JavaScript sends only message string; Python includes traceback
