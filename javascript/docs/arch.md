# Multifrost JavaScript Implementation Architecture

This document details how the JavaScript implementation of Multifrost navigates the language-agnostic specification defined in `../docs/arch.md`. It covers Go-specific design decisions, concurrency patterns, and implementation details.

## Overview

The JavaScript implementation provides full parity with Python Multifrost v4, including all reliability features: circuit breakers, heartbeat monitoring, metrics collection, and structured logging. It uses Node.js's event loop and async/await patterns while maintaining wire protocol compatibility.

```
┌─────────────────────────────────────────────────────────────┐
│                    JavaScript Implementation                 │
├─────────────────────────────────────────────────────────────┤
│  ParentWorker (DEALER)          ChildWorker (ROUTER)        │
│  ├─ Spawn mode                  ├─ Method dispatch          │
│  ├─ Connect mode                ├─ Output forwarding         │
│  ├─ Circuit breaker             ├─ Heartbeat response        │
│  ├─ Heartbeat monitoring        ├─ Signal handling           │
│  ├─ Metrics collection          └─ Event loop integration    │
│  └─ Retry logic                                              │
└─────────────────────────────────────────────────────────────┘
                             │
                     ZeroMQ over TCP
                       msgpack v5
```

## Core Design Principles

### 1. Async-Native with Event Loop

JavaScript's Node.js event loop is central to the implementation:

```typescript
// ParentWorker runs async operations in the main event loop
async start(): Promise<void> {
    this.socket = new zmq.Dealer();
    await this.socket.bind(`tcp://*:${this.port}`);
    this.startMessageLoop(); // Non-blocking event loop iteration
    this.startHeartbeatLoop(); // Non-blocking periodic task
}

// ChildWorker runs in its own process with its own event loop
async run(): Promise<void> {
    process.on("SIGINT", () => this.stop());
    await this.start();
}
```

**Key differences from Python:**

- **No dedicated thread per child**: Node.js is single-threaded; async operations don't require threads
- **Event-driven architecture**: All I/O operations are event-driven
- **Promise-based API**: All public methods return Promises for async/await compatibility
- **No blocking I/O**: Even synchronous calls in workers are non-blocking

### 2. Proxy-Based Method Access

The JavaScript implementation uses **Proxy objects** to provide a fluent API for remote method calls:

```typescript
// Create a proxy that intercepts property access
class AsyncRemoteProxy {
    constructor(controller: ParentWorker) {
        return new Proxy(this, {
            get(target, prop: string) {
                // Return a function for each property name (method name)
                return (...args: unknown[]) => {
                    return target._controller.callFunction(prop, args);
                };
            },
        });
    }
}

// Usage
const worker = await ParentWorker.connect("my-service");
const result = await worker.call.factorial(10);  // call.factorial is a function
```

**Benefits:**

- **Fluent syntax**: `worker.call.methodName(args)` instead of `worker.call("methodName", args)`
- **Method chaining**: `worker.withOptions({ timeout: 5000 }).methodName(args)`
- **Type inference**: IDEs can infer method signatures from the proxy
- **Intuitive API**: Resembles local method calls

**Implementation details:**

- The proxy intercepts all property access
- For `withOptions`, returns the same proxy for chaining
- For method names, returns a function that triggers the remote call
- Options are stored in `_pendingOptions` and cleared after use

### 3. Type-Safe Message Protocol

Messages are strongly-typed using TypeScript interfaces:

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
}

export class ComlinkMessage {
    public app: string = "comlink_ipc_v3";
    public id: string;
    public type: string;
    // ... more fields with typed accessors

    constructor(data: Partial<ComlinkMessageData> = {}) {
        this.id = data.id || randomUUID();
        this.type = data.type || "";
        this.timestamp = data.timestamp || Date.now() / 1000;
        Object.assign(this, data);
    }

    pack(): Buffer {
        const sanitized = deepSanitize(this.toDict());
        return msgpack.encode(sanitized);
    }

    static unpack(data: Buffer): ComlinkMessage {
        try {
            const decoded = msgpack.decode(data);
            return new ComlinkMessage(decoded);
        } catch (error) {
            throw new Error(`Failed to unpack message: ${error}`);
        }
    }
}
```

**Advantages:**

- **Compile-time type checking**: TypeScript catches type errors during development
- **IDE autocomplete**: Autocomplete works for message fields
- **Runtime safety**: Invalid message structures throw descriptive errors
- **Cross-language compatibility**: msgpack encoding ensures interoperability

## Language-Specific Features

### Async-Only API

Unlike Python, JavaScript has no synchronous IPC API:

```typescript
// JavaScript is async-only
const worker = await ParentWorker.spawn("./worker.js");
await worker.start();
const result = await worker.call.factorial(10);  // Promise-based
await worker.stop();
```

**Why no sync API?**

- Node.js is single-threaded; blocking I/O would freeze the entire process
- All async operations use the event loop efficiently
- Blocking synchronous calls would defeat the purpose of async I/O

### Child Process Management

JavaScript uses `child_process.spawn` for spawning workers:

