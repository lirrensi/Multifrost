# Multifrost JavaScript Quick Examples

Get started with the JavaScript/TypeScript implementation of Multifrost IPC library.

## Installation

```bash
# Install dependencies
npm install zeromq msgpackr

# Install TypeScript
npm install -D typescript @types/node

# Compile TypeScript
npx tsc
```

## Quick Start: Parent-Child Example

Create a child worker with a callable method:

```typescript
// math_worker.ts
import { ChildWorker } from "./src/multifrost";

class MathWorker extends ChildWorker {
  add(a: number, b: number): number {
    return a + b;
  }

  multiply(a: number, b: number): number {
    return a * b;
  }
}

const worker = new MathWorker();
worker.run();
```

Create a parent that calls the child:

```typescript
// parent.ts
import { ParentWorker } from "./src/multifrost";

async function main() {
  // Spawn the child worker
  const worker = ParentWorker.spawn("./math_worker.ts", "ts-node");

  // Get a handle (async mode - JavaScript only has async)
  const handle = worker.handle();

  // Start the worker
  await handle.start();

  // Call methods via the handle
  const result1 = await handle.call.add(5, 3);
  console.log(`5 + 3 = ${result1}`);

  const result2 = await handle.call.multiply(4, 7);
  console.log(`4 * 7 = ${result2}`);

  // Clean up
  await handle.stop();
}

main();
```

Run the example:

```bash
# Start the child worker in one terminal
npx ts-node math_worker.ts

# In another terminal, run the parent
npx ts-node parent.ts
```

## Worker → Handle Pattern (v4)

Starting with v4, the API separates process definition (Worker) from runtime interface (Handle):

```
Worker = config/state (holds socket, process, registry internally)
Handle = lightweight API view (lifecycle + call interface)
```

### Async Handle (JavaScript only has async)

```typescript
import { ParentWorker } from "./src/multifrost";

async function main() {
  const worker = ParentWorker.spawn("./worker.ts", "ts-node");
  const handle = worker.handle();  // Async handle

  await handle.start();
  const result = await handle.call.add(1, 2);
  await handle.stop();
}
```

### Legacy API (still available)

```typescript
// OLD (v3) - still works
const worker = ParentWorker.spawn("./worker.ts", "ts-node");
await worker.start();
const result = await worker.call.add(1, 2);
await worker.stop();

// NEW (v4) - recommended
const worker = ParentWorker.spawn("./worker.ts", "ts-node");
const handle = worker.handle();
await handle.start();
const result = await handle.call.add(1, 2);
await handle.stop();
```

## With Options Chaining

Use `withOptions()` to chain method calls with custom settings:

```typescript
const worker = ParentWorker.spawn("./worker.ts", "ts-node");
const handle = worker.handle();
await handle.start();

// Call with custom timeout and namespace
const result = await handle.call
  .withOptions({
    timeout: 5000,
    namespace: "my-namespace",
  })
  .add(1, 2);

await handle.stop();
```

## Connect Mode

Register a service and connect from a parent:

```typescript
// worker.ts
import { ChildWorker } from "./src/multifrost";

class MathWorker extends ChildWorker {
  constructor() {
    super("math-service");
  }

  add(a: number, b: number): number {
    return a + b;
  }
}

const worker = new MathWorker();
worker.run();
```

```typescript
// parent.ts
import { ParentWorker } from "./src/multifrost";

async function main() {
  // Connect to the existing service
  const worker = await ParentWorker.connect("math-service", 5000);
  const handle = worker.handle();
  await handle.start();

  const result = await handle.call.add(5, 3);
  console.log(`5 + 3 = ${result}`);

  await handle.stop();
}

main();
```

## Common Patterns

### Error Handling

```typescript
async function main() {
  const worker = ParentWorker.spawn("./worker.ts", "ts-node");
  const handle = worker.handle();
  await handle.start();

  try {
    const result = await handle.call.add(1, 2);
    console.log(`Result: ${result}`);
  } catch (error) {
    if (error instanceof CircuitOpenError) {
      console.error("Circuit breaker is open:", error.message);
    } else if (error instanceof RemoteCallError) {
      console.error("Remote call failed:", error.message);
    } else {
      console.error("Unexpected error:", error);
    }
  }

  await handle.stop();
}

main();
```

### Async Methods in Child

```typescript
// worker.ts
import { ChildWorker } from "./src/multifrost";

class Worker extends ChildWorker {
  async fetchData(url: string): Promise<any> {
    const response = await fetch(url);
    return response.json();
  }

  async processData(data: any): Promise<any> {
    return { result: data.value * 2 };
  }
}

const worker = new Worker();
worker.run();
```

### Metrics Collection

```typescript
async function main() {
  const worker = ParentWorker.spawn("./worker.ts", "ts-node");
  const handle = worker.handle();
  await handle.start();

  // Get metrics from worker (introspection)
  const metrics = worker.metrics;
  console.log(`Total requests: ${metrics.requestsTotal}`);
  console.log(`Success rate: ${metrics.requestsSuccess / metrics.requestsTotal || 0}`);
  console.log(`Last latency: ${metrics.lastLatencyMs}ms`);
  console.log(`Heartbeat RTT: ${metrics.lastHeartbeatRttMs}ms`);

  await handle.stop();
}

main();
```

