import * as zmq from "zeromq";
import { spawn, ChildProcess } from "child_process";
import { randomUUID } from "crypto";
import * as net from "net";
import * as msgpack from "msgpackr";
import { ServiceRegistry } from "./service_registry.js";

const APP_NAME = "comlink_ipc_v3";

/**
 * Sanitize values for msgpack serialization to ensure cross-language interop safety.
 * Converts NaN/Infinity to null and ensures integer values are within safe range.
 */
function sanitizeForMsgpack(value: any): any {
    if (typeof value === "number") {
        if (isNaN(value) || !isFinite(value)) return null;
        // Clamp to safe integer range (2^53) for interop
        if (Math.abs(value) > 2 ** 53 && !Number.isInteger(value)) {
            return Math.round(value);
        }
    }
    return value;
}

/**
 * Deeply sanitize all values in an object/array structure for msgpack serialization.
 */
function deepSanitize(obj: any): any {
    if (obj === null || obj === undefined) return obj;
    if (Array.isArray(obj)) return obj.map(item => deepSanitize(item));
    if (typeof obj === "object") {
        const result: any = {};
        for (const key in obj) {
            if (obj.hasOwnProperty(key)) {
                result[key] = deepSanitize(obj[key]);
            }
        }
        return result;
    }
    return sanitizeForMsgpack(obj);
}

export enum MessageType {
    CALL = "call",
    RESPONSE = "response",
    ERROR = "error",
    STDOUT = "stdout",
    STDERR = "stderr",
    HEARTBEAT = "heartbeat",
    SHUTDOWN = "shutdown",
}

export interface ComlinkMessageData {
    app: string;
    id: string;
    type: string;
    timestamp: number;
    function?: string;
    args?: any[];
    namespace?: string;
    result?: any;
    error?: string;
    output?: string;
    client_name?: string;
}

export class ComlinkMessage {
    public app: string = APP_NAME;
    public id: string;
    public type: string;
    public timestamp: number;
    public function?: string;
    public args?: any[];
    public namespace?: string;
    public result?: any;
    public error?: string;
    public output?: string;
    public client_name?: string;

    constructor(data: Partial<ComlinkMessageData> = {}) {
        this.id = data.id || randomUUID();
        this.type = data.type || "";
        this.timestamp = data.timestamp || Date.now() / 1000;

        Object.assign(this, data);
    }

    static createCall(
        functionName: string,
        args: any[] = [],
        namespace: string = "default",
        msgId?: string,
        clientName?: string,
    ): ComlinkMessage {
        return new ComlinkMessage({
            type: MessageType.CALL,
            id: msgId || randomUUID(),
            function: functionName,
            args,
            namespace,
            client_name: clientName,
        });
    }

    static createResponse(result: any, msgId: string): ComlinkMessage {
        return new ComlinkMessage({
            type: MessageType.RESPONSE,
            id: msgId,
            result,
        });
    }

    static createError(error: string, msgId: string): ComlinkMessage {
        return new ComlinkMessage({
            type: MessageType.ERROR,
            id: msgId,
            error,
        });
    }

    static createOutput(output: string, msgType: MessageType): ComlinkMessage {
        return new ComlinkMessage({
            type: msgType,
            output,
        });
    }

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
        if (this.client_name !== undefined) result.client_name = this.client_name;

        return result;
    }

pack(): Buffer {
        const sanitized = deepSanitize(this.toDict());
        return msgpack.encode(sanitized, { useBigInt64: true });
    }

static unpack(data: Buffer): ComlinkMessage {
        try {
            const decoded = msgpack.decode(data, { useBigInt64: true });
            return new ComlinkMessage(decoded);
        } catch (error) {
            throw new Error(`Failed to unpack message: ${error}`);
        }
    }
}

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

interface PendingRequest {
    resolve: (value: any) => void;
    reject: (error: Error) => void;
    timeout?: NodeJS.Timeout;
}