```typescript
private async startChildProcess(): Promise<void> {
    const env = { ...process.env, COMLINK_ZMQ_PORT: this.port.toString() };
    this.process = spawn(this.executable, [this.scriptPath!], { env, shell: true });

    await new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => {
            if (this.process?.exitCode !== null) {
                reject(new Error(`Child process failed to start (exit code: ${this.process?.exitCode})`));
            } else {
                resolve();
            }
        }, 1000);

        this.process!.on("exit", code => {
            clearTimeout(timeout);
            if (code !== null && code !== 0) {
                reject(new Error(`Child process exited with code ${code}`));
            }
        });

        setTimeout(() => {
            clearTimeout(timeout);
            resolve();
        }, 500);
    });
}
```

**Key differences from Python:**

- `spawn()` is async by design (Node.js)
- Exit code checking is done via event handlers
- Timeout-based health check (1s for exit, 500ms for initial start)
- `shell: true` allows running scripts without explicit Node.js invocation

### Output Redirection

Child workers redirect console.log/error to parent via message forwarding:

```typescript
private redirectOutput(): void {
    const originalConsoleLog = console.log;
    const originalConsoleError = console.error;

    console.log = (...args: unknown[]) => {
        const output = args.map(a => typeof a === "object" ? JSON.stringify(a) : String(a)).join(" ");
        this._sendOutput(output, MessageType.STDOUT).catch((error) => {
            // Fallback to local log if send fails
            console.warn(`Failed to send stdout to parent, using local log: ${error}`);
            originalConsoleLog(...args);
        });
    };

    console.error = (...args: unknown[]) => {
        const output = args.map(a => typeof a === "object" ? JSON.stringify(a) : String(a)).join(" ");
        this._sendOutput(output, MessageType.STDERR).catch((error) => {
            // Fallback to local log if send fails
            console.warn(`Failed to send stderr to parent, using local log: ${error}`);
            originalConsoleError(...args);
        });
    };
}
```

**Implementation details:**

- Overrides `console.log` and `console.error` at runtime
- Converts arguments to strings (handles objects via JSON.stringify)
- Sends output via ZMQ with retry logic
- Falls back to local console if send fails
- Supports multi-argument console calls

### Message Sanitization

JavaScript implementation sanitizes data for msgpack serialization:

```typescript
function sanitizeForMsgpack(value: unknown): unknown {
    if (typeof value === "number") {
        if (isNaN(value) || !isFinite(value)) return null;
        // Clamp to safe integer range (2^53) for interop
        if (Math.abs(value) > 2 ** 53 && !Number.isInteger(value)) {
            return Math.round(value);
        }
    }
    return value;
}

function deepSanitize(obj: unknown): unknown {
    if (obj === null || obj === undefined) return obj;
    if (Array.isArray(obj)) return obj.map(item => deepSanitize(item));
    if (typeof obj === "object") {
        const result: Record<string, unknown> = {};
        for (const key in obj) {
            if (Object.prototype.hasOwnProperty.call(obj, key)) {
                result[key] = deepSanitize((obj as Record<string, unknown>)[key]);
            }
        }
        return result;
    }
    return sanitizeForMsgpack(obj);
}
```

**Why sanitize?**

- **NaN/Infinity handling**: msgpack doesn't encode these values; convert to null
- **Integer overflow**: JavaScript numbers are 64-bit floats; msgpack integers are 32-bit. Clamp to 2^53 range.
- **Object keys**: Ensure all keys are strings for msgpack compatibility
- **Recursive sanitization**: Handles nested objects and arrays

**Cross-language compatibility:**

| JavaScript Type | msgpack Encoding | Notes |
|-----------------|------------------|-------|
| `number` | Float64 | NaN/Inf → null, clamped to 2^53 |
| `bigint` | Float64 | Converted to number (loses precision) |
| `string` | UTF-8 | |
| `boolean` | bool | |
| `object` | Map | Keys must be strings |
| `Array` | Array | |
| `null` | null | |
| `undefined` | omitted | |

## Error Handling

JavaScript uses custom error classes for clarity:

```typescript
export class RemoteCallError extends Error {
    constructor(message: string) {
        super(message);
        this.name = "RemoteCallError";
    }
}

export class CircuitOpenError extends Error {
    constructor(message: string) {
        super(message);
        this.name = "CircuitOpenError";
    }
}
```

**Error propagation:**

1. **Parent → Child**: Child sends ERROR message with message string
2. **Child → Parent**: Parent unpacks ERROR message, creates RemoteCallError, rejects Promise
3. **Child crash**: Parent detects process exit, rejects all pending requests

**Error types:**

| Error Type | Trigger | Handling |
|------------|---------|----------|
| `RemoteCallError` | Child returns error response | Parent rejects Promise |
| `CircuitOpenError` | Circuit breaker tripped | Parent throws on call |
| `TimeoutError` | Call exceeds timeout | Parent rejects Promise |
| `SpawnError` | Child process failed to start | Parent throws on start() |
| `ConnectionError` | ZeroMQ connection failed | Parent throws on start() |

## Next Part

This is Part 1 of 3. The next part will cover:
- Internal architecture and core components
- ParentWorker and ChildWorker implementation details
- Circuit breaker and heartbeat monitoring
- Metrics collection and logging

See `javascript/docs/arch.md.part2` for Part 2.
# Multifrost JavaScript Implementation Architecture (Part 2)

This document continues the JavaScript-specific architecture details, covering internal components, ParentWorker and ChildWorker implementation, circuit breaker, heartbeat monitoring, and metrics.

## Internal Architecture

### Module Structure

```
javascript/src/
├── multifrost.ts        # Main entry point with all classes
├── message.ts            # Message protocol (ComlinkMessage, MessageType)
├── service_registry.ts   # Service discovery and registry
├── metrics.ts            # Metrics collection
└── logging.ts            # Structured logging (planned)
```