### Health Checks

```typescript
async function main() {
  const worker = ParentWorker.spawn("./worker.ts", "ts-node");
  const handle = worker.handle();
  await handle.start();

  // Check if worker is healthy (introspection on worker)
  console.log(`Healthy: ${worker.isHealthy}`);
  console.log(`Circuit open: ${worker.circuitOpen}`);
  console.log(`Last heartbeat RTT: ${worker.lastHeartbeatRttMs}`);

  await handle.stop();
}

main();
```

### List Available Methods

```typescript
async function main() {
  const worker = ParentWorker.spawn("./worker.ts", "ts-node");
  const handle = worker.handle();
  await handle.start();

  // List methods on the child
  const methods = handle.call.listFunctions();
  console.log(`Available methods: ${methods.join(", ")}`);

  await handle.stop();
}

main();
```

### Auto-Restart on Crash

```typescript
const worker = ParentWorker.spawn("./worker.ts", "ts-node", {
  autoRestart: true,
  maxRestartAttempts: 5,
  defaultTimeout: 30000,
  heartbeatInterval: 5.0,
  heartbeatTimeout: 3.0,
  heartbeatMaxMisses: 3,
});

const handle = worker.handle();
await handle.start();
// ... usage
await handle.stop();
```

## Key Concepts

### ParentWorker
- **Purpose**: Initiates calls and manages child lifecycle
- **Modes**: `spawn()` (creates new process) or `connect()` (connects to existing service)
- **Introspection**: `isHealthy`, `circuitOpen`, `metrics`, `lastHeartbeatRttMs`

### ParentHandle
- **Purpose**: Lightweight API view for lifecycle and calls
- **Methods**: `start()`, `stop()`, `call.*`
- **Note**: JavaScript only has async handles (no sync variant)

### ChildWorker
- **Purpose**: Exposes callable methods and handles requests
- **Methods**: Implement methods directly on the class (can be sync or async)
- **Modes**: Can register with `serviceId` for connect mode

### Proxy-Based API

The JavaScript implementation uses Proxy objects for a fluent API:

```typescript
// Instead of: worker.call("methodName", args)
// You get: handle.call.methodName(args)

const result = await handle.call.add(1, 2);  // call.add is a function
const result2 = await handle.call
  .withOptions({ timeout: 5000 })
  .multiply(3, 4);  // withOptions returns the same proxy for chaining
```

### Spawn Mode
- Parent finds free port and binds DEALER socket
- Parent spawns child with `COMLINK_ZMQ_PORT` environment variable
- Parent owns child process lifecycle
- Includes heartbeat monitoring (default: every 5s, timeout 3s)

### Connect Mode
- Child registers with service registry (`~/.multifrost/services.json`)
- Parent discovers service and connects
- Better for long-running services

### Circuit Breaker
- Tracks consecutive failures (default: 5)
- Opens circuit after threshold
- Resets on successful call
- Prevents cascading failures

### Heartbeat Monitoring
- Parent sends periodic heartbeats to child (spawn mode only)
- Calculates round-trip time (RTT)
- Trips circuit breaker on missed heartbeats (default: 3 consecutive misses)

## Cross-Language Usage

JavaScript parent calling Python child:

```typescript
// parent.ts
import { ParentWorker } from "./src/multifrost";

async function main() {
  // Spawn Python worker
  const worker = ParentWorker.spawn("./worker.py", "python");
  const handle = worker.handle();
  await handle.start();

  // Call Python method
  const result = await handle.call.factorial(10);
  console.log(`Factorial: ${result}`);

  await handle.stop();
}

main();
```

```python
# worker.py
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

## TypeScript Configuration

Create `tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "lib": ["ES2020"],
    "outDir": "./dist",
    "rootDir": "./src",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules"]
}
```

## Troubleshooting

**Child process exits immediately:**
- Check `COMLINK_ZMQ_PORT` environment variable
- Verify port is in range 1024-65535
- Check stderr for error messages
- Ensure script path is correct

**Async functions don't work:**
- Ensure methods return Promises or use `async`
- Check if event loop is running (Node.js main loop)
- Verify timeout is set appropriately (default: 30s)

**Circuit breaker trips unexpectedly:**
- Check heartbeat configuration
- Verify child process is responding
- Review logs for error patterns
- Consider increasing `maxRestartAttempts`

**Service registry issues:**
- Check if another process holds the lock
- Verify registry file exists at `~/.multifrost/services.json`
- Wait up to 5s for service discovery
- Check PID validity

**Messages not arriving:**
- Verify ZeroMQ socket is bound/connecting to correct port
- Check `app` ID matches `"comlink_ipc_v4"`
- Ensure `namespace` matches child's `namespace` attribute
- Check for port conflicts (EADDRINUSE)

## Dependencies

- `zeromq`: ZeroMQ bindings for Node.js
- `msgpackr`: Fast msgpack serialization/deserialization
- `@types/node`: TypeScript types for Node.js APIs (dev dependency)

For more details, see the [full architecture documentation](./arch.md).
