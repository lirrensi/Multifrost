# Multifrost JavaScript FAQ

## Table of Contents

- [Getting Started](#getting-started)
- [TypeScript vs JavaScript](#typescript-vs-javascript)
- [Proxy-Based Method Access](#proxy-based-method-access)
- [Spawn vs Connect Mode](#spawn-vs-connect-mode)
- [Common Gotchas](#common-gotchas)
- [Performance Considerations](#performance-considerations)
- [Debugging Tips](#debugging-tips)
- [Troubleshooting](#troubleshooting)
- [Best Practices](#best-practices)
- [Cross-Language Communication](#cross-language-communication)

---

## Getting Started

### What are the prerequisites for using Multifrost with JavaScript?

You need:
- **Node.js** (v18 or higher recommended)
- **npm** or **yarn** package manager
- **TypeScript** (v5.0.0 or higher)
- **ZeroMQ** bindings (`zeromq` package)
- **msgpack** serializer (`msgpackr` package)

Install dependencies:
```bash
npm install zeromq msgpackr
npm install -D typescript @types/node
```

### How do I create a simple worker script?

```typescript
// worker.ts
import { ChildWorker } from "./src/multifrost.js";

class CalculatorWorker extends ChildWorker {
    async add(a: number, b: number): Promise<number> {
        return a + b;
    }

    async multiply(a: number, b: number): Promise<number> {
        return a * b;
    }

    async fibonacci(n: number): Promise<number> {
        if (n <= 1) return n;
        return this.fibonacci(n - 1) + this.fibonacci(n - 2);
    }
}

// Run the worker
new CalculatorWorker().run();
```

Run it with Node.js:
```bash
node worker.ts
```

### How do I connect to the worker from a parent process?

```typescript
// parent.ts
import { ParentWorker } from "./src/multifrost.js";

async function main() {
    // Spawn a new worker process
    const worker = ParentWorker.spawn("./worker.ts", "node", {
        autoRestart: true,
        defaultTimeout: 5000,
    });

    await worker.start();

    try {
        // Call remote methods using the proxy API
        const sum = await worker.call.add(5, 3);
        console.log("5 + 3 =", sum);

        const product = await worker.call.multiply(4, 7);
        console.log("4 * 7 =", product);

        const fib = await worker.call.fibonacci(10);
        console.log("fib(10) =", fib);
    } finally {
        await worker.stop();
    }
}

main().catch(console.error);
```

### What are the available configuration options?

```typescript
const worker = ParentWorker.spawn("./worker.ts", "node", {
    // Process management
    autoRestart: true,          // Restart child on crash (default: false)
    maxRestartAttempts: 5,      // Max restart attempts before giving up (default: 5)

    // Timeouts
    defaultTimeout: 5000,       // Default request timeout in ms (default: 5000)
    heartbeatInterval: 5.0,     // Heartbeat interval in seconds (default: 5.0)
    heartbeatTimeout: 3.0,      // Heartbeat timeout in seconds (default: 3.0)
    heartbeatMaxMisses: 3,      // Max consecutive misses before circuit open (default: 3)
});

// Connect mode doesn't support autoRestart
const worker = await ParentWorker.connect("my-service");
```

---

## TypeScript vs JavaScript

### Does Multifrost support JavaScript (without TypeScript)?

**No, Multifrost JavaScript implementation requires TypeScript.**

The library is designed with TypeScript-first development in mind because:
- The API is heavily typed for better developer experience
- Type safety is critical for cross-language communication
- The architecture relies on TypeScript interfaces for message protocol validation

**To use JavaScript without TypeScript:**
1. Install TypeScript: `npm install -D typescript`
2. Compile your code: `npx tsc`
3. Run the compiled JavaScript: `node dist/parent.js`

### What TypeScript version is required?

**TypeScript ^5.0.0** is required. This version provides:
- Better async/await support
- Improved type inference for generic types
- Enhanced Promise type checking
- Better error messages

### Why does the API use `unknown` instead of `any`?

TypeScript's `unknown` type is safer than `any` because:
- You must perform type checking before using the value
- It prevents accidental type errors at runtime
- IDE autocomplete still works effectively
- It aligns with TypeScript best practices

```typescript
// Good: Use unknown
const result: unknown = await worker.call.add(1, 2);
if (typeof result === "number") {
    console.log("Result:", result);
}

// Avoid: Using any
const result: any = await worker.call.add(1, 2); // Less safe
```

---

## Proxy-Based Method Access

### How does the proxy-based API work?

The API uses JavaScript `Proxy` objects to intercept property access, providing a fluent interface:

```typescript
// Under the hood, this is what happens:
const worker = await ParentWorker.connect("my-service");

// Proxy intercepts property access
const callProxy = new Proxy({ _controller: worker }, {
    get(target, prop: string) {
        // Returns a function for each property name
        return (...args: unknown[]) => {
            return target._controller.callFunction(prop, args);
        };
    },
});

// Usage
await callProxy.factorial(10);  // Returns a function
await callProxy.add(1, 2);     // Returns another function
```

### Can I chain method calls with options?

Yes! The proxy returns itself for chaining:

```typescript
const result = await worker
    .withOptions({ timeout: 5000, namespace: "math" })
    .add(1, 2)
    .multiply(3, 4);

// Equivalent to:
// const result = await worker.callFunction("add", [1, 2], 5000, "math");
// const result = await worker.callFunction("multiply", [result, 3, 4], 5000, "math");
```

### What's the difference between `call.factorial()` and `callFunction("factorial", [10])`?

Both achieve the same result, but the proxy provides better syntax:

| Syntax | Description | Type Safety |
|--------|-------------|-------------|
| `call.factorial(10)` | Fluent, readable | Good (inferred) |
| `callFunction("factorial", [10])` | Direct call | Minimal |

```typescript
// Fluent API (recommended)
const result = await worker.call.add(1, 2);

// Direct call (alternative)
const result = await worker.callFunction("add", [1, 2]);
```

### Can I use namespaces with the proxy API?

Yes! Pass the namespace in `withOptions()`:

```typescript
const mathWorker = await ParentWorker.connect("math-service");

const result = await mathWorker
    .withOptions({ namespace: "calculator" })
    .add(1, 2);

// Or callFunction with namespace parameter
const result = await mathWorker.callFunction(
    "add",
    [1, 2],
    undefined,
    "calculator"
);
```

---

## Spawn vs Connect Mode

### What's the difference between spawn and connect mode?

| Feature | Spawn Mode | Connect Mode |
|---------|-----------|--------------|
| **Purpose** | Create new child process | Connect to existing service |
| **Command** | `ParentWorker.spawn("./worker.ts", "node")` | `ParentWorker.connect("my-service")` |
| **Port Management** | Finds free port automatically | Uses service registry |
| **Process Lifecycle** | Parent manages lifecycle | External process manages |
| **Auto Restart** | Supported | Not applicable |
| **Best For** | Standalone workers | Distributed services |

### When should I use spawn mode?

Use spawn mode when:
- You need full control over the child process
- You want automatic restart on crash
- The worker should run alongside your application
- You need to pass environment variables to the worker

```typescript
const worker = ParentWorker.spawn("./worker.ts", "node", {
    autoRestart: true,  // Restart on crash
    defaultTimeout: 5000,
});

// Pass custom environment
const worker = ParentWorker.spawn("./worker.ts", "node", {
    env: { CUSTOM_VAR: "value" },
});
```

### When should I use connect mode?

Use connect mode when:
- The worker is already running as a service
- Multiple clients need to connect to the same worker
- You want to use the service registry for discovery
- The worker is managed separately (e.g., by systemd)

```typescript
// Worker registers itself on startup
const worker = new MathWorker("math-service");
worker.run();

// Parent discovers and connects
const client = await ParentWorker.connect("math-service");
const result = await client.call.add(1, 2);
```

### How does service discovery work?

1. **Worker starts**: Registers itself in `~/.multifrost/services.json`
2. **Client connects**: Queries the registry for the service
3. **Connection**: Connects to the registered port
4. **Cleanup**: Worker unregisters when it stops

```typescript
// Client waits up to 5 seconds for discovery
const worker = await ParentWorker.connect("my-service", 5000);
```

### Can I mix spawn and connect in the same application?

Yes! You can use both modes simultaneously:

```typescript
// Spawn a local worker
const localWorker = ParentWorker.spawn("./worker.ts", "node");

// Connect to a remote service
const remoteWorker = await ParentWorker.connect("math-service");

// Use both
await localWorker.call.add(1, 2);
await remoteWorker.call.multiply(3, 4);
```

---

## Common Gotchas

### Why do I see "Cannot call private method '_method'" errors?

Child workers reject calls to methods starting with `_`:

```typescript
class MyWorker extends ChildWorker {
    // Public method - works
    public async getData(): Promise<string> {
        return "data";
    }

    // Private method - throws error
    private async _internalMethod(): Promise<void> {
        // This will throw: "Cannot call private method '_internalMethod'"
    }
}
```

**Solution**: Don't use private methods, or prefix with `public`.

### Why is NaN or Infinity converted to null?

Message sanitization converts these values to `null` for msgpack compatibility:

```typescript
// JavaScript
const result = await worker.call.divide(5, 0);  // NaN
// Sent as null to msgpack
// Received as null on the other side

// JavaScript
const result = await worker.call.divide(Infinity, 1);  // Infinity
// Sent as null to msgpack
// Received as null on the other side
```

**Solution**: Validate inputs before calling remote methods:

```typescript
if (b === 0) {
    throw new Error("Cannot divide by zero");
}
const result = await worker.call.divide(a, b);
```

### Why do I get "Maximum safe integer exceeded" errors?

JavaScript numbers are 64-bit floats. When values exceed `2^53`, precision is lost:

```typescript
// Large integer (loses precision)
const hugeNumber = BigInt(9007199254740991);
// Converted to number: 9007199254740991.0
// msgpack sends as float64

// Cross-language issue
// Python receives: 9007199254740992 (off by 1)
```

**Solution**: Use Python or other languages for large integers, or accept precision loss.

### Why does my worker exit immediately?

Common causes:
1. **Missing COMLINK_ZMQ_PORT**: Child process doesn't know where to connect
2. **Invalid port**: Port not in 1024-65535 range
3. **Script path error**: Worker script doesn't exist or can't execute

```typescript
// Check environment variable is set
console.log("COMLINK_ZMQ_PORT:", process.env.COMLINK_ZMQ_PORT);

// Check port validation
if (port < 1024 || port > 65535) {
    console.error("Invalid port:", port);
    process.exit(1);
}
```

### Why doesn't async/await work in my methods?

Remote methods must return Promises:

```typescript
class MyWorker extends ChildWorker {
    // Good: Returns Promise
    async getData(): Promise<string> {
        return new Promise(resolve => setTimeout(() => resolve("data"), 1000));
    }

    // Bad: Not async, doesn't return Promise
    getDataSync(): string {
        return "data";  // This returns immediately, not a Promise
    }
}
```

### Why do I get "Circuit breaker open" errors?

The circuit breaker trips when:
1. Child process crashes
2. Too many timeout errors
3. Too many heartbeat misses

```typescript
const worker = ParentWorker.spawn("./worker.ts", "node", {
    maxRestartAttempts: 3,  // Trips after 3 failures
});

// Wait a bit, then reset (if autoRestart is true)
await new Promise(resolve => setTimeout(resolve, 10000));
// Or manually restart
await worker.stop();
await worker.start();
```

---

## Performance Considerations

### How does Multifrost handle concurrency?

Multifrost uses a **single-threaded event loop** with async operations:

```typescript
// All requests are serialized in the event loop
const result1 = await worker.call.add(1, 2);  // Waits for completion
const result2 = await worker.call.multiply(3, 4);  // Waits for completion
// Total: 2 round-trip times

// Better: Parallel processing (if child supports it)
const [result1, result2] = await Promise.all([
    worker.call.add(1, 2),
    worker.call.multiply(3, 4),
]);
```

### What's the impact of msgpack serialization?

msgpack is **fast and compact**, but has overhead:

| Data Type | JavaScript Size | msgpack Size | Overhead |
|-----------|----------------|--------------|----------|
| String | ~50 bytes | ~20 bytes | -40% |
| Number | ~8 bytes | ~5 bytes | -37% |
| Object | ~100 bytes | ~40 bytes | -60% |

**Best practices**:
- Use smaller data structures
- Avoid nested objects (flatten if possible)
- Use primitive types when appropriate

### How does heartbeat affect performance?

Heartbeat has minimal impact (~1ms overhead per interval):

```typescript
// Default heartbeat: every 5 seconds
// Overhead: 1ms every 5 seconds = 0.02% CPU time
const worker = ParentWorker.spawn("./worker.ts", "node", {
    heartbeatInterval: 5.0,  // Default
    heartbeatTimeout: 3.0,
    heartbeatMaxMisses: 3,
});
```

**Adjustment**:
- Increase interval for lower overhead: `heartbeatInterval: 10.0`
- Decrease interval for faster failure detection: `heartbeatInterval: 2.0`

### Should I use connect or spawn for performance?

| Mode | Startup Time | Memory Usage | Overhead |
|------|--------------|--------------|----------|
| **Spawn** | ~200ms | High (fork + init) | Process overhead |
| **Connect** | ~10ms | Low | Network overhead |

**Recommendation**:
- **Spawn**: For frequent short-lived workers (high overhead acceptable)
- **Connect**: For long-lived services (lower overhead)

### How can I optimize network latency?

1. **Use localhost**: Reduce network hops
2. **Enable heartbeat**: Detect failures quickly
3. **Use smaller timeouts**: Don't wait too long
4. **Reuse connections**: Keep workers alive

```typescript
// Optimize for local development
const worker = ParentWorker.spawn("./worker.ts", "node", {
    heartbeatInterval: 5.0,
    heartbeatTimeout: 3.0,
    heartbeatMaxMisses: 3,
    defaultTimeout: 3000,  // Shorter timeout
});
```

---

## Debugging Tips

### How can I see console output from the child worker?

Child workers redirect `console.log` and `console.error` to the parent:

```typescript
// In child worker
console.log("Debug message");  // Appears in parent console
console.error("Error message");  // Appears in parent console with [STDERR] prefix

// In parent
await worker.call.doSomething();  // Logs appear automatically
```

### How do I enable verbose logging?

```typescript
// Add debug flag to options
const worker = ParentWorker.spawn("./worker.ts", "node", {
    debug: true,  // Enable debug mode (if implemented)
});

// Or check environment
if (process.env.DEBUG === "multifrost") {
    console.log("Debug mode enabled");
}
```

### How can I measure request latency?

```typescript
const worker = ParentWorker.spawn("./worker.ts", "node");

await worker.start();

// Measure latency
const start = Date.now();
const result = await worker.call.add(1, 2);
const latency = Date.now() - start;

console.log(`Request latency: ${latency}ms`);
console.log(`Result: ${result}`);
```

### How can I inspect message traffic?

```typescript
// Add message logging (if implemented)
import { ParentWorker } from "./src/multifrost.js";

// Monkey-patch the send method
const originalSend = ParentWorker.prototype._sendMessage;

ParentWorker.prototype._sendMessage = async function(...args: unknown[]) {
    console.log("Sending message:", args);
    await originalSend.apply(this, args);
};

// Or use a debugging library
const { debug } = require("debug");
const debugZmq = debug("multifrost:zmq");

debugZmq("Sending message to %s", this.port);
```

### How can I debug TypeScript errors?

```bash
# Enable strict mode in tsconfig.json
{
  "compilerOptions": {
    "strict": true,
    "noImplicitAny": true,
    "strictNullChecks": true
  }
}

# Run TypeScript compiler in check mode
npx tsc --noEmit

# Use tsc-watch for live debugging
npx tsc --watch
```

### How can I profile performance?

```typescript
import { performance } from "perf_hooks";

// Profile a function
const start = performance.now();
const result = await worker.call.compute(1000);
const duration = performance.now() - start;

console.log(`Execution time: ${duration.toFixed(2)}ms`);
```

---

## Troubleshooting

### Error: "Child process failed to start"

**Causes**:
- Script path doesn't exist
- Node.js executable not found
- Permission denied

**Solutions**:
```typescript
// Check script path
console.log("Script path:", scriptPath);
console.log("Executable:", executable);

// Verify with Node.js
node ./worker.ts  // Should run successfully

// Check permissions
ls -la worker.ts  # Unix
dir worker.ts     # Windows
```

### Error: "Connection refused" or "ECONNREFUSED"

**Causes**:
- Child process not started
- Port not bound
- Wrong port number

**Solutions**:
```typescript
// Check if child process is running
console.log("Process PID:", worker.process?.pid);

// Verify port
console.log("Port:", worker.port);

// Check logs for errors
// Look for "FATAL" messages in child output
```

### Error: "EADDRINUSE" (Address already in use)

**Causes**:
- Another process is using the same port
- Previous process didn't shut down cleanly

**Solutions**:
```typescript
// Find and kill the process using the port
// Unix
lsof -i :PORT

# Windows
netstat -ano | findstr :PORT
taskkill /PID <PID> /F

// Or use a different port (automatic in spawn mode)
const worker = ParentWorker.spawn("./worker.ts", "node");
// Port is automatically chosen
```

### Error: "Service not found"

**Causes**:
- Service not registered
- Registry file corrupted
- Service crashed before registering

**Solutions**:
```typescript
// Check registry file
import { ServiceRegistry } from "./src/multifrost.js";

const registryPath = require("path").join(require("os").homedir(), ".multifrost/services.json");
console.log("Registry path:", registryPath);

// Check if service exists
const port = await ServiceRegistry.discover("my-service", 5000);
console.log("Service port:", port);
```

### Error: "Maximum call stack size exceeded"

**Causes**:
- Recursive methods without base case
- Infinite loops in remote methods

**Solutions**:
```typescript
class MyWorker extends ChildWorker {
    // Good: Has base case
    async factorial(n: number): Promise<number> {
        if (n <= 1) return 1;
        return n * this.factorial(n - 1);
    }

    // Bad: No base case
    async infiniteLoop(): Promise<void> {
        await this.infiniteLoop();  // Stack overflow!
    }
}
```

### Error: "Promise rejected after timeout"

**Causes**:
- Child process crashed
- Method takes too long
- Network issues

**Solutions**:
```typescript
// Increase timeout
const worker = ParentWorker.spawn("./worker.ts", "node", {
    defaultTimeout: 30000,  // 30 seconds
});

// Use Promise.race for timeout
const result = await Promise.race([
    worker.call.longRunningTask(),
    new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error("Timeout")), 5000)
    ),
]);
```

### Error: "Circuit breaker open"

**Causes**:
- Too many consecutive failures
- Child process not responding

**Solutions**:
```typescript
// Wait for circuit to reset
await new Promise(resolve => setTimeout(resolve, 10000));

// Or manually restart
await worker.stop();
await worker.start();

// Or disable circuit breaker temporarily
// (Requires code changes to reset manually)
```

---

## Best Practices

### How should I structure my worker class?

```typescript
// Good: Well-organized worker
class DataProcessorWorker extends ChildWorker {
    // Configuration
    private readonly batchSize: number = 100;

    // Public API
    async processData(items: unknown[]): Promise<number[]> {
        return items.map(item => this.processItem(item));
    }

    async processItem(item: unknown): Promise<number> {
        // Implementation
        return 0;
    }

    // Helper methods (not exposed to parent)
    private validateItem(item: unknown): boolean {
        return typeof item === "number";
    }

    private calculateStats(data: number[]): number[] {
        // Implementation
        return [0, 0, 0];
    }
}
```

### How should I handle errors in remote methods?

```typescript
class MyWorker extends ChildWorker {
    async divide(a: number, b: number): Promise<number> {
        // Validate inputs
        if (typeof a !== "number" || typeof b !== "number") {
            throw new Error("Both arguments must be numbers");
        }

        if (b === 0) {
            throw new Error("Cannot divide by zero");
        }

        // Perform operation
        return a / b;
    }

    async processData(data: unknown): Promise<unknown> {
        try {
            // Validate data
            if (!data) {
                throw new Error("Data is required");
            }

            // Process
            return this.transform(data);

        } catch (error) {
            // Log error
            console.error("Processing failed:", error);

            // Re-throw with context
            throw new Error(`Failed to process data: ${error.message}`);
        }
    }
}
```

### How should I manage lifecycle events?

```typescript
class MyWorker extends ChildWorker {
    async start(): Promise<void> {
        console.log("Worker starting...");

        try {
            await this.setupZmq();
            await this.messageLoop();
        } catch (error) {
            console.error("Fatal error:", error);
            process.exit(1);
        }
    }

    stop(): void {
        console.log("Worker stopping...");
        this.running = false;

        // Cleanup
        if (this.socket) {
            this.socket.close();
        }

        // Unregister service
        if (this.serviceId) {
            ServiceRegistry.unregister(this.serviceId).catch(err => {
                console.error("Failed to unregister:", err);
            });
        }
    }
}
```

### How should I configure timeouts?

```typescript
const worker = ParentWorker.spawn("./worker.ts", "node", {
    // Global default timeout
    defaultTimeout: 5000,

    // Per-call timeout (overrides default)
    const result = await worker.call
        .withOptions({ timeout: 10000 })
        .longRunningTask();

    // Heartbeat configuration
    heartbeatInterval: 5.0,    // Check every 5 seconds
    heartbeatTimeout: 3.0,     // Fail after 3 seconds of silence
    heartbeatMaxMisses: 3,     // Trip circuit after 3 misses
});
```

### How should I test my implementation?

```typescript
// test/worker.test.ts
import { ParentWorker } from "../src/multifrost.js";

describe("MyWorker", () => {
    let worker: ParentWorker;

    beforeEach(async () => {
        worker = ParentWorker.spawn("./worker.ts", "node", {
            defaultTimeout: 1000,
        });
        await worker.start();
    });

    afterEach(async () => {
        await worker.stop();
    });

    it("should add numbers correctly", async () => {
        const result = await worker.call.add(2, 3);
        expect(result).toBe(5);
    });

    it("should handle errors", async () => {
        await expect(worker.call.divide(1, 0))
            .rejects.toThrow("Cannot divide by zero");
    });

    it("should timeout on slow operations", async () => {
        await expect(worker.call.sleep(2000))
            .rejects.toThrow("timed out");
    });
});
```

### How should I handle large data transfers?

```typescript
class DataWorker extends ChildWorker {
    // Good: Stream large data
    async processLargeDataset(data: unknown[]): Promise<unknown[]> {
        const results: unknown[] = [];
        const batchSize = 100;

        for (let i = 0; i < data.length; i += batchSize) {
            const batch = data.slice(i, i + batchSize);
            const batchResult = await this.processBatch(batch);
            results.push(...batchResult);
        }

        return results;
    }

    // Good: Use transferables
    async processBinary(data: Buffer): Promise<Buffer> {
        // Process in place
        for (let i = 0; i < data.length; i++) {
            data[i] = data[i] * 2;
        }
        return data;
    }
}
```

---

## Cross-Language Communication

### How do JavaScript and Python communicate?

JavaScript uses Node.js with async/await; Python uses asyncio. They communicate via ZeroMQ DEALER/ROUTER sockets with msgpack serialization.

```typescript
// JavaScript parent
const worker = ParentWorker.spawn("./python_worker.py", "python");
await worker.start();
const result = await worker.call.add(1, 2);  // → Python: 3
```

```python
# Python child
class MathWorker(ChildWorker):
    def add(self, a, b):
        return a + b

MathWorker().run()
```

### What data types are supported across languages?

| JavaScript Type | Python Type | Go Type | Rust Type | Notes |
|-----------------|-------------|---------|-----------|-------|
| `number` | `float` | `float64` | `f64` | Clamped to 2^53 |
| `string` | `str` | `string` | `String` | UTF-8 |
| `boolean` | `bool` | `bool` | `bool` | |
| `Array` | `list` | `[]` | `Vec` | |
| `object` | `dict` | `map` | `HashMap` | Keys must be strings |
| `null` | `None` | `nil` | `None` | |
| `undefined` | omitted | omitted | omitted | Not transmitted |

### What are the known limitations?

1. **BigInt**: JavaScript `BigInt` → Python `float` (precision loss)
2. **NaN/Infinity**: Converted to `null` in msgpack
3. **Functions/Classes**: Cannot be serialized
4. **Cyclic references**: Not supported
5. **Date objects**: Convert to timestamps first

```typescript
// Good: Convert complex types
class DateWorker extends ChildWorker {
    async formatDate(date: Date): Promise<string> {
        return date.toISOString();
    }
}

// Bad: Don't send functions
class BadWorker extends ChildWorker {
    async sendFunction(fn: () => void): Promise<void> {
        // This won't work - functions can't be serialized
    }
}
```

### How do I handle type mismatches?

Always validate inputs before sending:

```typescript
class DataWorker extends ChildWorker {
    async processData(data: unknown): Promise<number> {
        // Validate type
        if (typeof data !== "object" || data === null) {
            throw new Error("Data must be an object");
        }

        const num = (data as Record<string, unknown>).value;

        if (typeof num !== "number") {
            throw new Error("Data.value must be a number");
        }

        return num * 2;
    }
}
```

### Can I use JavaScript workers from Python?

Yes! Python can call JavaScript workers:

```python
# Python parent
import asyncio

async def main():
    worker = await ParentWorker.spawn("./js_worker.js", "node")
    result = await worker.acall.factorial(10)  # → JavaScript: 3628800
    print(f"factorial(10) = {result}")

asyncio.run(main())
```

```typescript
// JavaScript child
class MathWorker extends ChildWorker {
    async factorial(n: number): Promise<number> {
        if (n <= 1) return 1;
        return n * this.factorial(n - 1);
    }
}

new MathWorker().run();
```

### How do error messages propagate?

JavaScript sends only the error message string; Python includes the full traceback:

```typescript
// JavaScript child throws
throw new Error("Division by zero");
// Sent to parent as: { type: "ERROR", error: "Division by zero" }

// Python child throws
raise ValueError("Division by zero")
# Sent to parent as: { type: "ERROR", error: "Division by zero\nTraceback..." }
```

**Handling in JavaScript**:
```typescript
try {
    const result = await worker.call.divide(1, 0);
} catch (error) {
    // error.message contains just "Division by zero"
    console.error("Remote error:", error.message);
}
```

### How do I handle namespaces across languages?

Namespaces work the same way across languages:

```typescript
// JavaScript
const result = await worker
    .withOptions({ namespace: "math" })
    .add(1, 2);

// Python
result = await worker.acall.add(1, 2, namespace="math")

// Go
result, err := worker.Call("add", []interface{}{1, 2}, "math")
```

### What's the recommended architecture for multi-language systems?

```typescript
// JavaScript microservice
class AnalyticsService extends ChildWorker {
    async processData(data: unknown[]): Promise<number[]> {
        // Process data
        return data.map(item => this.calculate(item));
    }
}

new AnalyticsService("analytics-service").run();
```

```typescript
// JavaScript client
const analytics = await ParentWorker.connect("analytics-service");
const results = await analytics.call.processData(rawData);
```

```python
# Python worker
class DataProcessorWorker(ChildWorker):
    async aggregate(results: list) -> dict:
        # Combine results from multiple services
        return {"total": sum(results), "count": len(results)}

DataProcessorWorker().run()
```

---

## Additional Resources

### Documentation
- [Architecture Overview](arch.md) - Detailed implementation details
- [Root Architecture](../docs/arch.md) - Language-agnostic specification
- [API Reference](../README.md) - Installation and usage

### Community
- GitHub Issues: Report bugs or request features
- Discussions: Ask questions and share ideas

### Related Projects
- [zeromq](https://github.com/zeromq/zeromq.js) - ZeroMQ bindings for Node.js
- [msgpackr](https://github.com/mrshu/node-msgpackr) - msgpack serializer for JavaScript

---

**Version**: 1.0.0
**TypeScript Version**: ^5.0.0
**Last Updated**: 2026-02-12