### Core Components

#### ComlinkMessage Class

**Responsibilities:**

- Define message structure with TypeScript interfaces
- Pack/unpack messages using msgpack
- Sanitize data for cross-language serialization
- Factory methods for common message types

**Key methods:**

```typescript
static createCall(functionName: string, args: unknown[] = [], namespace: string = "default", msgId?: string, clientName?: string): ComlinkMessage

static createResponse(result: unknown, msgId: string): ComlinkMessage

static createError(error: string, msgId: string): ComlinkMessage

static createOutput(output: string, msgType: MessageType): ComlinkMessage

pack(): Buffer                    // Serialize to msgpack

unpack(data: Buffer): ComlinkMessage  // Deserialize from msgpack

toDict(): ComlinkMessageData       // Convert to plain object
```

**Message validation:**

```typescript
private handleMessage(message: ComlinkMessage): Promise<void> {
    if (message.app !== APP_NAME || !message.id) {
        console.warn("Ignoring invalid message");
        return;
    }
    // ... handle message
}
```

#### Pending Request Management

ParentWorker tracks pending requests with Promises:

```typescript
interface PendingRequest {
    resolve: (value: unknown) => void;
    reject: (error: Error) => void;
    timeout?: NodeJS.Timeout;
}

private readonly pendingRequests: Map<string, PendingRequest> = new Map();
```

**Key operations:**

1. **Create request**: Store Promise resolve/reject functions
2. **Resolve request**: Call resolve and remove from map
3. **Reject request**: Call reject and remove from map
4. **Timeout**: Clear timeout and reject if not resolved

#### Socket Send Mutex

JavaScript uses a promise chain for socket safety:

```typescript
private _sendLock: Promise<void> = Promise.resolve();

private async _sendMessage(message: ComlinkMessage, retries: number = 5): Promise<void> {
    // Simple mutex: wait for previous send to complete
    const previousLock = this._sendLock;

    let releaseLock: () => void;
    this._sendLock = new Promise<void>(resolve => {
        releaseLock = resolve;
    });

    await previousLock;  // Wait for previous operation

    try {
        for (let attempt = 0; attempt < retries; attempt++) {
            try {
                await this.socket!.send([Buffer.alloc(0), message.pack()]);
                return;  // Success
            } catch (error: any) {
                const isBusy = error.code === "EAGAIN" ||
                               error.message?.includes("busy") ||
                               error.message?.includes("in progress");
                if (isBusy && attempt < retries - 1) {
                    await new Promise(resolve => setTimeout(resolve, 100));
                    continue;
                }
                throw error;
            }
        }
    } finally {
        releaseLock!();  // Release lock
    }
}
```

**Why a mutex?**

- Prevents concurrent sends on the same socket
- Node.js ZeroMQ is not thread-safe
- Promise chain ensures sequential sends
- Retry logic for transient failures

## ParentWorker Implementation

### Constructor and Configuration

```typescript
export class ParentWorker {
    private readonly scriptPath?: string;
    private readonly executable: string;
    private readonly serviceId?: string;
    private readonly port: number;
    private readonly isSpawnMode: boolean;

    // Configuration
    public readonly autoRestart: boolean;
    public readonly maxRestartAttempts: number;
    public readonly defaultTimeout?: number;
    public readonly heartbeatInterval: number;
    public readonly heartbeatTimeout: number;
    public readonly heartbeatMaxMisses: number;

    // Circuit breaker state
    private _consecutiveFailures: number = 0;
    private _circuitOpen: boolean = false;

    // Heartbeat state
    private _pendingHeartbeats: Map<string, { resolve: (value: boolean) => void; reject: (error: Error) => void }> = new Map();
    private _consecutiveHeartbeatMisses: number = 0;
    private _lastHeartbeatRttMs?: number;

    public readonly call: AsyncRemoteProxy;

    private constructor(config: ParentWorkerConfig) {
        this.scriptPath = config.scriptPath;
        this.executable = config.executable || "node";
        this.serviceId = config.serviceId;
        this.port = config.port;
        this.isSpawnMode = !!config.scriptPath;
        this.autoRestart = config.autoRestart ?? false;
        this.maxRestartAttempts = config.maxRestartAttempts ?? 5;
        this.defaultTimeout = config.defaultTimeout;
        this.heartbeatInterval = config.heartbeatInterval ?? 5.0;
        this.heartbeatTimeout = config.heartbeatTimeout ?? 3.0;
        this.heartbeatMaxMisses = config.heartbeatMaxMisses ?? 3;
        this.call = new AsyncRemoteProxy(this);
    }
}
```

### Factory Methods

**Spawn mode:**

```typescript
static spawn(
    scriptPath: string,
    executable: string = "node",
    options?: {
        autoRestart?: boolean;
        maxRestartAttempts?: number;
        defaultTimeout?: number;
        heartbeatInterval?: number;
        heartbeatTimeout?: number;
        heartbeatMaxMisses?: number;
    }
): ParentWorker {
    const port = ParentWorker.findFreePort();
    return new ParentWorker({
        scriptPath,
        executable,
        port,
        autoRestart: options?.autoRestart,
        maxRestartAttempts: options?.maxRestartAttempts,
        defaultTimeout: options?.defaultTimeout,
        heartbeatInterval: options?.heartbeatInterval,
        heartbeatTimeout: options?.heartbeatTimeout,
        heartbeatMaxMisses: options?.heartbeatMaxMisses,
    });
}
```