interface ParentWorkerConfig {
    scriptPath?: string;
    executable?: string;
    serviceId?: string;
    port: number;
    autoRestart?: boolean;
    maxRestartAttempts?: number;
    defaultTimeout?: number;
    heartbeatInterval?: number;
    heartbeatTimeout?: number;
    heartbeatMaxMisses?: number;
}

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
    public restartCount: number = 0;

    // Circuit breaker state
    private _consecutiveFailures: number = 0;
    private _circuitOpen: boolean = false;

    // Heartbeat configuration
    public readonly heartbeatInterval: number;
    public readonly heartbeatTimeout: number;
    public readonly heartbeatMaxMisses: number;

    // Heartbeat state
    private _pendingHeartbeats: Map<string, { resolve: (value: boolean) => void; reject: (error: Error) => void }> = new Map();
    private _consecutiveHeartbeatMisses: number = 0;
    private _lastHeartbeatRttMs?: number;
    private _heartbeatLoopPromise?: Promise<void>;
    private _heartbeatRunning: boolean = false;

    private socket?: zmq.Dealer;
    private process?: ChildProcess;
    private running: boolean = false;
    private readonly pendingRequests: Map<string, PendingRequest> = new Map();

    // Mutex for socket send operations
    private _sendLock: Promise<void> = Promise.resolve();

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

    /** Check if the worker is healthy (circuit breaker not tripped). */
    get isHealthy(): boolean {
        return !this._circuitOpen && this.running;
    }

    /** Check if circuit breaker is open. */
    get circuitOpen(): boolean {
        return this._circuitOpen;
    }

    /** Record a failure for circuit breaker tracking. */
    private _recordFailure(): void {
        this._consecutiveFailures++;
        if (this._consecutiveFailures >= this.maxRestartAttempts) {
            this._circuitOpen = true;
        }
    }

    /** Record a success, resetting circuit breaker. */
    private _recordSuccess(): void {
        if (this._consecutiveFailures > 0) {
            this._consecutiveFailures = 0;
            if (this._circuitOpen) {
                this._circuitOpen = false;
            }
        }
    }

    /** Get the last heartbeat round-trip time in milliseconds. */
    get lastHeartbeatRttMs(): number | undefined {
        return this._lastHeartbeatRttMs;
    }

    /**
     * Create a ParentWorker in spawn mode (owns the child process).
     */
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

    /**
     * Create a ParentWorker in connect mode (connects to existing service).
     */
    static async connect(serviceId: string, timeout: number = 5000): Promise<ParentWorker> {
        const port = await ServiceRegistry.discover(serviceId, timeout);
        return new ParentWorker({ serviceId, port });
    }

    private static findFreePort(): number {
        const server = net.createServer();
        server.listen(0);
        const address = server.address();
        const port = address && typeof address === "object" ? address.port : 5555;
        server.close();
        return port;
    }

    async start(): Promise<void> {
        // Setup ZeroMQ DEALER socket
        this.socket = new zmq.Dealer();

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
    }

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

    private async startMessageLoop(): Promise<void> {
        if (!this.socket) return;

        // DEALER socket receives: [empty_frame, message_data]
        for await (const [empty, message] of this.socket) {
            if (!this.running) break;

            try {
                const comlinkMessage = ComlinkMessage.unpack(message as Buffer);
                await this.handleMessage(comlinkMessage);
            } catch (error) {
                console.error("Failed to process message:", error);
            }

            // Check child process health (spawn mode only)
            if (this.isSpawnMode && this.process && this.process.exitCode !== null) {
                await this._handleChildExit();
                break;
            }
        }
    }

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
            // Handle heartbeat response
            const heartbeat = this._pendingHeartbeats.get(message.id);
            if (heartbeat) {
                this._pendingHeartbeats.delete(message.id);

                // Calculate RTT from timestamp in args if present
                if (message.args && message.args.length > 0) {
                    const sentTime = message.args[0] as number;
                    this._lastHeartbeatRttMs = (Date.now() / 1000 - sentTime) * 1000;
                }

                // Reset consecutive misses on successful response
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

    async callFunction(
        functionName: string,
        args: any[] = [],
        timeout?: number,
        namespace: string = "default",
        clientName?: string,
    ): Promise<any> {
        // Check circuit breaker
        if (this._circuitOpen) {
            throw new CircuitOpenError(
                `Circuit breaker open after ${this._consecutiveFailures} consecutive failures`
            );
        }

        if (!this.running || !this.socket) {
            throw new Error("Worker is not running");
        }

        // Use default timeout if not specified
        const effectiveTimeout = timeout ?? this.defaultTimeout;

        const requestId = randomUUID();
        const message = ComlinkMessage.createCall(functionName, args, namespace, requestId, clientName);

        return new Promise((resolve, reject) => {
            let timeoutHandle: NodeJS.Timeout | undefined;

            if (effectiveTimeout) {
                timeoutHandle = setTimeout(() => {
                    this.pendingRequests.delete(requestId);
                    this._recordFailure();
                    reject(new Error(`Function '${functionName}' timed out after ${effectiveTimeout}ms`));
                }, effectiveTimeout);
            }

            this.pendingRequests.set(requestId, {
                resolve: (value: any) => {
                    this._recordSuccess();
                    resolve(value);
                },
                reject: (error: Error) => {
                    this._recordFailure();
                    reject(error);
                },
                timeout: timeoutHandle,
            });

            // Send message with DEALER envelope: [empty_frame, message_data]
            this._sendMessage(message).catch(error => {
                this.pendingRequests.delete(requestId);
                if (timeoutHandle) clearTimeout(timeoutHandle);
                this._recordFailure();
                reject(new Error(`Failed to send request: ${error}`));
            });
        });
    }

    /** Send a message with retry logic and mutex for socket safety. */
    private async _sendMessage(message: ComlinkMessage, retries: number = 5): Promise<void> {
        // Simple mutex: wait for previous send to complete, then chain our operation
        const previousLock = this._sendLock;

        let releaseLock: () => void;
        this._sendLock = new Promise<void>(resolve => {
            releaseLock = resolve;
        });

        // Wait for previous operation to complete
        await previousLock;

        try {
            for (let attempt = 0; attempt < retries; attempt++) {
                try {
                    // DEALER socket sends with empty delimiter frame
                    await this.socket!.send([Buffer.alloc(0), message.pack()]);
                    return; // Success
                } catch (error: any) {
                    // Retry on socket busy (EAGAIN or "busy" errors)
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
            releaseLock!();
        }
    }

    /** Periodically send heartbeats to child process. */
    private async _heartbeatLoop(): Promise<void> {
        // Wait for initial connection
        await new Promise(resolve => setTimeout(resolve, 1000));

        while (this.running && this._heartbeatRunning) {
            try {
                // Only send heartbeats in spawn mode (we own the child)
                if (!this.isSpawnMode) {
                    await new Promise(resolve => setTimeout(resolve, this.heartbeatInterval * 1000));
                    continue;
                }

                // Check if child is still running
                if (this.process && this.process.exitCode !== null) {
                    // Child died, let message loop handle it
                    break;
                }

                // Create heartbeat message with timestamp
                const heartbeatId = randomUUID();
                const heartbeat = new ComlinkMessage({
                    type: MessageType.HEARTBEAT,
                    id: heartbeatId,
                    args: [Date.now() / 1000], // Send timestamp for RTT calculation
                });

                // Create promise for response
                const heartbeatPromise = new Promise<boolean>((resolve, reject) => {
                    this._pendingHeartbeats.set(heartbeatId, { resolve, reject });
                });

                // Send heartbeat
                await this._sendMessage(heartbeat);

                // Wait for response with timeout
                try {
                    await Promise.race([
                        heartbeatPromise,
                        new Promise<never>((_, reject) =>
                            setTimeout(() => reject(new Error("Heartbeat timeout")), this.heartbeatTimeout * 1000)
                        ),
                    ]);
                    // Success - RTT already recorded in handleMessage
                } catch {
                    // Heartbeat timed out
                    this._consecutiveHeartbeatMisses++;
                    this._pendingHeartbeats.delete(heartbeatId);

                    console.warn(
                        `Heartbeat missed (${this._consecutiveHeartbeatMisses}/${this.heartbeatMaxMisses})`
                    );

                    // Check if too many misses
                    if (this._consecutiveHeartbeatMisses >= this.heartbeatMaxMisses) {
                        console.error(
                            `Heartbeat timeout after ${this._consecutiveHeartbeatMisses} consecutive misses`
                        );
                        // Treat as failure - trip circuit breaker
                        this._recordFailure();
                        break;
                    }
                }

                // Wait for next interval
                await new Promise(resolve => setTimeout(resolve, this.heartbeatInterval * 1000));

            } catch (error) {
                if (this.running) {
                    console.error(`Error in heartbeat loop: ${error}`);
                }
                await new Promise(resolve => setTimeout(resolve, this.heartbeatInterval * 1000));
            }
        }
    }

    /** Handle child process exit. */
    private async _handleChildExit(): Promise<void> {
        const exitCode = this.process?.exitCode ?? -1;
        console.log(`Child process exited with code ${exitCode}`);

        // Record failure for circuit breaker
        this._recordFailure();

        // Notify all pending requests
        for (const [id, pending] of this.pendingRequests) {
            if (pending.timeout) clearTimeout(pending.timeout);
            pending.reject(new RemoteCallError("Child process terminated unexpectedly"));
        }
        this.pendingRequests.clear();

        // Handle restart
        if (this.autoRestart && this.restartCount < this.maxRestartAttempts) {
            await this._attemptRestart();
        } else {
            this.running = false;
        }
    }

    /** Attempt to restart the child process. */
    private async _attemptRestart(): Promise<void> {
        try {
            this.restartCount++;
            console.log(`Restarting worker (attempt ${this.restartCount}/${this.maxRestartAttempts})`);

            await new Promise(resolve => setTimeout(resolve, 1000));

            // Start new process
            const env = { ...process.env, COMLINK_ZMQ_PORT: this.port.toString() };
            this.process = spawn(this.executable, [this.scriptPath!], { env, shell: true });

            // Quick health check
            await new Promise(resolve => setTimeout(resolve, 500));
            if (this.process.exitCode !== null) {
                throw new Error(`Restart failed (exit code: ${this.process.exitCode})`);
            }

            console.log("Worker restarted successfully");
            this.restartCount = 0;

            // Restart heartbeat loop if it was running
            if (this.heartbeatInterval > 0 && !this._heartbeatRunning) {
                this._heartbeatRunning = true;
                this._heartbeatLoopPromise = this._heartbeatLoop();
            }

        } catch (error) {
            console.error(`Auto-restart failed: ${error}`);
            if (this.restartCount >= this.maxRestartAttempts) {
                console.warn("Max restart attempts reached");
            }
            this.running = false;
        }
    }

    async stop(): Promise<void> {
        this.running = false;
        this._heartbeatRunning = false;

        // Cancel all pending heartbeats
        for (const [id, hb] of this._pendingHeartbeats) {
            hb.reject(new Error("Worker shutting down"));
        }
        this._pendingHeartbeats.clear();

        // Wait for heartbeat loop to finish
        if (this._heartbeatLoopPromise) {
            try {
                await this._heartbeatLoopPromise;
            } catch {
                // Ignore errors during shutdown
            }
        }

        // Cancel all pending requests
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
}

class AsyncRemoteProxy {
    private _controller: ParentWorker;
    private _pendingOptions: { timeout?: number; namespace?: string } = {};

    constructor(controller: ParentWorker) {
        this._controller = controller;

        // Return a Proxy that intercepts all property access
        return new Proxy(this, {
            get(target, prop: string) {
                if (prop === "withOptions") {
                    return (options: { timeout?: number; namespace?: string }) => {
                        target._pendingOptions = options;
                        return target; // Return the same proxy for chaining
                    };
                }

                // For any other property, return a remote method function
                return (...args: any[]) => {
                    // Use pending options then clear them
                    const timeout = target._pendingOptions.timeout;
                    const namespace = target._pendingOptions.namespace || "default";
                    target._pendingOptions = {}; // Reset after use

                    return target._controller.callFunction(prop, args, timeout, namespace);
                };
            },
        });
    }
}

export abstract class ChildWorker {
    protected namespace: string = "default";
    protected serviceId?: string;
    private running: boolean = true;
    private socket?: zmq.Router;
    private port?: number;
    private _lastSenderId?: Buffer;

    constructor(serviceId?: string) {
        this.serviceId = serviceId;
        // ZMQ setup is deferred to start()
    }

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
            console.error(`DEBUG: Connected to tcp://localhost:${this.port}`);

        } else if (this.serviceId) {
            // CONNECT MODE: Register service, bind to port
            try {
                this.port = await ServiceRegistry.register(this.serviceId);
                console.error(`Service '${this.serviceId}' ready on port ${this.port}`);
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

        // Redirect stdout/stderr (simplified - just log to parent)
        this.redirectOutput();
    }

    private redirectOutput(): void {
        const originalConsoleLog = console.log;
        const originalConsoleError = console.error;

        console.log = (...args: any[]) => {
            const output = args.map(a => typeof a === "object" ? JSON.stringify(a) : String(a)).join(" ");
            this._sendOutput(output, MessageType.STDOUT).catch(() => {
                // Fallback to local log if send fails
                originalConsoleLog(...args);
            });
        };

        console.error = (...args: any[]) => {
            const output = args.map(a => typeof a === "object" ? JSON.stringify(a) : String(a)).join(" ");
            this._sendOutput(output, MessageType.STDERR).catch(() => {
                // Fallback to local log if send fails
                originalConsoleError(...args);
            });
        };
    }

    /** Send output message to parent. */
    private async _sendOutput(output: string, msgType: MessageType, retries: number = 2): Promise<void> {
        if (!this.socket || !this._lastSenderId) {
            return; // No parent connected yet
        }

        const message = ComlinkMessage.createOutput(output, msgType);

        for (let attempt = 0; attempt < retries; attempt++) {
            try {
                await this.socket.send([this._lastSenderId, Buffer.alloc(0), message.pack()]);
                return;
            } catch (error: any) {
                if (attempt < retries - 1) {
                    await new Promise(resolve => setTimeout(resolve, 1));
                    continue;
                }
                console.warn(`Warning: Failed to send output: ${error}`);
            }
        }
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

    private async messageLoop(): Promise<void> {
        if (!this.socket) return;

        // ROUTER socket receives: [sender_id, empty_frame, message_data]
        for await (const [senderId, empty, messageData] of this.socket) {
            if (!this.running) break;

            // Track sender for output forwarding
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
                    // Respond to heartbeat
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

    private async handleFunctionCall(message: ComlinkMessage, senderId: Buffer): Promise<void> {
        let response: ComlinkMessage;

        try {
            if (!message.function || !message.id) {
                throw new Error("Message missing 'function' or 'id' field");
            }

            const args = message.args || [];
            const func = (this as any)[message.function];

            if (typeof func !== "function") {
                throw new Error(`Function '${message.function}' not found or not callable`);
            }

            if (message.function.startsWith("_")) {
                throw new Error(`Cannot call private method '${message.function}'`);
            }

            const result = await func.apply(this, args);
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
            console.error(`CRITICAL: Failed to send response: ${error}`);
        }
    }

    private async handleHeartbeat(message: ComlinkMessage, senderId: Buffer): Promise<void> {
        // Echo back the heartbeat with the same ID and args (for RTT calculation)
        const response = new ComlinkMessage({
            type: MessageType.HEARTBEAT,
            id: message.id,
            args: message.args, // Preserve timestamp for RTT calculation
        });

        try {
            if (this.socket) {
                await this.socket.send([senderId, Buffer.alloc(0), response.pack()]);
            }
        } catch (error) {
            console.error(`CRITICAL: Failed to send heartbeat response: ${error}`);
        }
    }

    listFunctions(): string[] {
        const excluded = new Set(Object.getOwnPropertyNames(ChildWorker.prototype));
        return Object.getOwnPropertyNames(Object.getPrototypeOf(this)).filter(
            name => typeof (this as any)[name] === "function" && !name.startsWith("_") && !excluded.has(name),
        );
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
}