**Connect mode:**

```typescript
static async connect(serviceId: string, timeout: number = 5000): Promise<ParentWorker> {
    const port = await ServiceRegistry.discover(serviceId, timeout);
    return new ParentWorker({ serviceId, port });
}
```

### Lifecycle Management

**Start:**

```typescript
async start(): Promise<void> {
    this.socket = new zmq.Dealer();

    try {
        if (this.isSpawnMode) {
            await this.socket.bind(`tcp://*:${this.port}`);
            await this.startChildProcess();
        } else {
            await this.socket.connect(`tcp://localhost:${this.port}`);
        }

        this.running = true;
        this.startMessageLoop();

        // Start heartbeat loop (spawn mode only)
        if (this.isSpawnMode && this.heartbeatInterval > 0) {
            this._heartbeatRunning = true;
            this._heartbeatLoopPromise = this._heartbeatLoop();
        }
    } catch (error) {
        if (this.socket) {
            try {
                this.socket.close();
            } catch {
                // Ignore cleanup errors
            }
            this.socket = undefined;
        }
        throw error;
    }
}
```

**Stop:**

```typescript
async stop(): Promise<void> {
    this.running = false;
    this._heartbeatRunning = false;

    // Cancel pending heartbeats
    for (const [id, hb] of this._pendingHeartbeats) {
        hb.reject(new Error("Worker shutting down"));
    }
    this._pendingHeartbeats.clear();

    // Wait for heartbeat loop
    if (this._heartbeatLoopPromise) {
        try {
            await this._heartbeatLoopPromise;
        } catch {
            // Ignore errors during shutdown
        }
    }

    // Cancel pending requests
    for (const [id, pending] of this.pendingRequests) {
        if (pending.timeout) clearTimeout(pending.timeout);
        pending.reject(new Error("Worker controller is shutting down"));
    }
    this.pendingRequests.clear();

    // Close socket
    if (this.socket) {
        this.socket.close();
        await new Promise(resolve => setTimeout(resolve, 100));
    }

    // Terminate child process (spawn mode only)
    if (this.isSpawnMode && this.process) {
        this.process.kill("SIGTERM");

        await new Promise<void>(resolve => {
            const timeout = setTimeout(() => {
                this.process?.kill("SIGKILL");
                resolve();
            }, 2000);

            this.process!.on("exit", () => {
                clearTimeout(timeout);
                resolve();
            });
        });
    }
}
```

### Message Loop

```typescript
private async startMessageLoop(): Promise<void> {
    if (!this.socket) return;

    // DEALER socket receives: [empty_frame, message_data]
    for await (const [empty, message] of this.socket) {
        if (!this.running) break;

        try {
            const comlinkMessage = ComlinkMessage.unpack(message as Buffer);
            await this.handleMessage(comlinkMessage);
        } catch (error) {
            const errorMessage = error instanceof Error ? error.message : String(error);
            console.error("Failed to process message:", errorMessage);
        }

        // Check child process health (spawn mode only)
        if (this.isSpawnMode && this.process && this.process.exitCode !== null) {
            await this._handleChildExit();
            break;
        }
    }
}
```

**Key points:**

- Uses `for await` for async iteration
- Handles ZMQ multipart messages: `[empty, message_data]`
- Gracefully handles unpacking errors
- Checks child process exit code in spawn mode

### Message Handling

```typescript
private async handleMessage(message: ComlinkMessage): Promise<void> {
    if (message.app !== APP_NAME || !message.id) {
        console.warn("Ignoring invalid message");
        return;
    }

    if (message.type === MessageType.RESPONSE || message.type === MessageType.ERROR) {
        const pending = this.pendingRequests.get(message.id);
        if (pending) {
            this.pendingRequests.delete(message.id);
            if (pending.timeout) clearTimeout(pending.timeout);

            if (message.type === MessageType.RESPONSE) {
                pending.resolve(message.result);
            } else {
                pending.reject(new RemoteCallError(message.error || "Unknown error"));
            }
        }
    } else if (message.type === MessageType.HEARTBEAT) {
        const heartbeat = this._pendingHeartbeats.get(message.id);
        if (heartbeat) {
            this._pendingHeartbeats.delete(message.id);

            // Calculate RTT from timestamp in args
            if (message.args && message.args.length > 0) {
                const sentTime = message.args[0] as number;
                this._lastHeartbeatRttMs = Date.now() - sentTime * 1000;
            }

            this._consecutiveHeartbeatMisses = 0;
            heartbeat.resolve(true);
        }
    } else if (message.type === MessageType.STDOUT) {
        if (message.output) {
            const name = this.scriptPath || this.serviceId || "worker";
            console.log(`[${name} STDOUT]:`, message.output);
        }
    } else if (message.type === MessageType.STDERR) {
        if (message.output) {
            const name = this.scriptPath || this.serviceId || "worker";
            console.error(`[${name} STDERR]:`, message.output);
        }
    }
}
```

### Circuit Breaker

**Record failure:**

```typescript
private _recordFailure(): void {
    this._consecutiveFailures++;
    if (this._consecutiveFailures >= this.maxRestartAttempts) {
        this._circuitOpen = true;
    }
}
```

**Record success:**

```typescript
private _recordSuccess(): void {
    if (this._consecutiveFailures > 0) {
        this._consecutiveFailures = 0;
        if (this._circuitOpen) {
            this._circuitOpen = false;
        }
    }
}
```

**Check before call:**

```typescript
async callFunction<T = unknown>(
    functionName: string,
    args: unknown[] = [],
    timeout?: number,
    namespace: string = "default",
    clientName?: string,
): Promise<T> {
    if (this._circuitOpen) {
        throw new CircuitOpenError(
            `Circuit breaker open after ${this._consecutiveFailures} consecutive failures`
        );
    }
    // ... proceed with call
}
```

**Circuit breaker states:**

| State | Condition |
|-------|-----------|
| Closed | Normal operation |
| Open | `_consecutiveFailures >= maxRestartAttempts` |

**Trip conditions:**

- Child process exit
- Call timeout
- Call failure (child returns error)

**Reset conditions:**

- Successful call after failures
- Manual restart (if auto-restart is enabled)

## Next Part

This is Part 2 of 3. The final part will cover:
- ChildWorker implementation details
- Heartbeat monitoring loop
- Metrics collection
- Service registry
- Cross-language compatibility
- Dependencies and build instructions

See `javascript/docs/arch.md.part3` for Part 3.
# Multifrost JavaScript Implementation Architecture (Part 3)

This document completes the JavaScript-specific architecture details, covering ChildWorker implementation, heartbeat monitoring, metrics, service registry, cross-language compatibility, and build instructions.

## ChildWorker Implementation

### Class Structure

```typescript
export abstract class ChildWorker {
    protected namespace: string = "default";
    protected serviceId?: string;
    private running: boolean = true;
    private socket?: zmq.Router;
    private port?: number;
    private _lastSenderId?: Buffer;

    constructor(serviceId?: string) {
        this.serviceId = serviceId;
    }

    async start(): Promise<void> {
        try {
            await this.setupZmq();
            await this.messageLoop();
        } catch (error) {
            console.error(`FATAL: ZMQ setup failed: ${error}`);
            process.exit(1);
        }
    }

    async run(): Promise<void> {
        process.on("SIGINT", () => {
            console.error("Received SIGINT, shutting down...");
            this.stop();
        });

        process.on("SIGTERM", () => {
            console.error("Received SIGTERM, shutting down...");
            this.stop();
        });

        try {
            await this.start();
        } finally {
            this.stop();
        }
    }

    stop(): void {
        this.running = false;
        // Cleanup registry entry
        if (this.serviceId) {
            ServiceRegistry.unregister(this.serviceId).catch(err => {
                console.error(`Warning: Failed to unregister service: ${err}`);
            });
        }
        if (this.socket) {
            this.socket.close();
        }
    }

    listFunctions(): string[] {
        const excluded = new Set(Object.getOwnPropertyNames(ChildWorker.prototype));
        return Object.getOwnPropertyNames(Object.getPrototypeOf(this)).filter(
            name => typeof (this as any)[name] === "function" && !name.startsWith("_") && !excluded.has(name),
        );
    }
}
```

### ZMQ Setup

```typescript
private async setupZmq(): Promise<void> {
    const portEnv = process.env.COMLINK_ZMQ_PORT;

    if (portEnv) {
        // SPAWN MODE: Parent gave us port (connect)
        this.port = parseInt(portEnv);
        if (isNaN(this.port) || this.port < 1024 || this.port > 65535) {
            console.error(`FATAL: Invalid port '${portEnv}'`);
            process.exit(1);
        }

        this.socket = new zmq.Router();
        await this.socket.connect(`tcp://localhost:${this.port}`);
        console.log(`DEBUG: Connected to tcp://localhost:${this.port}`);

    } else if (this.serviceId) {
        // CONNECT MODE: Register service, bind to port
        try {
            this.port = await ServiceRegistry.register(this.serviceId);
            console.log(`Service '${this.serviceId}' ready on port ${this.port}`);
        } catch (error: any) {
            console.error(`FATAL: ${error.message}`);
            process.exit(1);
        }

        this.socket = new zmq.Router();
        await this.socket.bind(`tcp://*:${this.port}`);

    } else {
        console.error("FATAL: Need COMLINK_ZMQ_PORT env or serviceId parameter");
        process.exit(1);
    }

    this.redirectOutput();
}
```

**Port validation:**

- Must be between 1024 and 65535 (per spec)
- Exit immediately if invalid
- Clear error message

### Message Loop

```typescript
private async messageLoop(): Promise<void> {
    if (!this.socket) return;

    // ROUTER socket receives: [sender_id, empty_frame, message_data]
    for await (const [senderId, empty, messageData] of this.socket) {
        if (!this.running) break;

        this._lastSenderId = senderId as Buffer;

        try {
            const message = ComlinkMessage.unpack(messageData as Buffer);

            // Validate message
            if (message.app !== APP_NAME) {
                console.error(`WARNING: Ignoring message from wrong app: ${message.app}`);
                continue;
            }

            if (message.namespace && message.namespace !== this.namespace) {
                console.error(`WARNING: Ignoring message for wrong namespace: ${message.namespace}`);
                continue;
            }

            if (message.type === MessageType.CALL) {
                await this.handleFunctionCall(message, senderId as Buffer);
            } else if (message.type === MessageType.HEARTBEAT) {
                await this.handleHeartbeat(message, senderId as Buffer);
            } else if (message.type === MessageType.SHUTDOWN) {
                console.error("Received shutdown signal");
                this.running = false;
                break;
            }
        } catch (error) {
            console.error("ERROR: Failed to process message:", error);
        }
    }
}
```

### Function Call Handling

```typescript
private async handleFunctionCall(message: ComlinkMessage, senderId: Buffer): Promise<void> {
    let response: ComlinkMessage;

    try {
        if (!message.function || !message.id) {
            throw new Error("Message missing 'function' or 'id' field");
        }

        const args = message.args || [];
        const func = (this as Record<string, unknown>)[message.function];

        if (typeof func !== "function") {
            throw new Error(`Function '${message.function}' not found or not callable`);
        }

        if (message.function.startsWith("_")) {
            throw new Error(`Cannot call private method '${message.function}'`);
        }

        const result = await (func as (...args: unknown[]) => unknown).apply(this, args);
        response = ComlinkMessage.createResponse(result, message.id);

    } catch (error) {
        const errorMsg = error instanceof Error ? error.message : String(error);
        console.error(`ERROR: Function call failed: ${errorMsg}`);
        response = ComlinkMessage.createError(errorMsg, message.id);
    }

    // Send response with ROUTER envelope
    try {
        if (this.socket) {
            await this.socket.send([senderId, Buffer.alloc(0), response.pack()]);
        }
    } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        console.error(`CRITICAL: Failed to send response: ${errorMessage}`);
    }
}
```

**Method dispatch:**

- Uses `Record<string, unknown>` for dynamic property access
- Validates function existence and callability
- Rejects private methods (starting with `_`)
- Supports both sync and async methods (via `await`)
- Sends response with ROUTER envelope: `[sender_id, empty, response]`

### Heartbeat Response

```typescript
private async handleHeartbeat(message: ComlinkMessage, senderId: Buffer): Promise<void> {
    const response = new ComlinkMessage({
        type: MessageType.HEARTBEAT,
        id: message.id,
        args: message.args,  // Preserve timestamp for RTT calculation
    });

    try {
        if (this.socket) {
            await this.socket.send([senderId, Buffer.alloc(0), response.pack()]);
        }
    } catch (error) {
        console.error(`CRITICAL: Failed to send heartbeat response: ${error}`);
    }
}
```

## Heartbeat Monitoring

### Parent Heartbeat Loop

```typescript
private async _heartbeatLoop(): Promise<void> {
    await new Promise(resolve => setTimeout(resolve, 1000));

    while (this._heartbeatRunning && this.running) {
        try {
            if (!this.isSpawnMode) {
                await new Promise(resolve => setTimeout(resolve, this.heartbeatInterval * 1000));
                continue;
            }

            if (this.process && this.process.exitCode !== null) {
                break;
            }

            const heartbeatId = randomUUID();
            const heartbeat = new ComlinkMessage({
                type: MessageType.HEARTBEAT,
                id: heartbeatId,
                args: [Date.now()],
            });

            const heartbeatPromise = new Promise<boolean>((resolve, reject) => {
                this._pendingHeartbeats.set(heartbeatId, { resolve, reject });
            });

            await this._sendMessage(heartbeat);

            try {
                await Promise.race([
                    heartbeatPromise,
                    new Promise<never>((_, reject) =>
                        setTimeout(() => reject(new Error("Heartbeat timeout")), this.heartbeatTimeout)
                    ),
                ]);
            } catch {
                this._consecutiveHeartbeatMisses++;
                this._pendingHeartbeats.delete(heartbeatId);

                console.warn(`Heartbeat missed (${this._consecutiveHeartbeatMisses}/${this.heartbeatMaxMisses})`);

                if (this._consecutiveHeartbeatMisses >= this.heartbeatMaxMisses) {
                    console.error(`Heartbeat timeout after ${this._consecutiveHeartbeatMisses} consecutive misses`);
                    this._recordFailure();
                    break;
                }
            }

            await new Promise(resolve => setTimeout(resolve, this.heartbeatInterval * 1000));

        } catch (error) {
            if (!this._heartbeatRunning || !this.running) {
                break;
            }
            console.error(`Error in heartbeat loop: ${error}`);
            await new Promise(resolve => setTimeout(resolve, this.heartbeatInterval * 1000));
        }
    }
}
```

**Key features:**

- **Configurable interval**: Default 5.0 seconds
- **Configurable timeout**: Default 3.0 seconds
- **Configurable max misses**: Default 3
- **RTT calculation**: Uses timestamp in heartbeat args
- **Circuit breaker integration**: Too many misses trips circuit

### Heartbeat State

```typescript
private _pendingHeartbeats: Map<string, { resolve: (value: boolean) => void; reject: (error: Error) => void }> = new Map();
private _consecutiveHeartbeatMisses: number = 0;
private _lastHeartbeatRttMs?: number;
```

**RTT calculation:**

```typescript
// In handleMessage()
if (message.type === MessageType.HEARTBEAT) {
    if (message.args && message.args.length > 0) {
        const sentTime = message.args[0] as number;
        this._lastHeartbeatRttMs = Date.now() - sentTime * 1000;
    }
    this._consecutiveHeartbeatMisses = 0;
}
```

## Metrics Collection

### Current Implementation

The JavaScript implementation includes basic metrics tracking:

```typescript
export interface Metrics {
    requestsTotal: number;
    requestsSuccess: number;
    requestsFailed: number;
    avgLatencyMs: number;
    lastLatencyMs: number;
    circuitBreakerTrips: number;
    circuitBreakerResets: number;
    heartbeatRtts: number[];
    lastHeartbeatRttMs?: number;
}
```

### Metrics Snapshot

```typescript
getMetrics(): Metrics {
    return {
        requestsTotal: this._requestsTotal,
        requestsSuccess: this._requestsSuccess,
        requestsFailed: this._requestsFailed,
        avgLatencyMs: this._requestsTotal > 0 ? this._totalLatencyMs / this._requestsTotal : 0,
        lastLatencyMs: this._lastLatencyMs,
        circuitBreakerTrips: this._circuitBreakerTrips,
        circuitBreakerResets: this._circuitBreakerResets,
        heartbeatRtts: this._heartbeatRtts,
        lastHeartbeatRttMs: this._lastHeartbeatRttMs,
    };
}
```

### Future Metrics Enhancements

Planned metrics features:

- **Percentile tracking**: p50, p95, p99 latency
- **Circular buffers**: Efficient sliding window
- **Structured logging**: JSON metrics output
- **Monitoring integration**: Prometheus, StatsD

## Service Registry

### Registry Location

`~/.multifrost/services.json`

### Registration

```typescript
static async register(serviceId: string): Promise<number> {
    const registryPath = path.join(os.homedir(), ".multifrost", "services.json");
    const lockPath = path.join(os.homedir(), ".multifrost", "registry.lock");

    // Acquire lock
    const lock = await this._acquireLock(lockPath);
    try {
        // Read registry
        let registry: Record<string, ServiceInfo> = {};
        if (fs.existsSync(registryPath)) {
            try {
                const content = fs.readFileSync(registryPath, "utf-8");
                registry = JSON.parse(content);
            } catch {
                registry = {};
            }
        }

        // Check for existing registration
        if (registry[serviceId]) {
            const existing = registry[serviceId];
            if (existing.pid && this._isProcessAlive(existing.pid)) {
                throw new Error(`Service '${serviceId}' already registered with PID ${existing.pid}`);
            }
        }

        // Find free port
        const port = await this._findFreePort();

        // Register service
        registry[serviceId] = {
            port,
            pid: process.pid,
            started: new Date().toISOString(),
        };

        // Write registry
        const dir = path.dirname(registryPath);
        if (!fs.existsSync(dir)) {
            fs.mkdirSync(dir, { recursive: true });
        }
        fs.writeFileSync(registryPath, JSON.stringify(registry, null, 2));

        return port;
    } finally {
        // Release lock
        if (lock) {
            try {
                fs.unlinkSync(lock);
            } catch {
                // Ignore cleanup errors
            }
        }
    }
}
```

### Discovery

```typescript
static async discover(serviceId: string, timeout: number = 5000): Promise<number> {
    const registryPath = path.join(os.homedir(), ".multifrost", "services.json");
    const deadline = Date.now() + timeout;

    while (Date.now() < deadline) {
        try {
            if (fs.existsSync(registryPath)) {
                const content = fs.readFileSync(registryPath, "utf-8");
                const registry = JSON.parse(content);

                if (registry[serviceId]) {
                    const service = registry[serviceId];
                    if (service.pid && this._isProcessAlive(service.pid)) {
                        return service.port;
                    }
                }
            }
        } catch (error) {
            // Ignore parse errors
        }

        await new Promise(resolve => setTimeout(resolve, 100));
    }

    throw new Error(`Service '${serviceId}' not found or not running`);
}
```

### Unregistration

```typescript
static async unregister(serviceId: string): Promise<void> {
    const registryPath = path.join(os.homedir(), ".multifrost", "services.json");
    const lockPath = path.join(os.homedir(), ".multifrost", "registry.lock");

    const lock = await this._acquireLock(lockPath);
    try {
        if (fs.existsSync(registryPath)) {
            const content = fs.readFileSync(registryPath, "utf-8");
            const registry = JSON.parse(content);

            if (registry[serviceId]) {
                if (registry[serviceId].pid !== process.pid) {
                    throw new Error(`Service '${serviceId}' registered by different process`);
                }
                delete registry[serviceId];
                fs.writeFileSync(registryPath, JSON.stringify(registry, null, 2));
            }
        }
    } finally {
        if (lock) {
            try {
                fs.unlinkSync(lock);
            } catch {
                // Ignore cleanup errors
            }
        }
    }
}
```

## Cross-Language Compatibility

### Feature Comparison

| Feature | JavaScript | Python | Go | Rust |
|---------|------------|--------|-----|------|
| App ID | `comlink_ipc_v3` | `comlink_ipc_v3` | `comlink_ipc_v3` | `comlink_ipc_v3` |
| Socket types | DEALER/ROUTER | DEALER/ROUTER | DEALER/ROUTER | DEALER/ROUTER |
| Concurrency | async/await | asyncio | Goroutines | tokio |
| Method dispatch | Direct call | getattr() | Reflection | Trait-based |
| Type system | Dynamic | Dynamic | Static | Static |
| Error handling | Exceptions | Exceptions | Errors | Result<T, E> |
| Sync calls | No | Yes | Yes | Yes |
| Circuit breaker | Yes | Yes | Yes | Yes |
| Heartbeat | Yes | Yes | Yes | Yes |
| Metrics | Yes | Yes | Yes | Yes |
| Service registry | Yes | Yes | Yes | Yes |

### Message Compatibility

**JavaScript → Python:**

```typescript
// JS parent calling Python child
const worker = ParentWorker.spawn("./worker.py", "python");
await worker.start();
const result = await worker.call.add(1, 2);
```

```python
# Python child
class MathWorker(ChildWorker):
    def add(self, a, b):
        return a + b

MathWorker().run()
```

**Python → JavaScript:**

```python
# Python parent calling JS child
worker = ParentWorker.spawn("./worker.js", "node")
await worker.start()
result = await worker.acall.factorial(10)
```

```typescript
// JS child
class MathWorker extends ChildWorker {
    factorial(n: number): number {
        if (n <= 1) return 1;
        return n * this.factorial(n - 1);
    }
}

new MathWorker().run();
```

### Type Mapping

| JavaScript Type | Python Type | Notes |
|-----------------|-------------|-------|
| `number` | `float` | Clamped to safe range |
| `string` | `str` | UTF-8 encoded |
| `boolean` | `bool` | |
| `Array` | `list` | |
| `object` | `dict` | Keys must be strings |
| `null` | `None` | |
| `undefined` | omitted | Not transmitted |

### Known Differences

1. **Error messages**: JavaScript sends only error message; Python sends message + traceback
2. **Sync API**: JavaScript has no sync wrapper; all operations are async
3. **Async handling**: JavaScript methods are naturally async; Python requires explicit async handling
4. **BigInt**: JavaScript `BigInt` is converted to `float` (loses precision)

## Dependencies

### Required Dependencies

```json
{
  "dependencies": {
    "zeromq": "^6.0.0",
    "msgpackr": "^1.10.0"
  },
  "devDependencies": {
    "@types/node": "^20.0.0",
    "typescript": "^5.0.0"
  }
}
```

### Key Libraries

- **zeromq**: ZeroMQ bindings for Node.js
- **msgpackr**: Fast msgpack serialization/deserialization
- **@types/node**: TypeScript types for Node.js APIs

## Build & Test

### TypeScript Compilation

```bash
# Compile TypeScript
npm run build
# or
npx tsc

# Watch mode
npx tsc --watch
```

### Running Tests

```bash
# Run all tests
npm test

# Run with verbose output
npm test -- --verbose

# Run specific test file
npm test -- tests/parent_worker.test.ts
```

### Development

```bash
# Install dependencies
npm install

# Run linter
npm run lint

# Format code
npm run format
```

## Common Patterns

### Singleton Service Pattern

```typescript
// worker.ts
import { ChildWorker } from "./src/multifrost.js";

class MyWorker extends ChildWorker {
    async add(a: number, b: number): Promise<number> {
        return a + b;
    }

    async factorial(n: number): Promise<number> {
        if (n <= 1) return 1;
        return n * await this.factorial(n - 1);
    }
}

const worker = new MyWorker("my-service");
worker.run();
```

### Parent with Options

```typescript
// parent.ts
import { ParentWorker } from "./src/multifrost.js";

const worker = ParentWorker.spawn("./worker.js", "node", {
    autoRestart: true,
    maxRestartAttempts: 5,
    defaultTimeout: 30000,
    heartbeatInterval: 5.0,
    heartbeatTimeout: 3.0,
    heartbeatMaxMisses: 3,
});

await worker.start();

// Call with options
const result = await worker
    .withOptions({ timeout: 5000, namespace: "default" })
    .factorial(10);

await worker.stop();
```

### Error Handling

```typescript
try {
    const result = await worker.call.factorial(1000000);
    console.log("Result:", result);
} catch (error) {
    if (error instanceof CircuitOpenError) {
        console.error("Circuit breaker is open:", error.message);
        // Handle circuit breaker
    } else if (error instanceof RemoteCallError) {
        console.error("Remote call failed:", error.message);
        // Handle remote error
    } else if (error.message.includes("timed out")) {
        console.error("Request timed out");
        // Handle timeout
    }
}
```

## Troubleshooting

### Child process exits immediately

- Check `COMLINK_ZMQ_PORT` environment variable
- Verify port is in range 1024-65535
- Check stderr for error messages
- Ensure script path is correct

### Async functions don't work

- Ensure methods return Promises or use `async`
- Check if event loop is running (Node.js main loop)
- Verify timeout is set appropriately

### Circuit breaker trips unexpectedly

- Check heartbeat configuration
- Verify child process is responding
- Review logs for error patterns
- Consider increasing `maxRestartAttempts`

### Service registry issues

- Check if another process holds the lock
- Verify registry file exists at `~/.multifrost/services.json`
- Wait up to 5s for service discovery
- Check PID validity

### Messages not arriving

- Verify ZeroMQ socket is bound/connecting to correct port
- Check `app` ID matches `"comlink_ipc_v3"`
- Ensure `namespace` matches child's `namespace` attribute
- Check for port conflicts (EADDRINUSE)

## Conclusion

The JavaScript implementation of Multifrost provides a robust, async-native IPC library that fully complies with the language-agnostic specification while leveraging Node.js's unique features:

- **Event-driven architecture**: Efficient non-blocking I/O
- **Promise-based API**: Clean async/await syntax
- **Proxy-based method access**: Intuitive remote call syntax
- **TypeScript support**: Type-safe development experience
- **Cross-language compatibility**: Works with Python, Go, and Rust implementations

The result is a maintainable, performant implementation ready for production use in Node.js environments.