/**
 * Structured logging with correlation IDs for observability.
 *
 * Provides JSON-formatted logs with pluggable output handlers.
 */

/** Log levels for structured logging. */
export enum LogLevel {
    DEBUG = "debug",
    INFO = "info",
    WARN = "warn",
    ERROR = "error",
}

/** Standard log events for IPC operations. */
export enum LogEvent {
    // Lifecycle
    WORKER_START = "worker_start",
    WORKER_STOP = "worker_stop",
    WORKER_RESTART = "worker_restart",

    // Requests
    REQUEST_START = "request_start",
    REQUEST_END = "request_end",
    REQUEST_TIMEOUT = "request_timeout",
    REQUEST_ERROR = "request_error",

    // Circuit breaker
    CIRCUIT_OPEN = "circuit_open",
    CIRCUIT_CLOSE = "circuit_close",
    CIRCUIT_HALF_OPEN = "circuit_half_open",

    // Connection
    SOCKET_CONNECT = "socket_connect",
    SOCKET_DISCONNECT = "socket_disconnect",
    SOCKET_RECONNECT = "socket_reconnect",

    // Process
    PROCESS_SPAWN = "process_spawn",
    PROCESS_EXIT = "process_exit",

    // Heartbeat
    HEARTBEAT_SENT = "heartbeat_sent",
    HEARTBEAT_RECEIVED = "heartbeat_received",
    HEARTBEAT_MISSED = "heartbeat_missed",
    HEARTBEAT_TIMEOUT = "heartbeat_timeout",

    // Queue
    QUEUE_OVERFLOW = "queue_overflow",
}

/** Structured log entry with all context. */
export interface LogEntry {
    // Required
    event: string;
    level: string;
    message: string;
    timestamp: number;

    // Correlation
    correlationId?: string;
    requestId?: string;
    parentRequestId?: string;

    // Context
    workerId?: string;
    serviceId?: string;
    func?: string;
    namespace?: string;

    // Timing
    durationMs?: number;

    // Status
    success?: boolean;
    error?: string;
    errorType?: string;

    // Metrics snapshot (optional)
    metrics?: Record<string, unknown>;

    // Custom metadata
    metadata?: Record<string, unknown>;
}

/** Type alias for log handler function. */
export type LogHandler = (entry: LogEntry) => void;

/**
 * Structured logger with pluggable handlers.
 *
 * Usage:
 *     // Simple usage with callback
 *     const logger = new StructuredLogger({
 *         handler: (entry) => console.log(JSON.stringify(entry))
 *     });
 *
 *     // Log events
 *     logger.log({
 *         event: LogEvent.REQUEST_START,
 *         message: "Calling remote function",
 *         requestId: "abc123",
 *         func: "add",
 *     });
 *
 *     // Convenience methods
 *     logger.info(LogEvent.WORKER_START, "Worker started", { workerId: "worker-1" });
 *     logger.error(LogEvent.REQUEST_ERROR, "Call failed", { error: "Timeout" });
 */
export class StructuredLogger {
    private handler?: LogHandler;
    private level: LogLevel;
    private workerId?: string;
    private serviceId?: string;

    private readonly levelOrder: Record<LogLevel, number> = {
        [LogLevel.DEBUG]: 0,
        [LogLevel.INFO]: 1,
        [LogLevel.WARN]: 2,
        [LogLevel.ERROR]: 3,
    };

    constructor(options?: {
        handler?: LogHandler;
        level?: LogLevel;
        workerId?: string;
        serviceId?: string;
    }) {
        this.handler = options?.handler;
        this.level = options?.level !== undefined ? options.level : LogLevel.INFO;
        this.workerId = options?.workerId;
        this.serviceId = options?.serviceId;
    }

    /** Set or update the log handler. */
    setHandler(handler: LogHandler): void {
        this.handler = handler;
    }

    /** Set default context for all log entries. */
    setContext(options?: { workerId?: string; serviceId?: string }): void {
        if (options?.workerId !== undefined) {
            this.workerId = options.workerId;
        }
        if (options?.serviceId !== undefined) {
            this.serviceId = options.serviceId;
        }
    }

    /** Check if this level should be logged. */
    private shouldLog(level: LogLevel): boolean {
        return this.levelOrder[level] >= this.levelOrder[this.level];
    }

    /**
     * Log an event with structured data.
     */
    log(event: LogEvent, message: string, options?: {
        level?: LogLevel;
        correlationId?: string;
        requestId?: string;
        func?: string;
        namespace?: string;
        durationMs?: number;
        success?: boolean;
        error?: string;
        errorType?: string;
        metadata?: Record<string, unknown>;
    }): void {
        if (!this.handler || !this.shouldLog(options?.level ?? LogLevel.INFO)) {
            return;
        }

        const entry: LogEntry = {
            event: event,
            level: (options?.level ?? LogLevel.INFO).valueOf(),
            message,
            timestamp: Date.now(), // Use milliseconds for consistency
            workerId: this.workerId,
            serviceId: this.serviceId,
            correlationId: options?.correlationId,
            requestId: options?.requestId,
            func: options?.func,
            namespace: options?.namespace,
            durationMs: options?.durationMs,
            success: options?.success,
            error: options?.error,
            errorType: options?.errorType,
            metadata: options?.metadata,
        };

        try {
            this.handler(entry);
        } catch (e) {
            // Don't let logging errors break the application
            console.error(`Log handler error: ${e}`);
        }
    }

    /** Log at DEBUG level. */
    debug(event: LogEvent, message: string, options?: Omit<Parameters<typeof this.log>[2], 'level'>): void {
        this.log(event, message, { ...options, level: LogLevel.DEBUG });
    }

    /** Log at INFO level. */
    info(event: LogEvent, message: string, options?: Omit<Parameters<typeof this.log>[2], 'level'>): void {
        this.log(event, message, { ...options, level: LogLevel.INFO });
    }

    /** Log at WARN level. */
    warn(event: LogEvent, message: string, options?: Omit<Parameters<typeof this.log>[2], 'level'>): void {
        this.log(event, message, { ...options, level: LogLevel.WARN });
    }

    /** Log at ERROR level. */
    error(event: LogEvent, message: string, options?: Omit<Parameters<typeof this.log>[2], 'level'>): void {
        this.log(event, message, { ...options, level: LogLevel.ERROR });
    }

    // Convenience methods for common events

    /** Log request start. */
    requestStart(options: {
        requestId: string;
        func: string;
        namespace?: string;
        correlationId?: string;
        metadata?: Record<string, unknown>;
    }): void {
        this.debug(LogEvent.REQUEST_START, `Calling ${options.func}`, {
            requestId: options.requestId,
            func: options.func,
            namespace: options.namespace ?? "default",
            correlationId: options.correlationId,
            metadata: options.metadata,
        });
    }

    /** Log request completion. */
    requestEnd(options: {
        requestId: string;
        func: string;
        durationMs: number;
        success?: boolean;
        error?: string;
        correlationId?: string;
    }): void {
        const event = options.success !== false ? LogEvent.REQUEST_END : LogEvent.REQUEST_ERROR;
        const level = options.success !== false ? LogLevel.INFO : LogLevel.WARN;
        this.log(event, options.success !== false ? `Completed ${options.func}` : `Failed ${options.func}`, {
            level,
            requestId: options.requestId,
            func: options.func,
            durationMs: Math.round(options.durationMs * 100) / 100,
            success: options.success ?? true,
            error: options.error,
            correlationId: options.correlationId,
        });
    }

    /** Log circuit breaker opening. */
    circuitOpen(failures: number): void {
        this.warn(LogEvent.CIRCUIT_OPEN, `Circuit breaker opened after ${failures} failures`);
    }

    /** Log circuit breaker closing (recovery). */
    circuitClose(): void {
        this.info(LogEvent.CIRCUIT_CLOSE, "Circuit breaker closed (recovered)");
    }

    /** Log worker start. */
    workerStart(mode: string = "spawn"): void {
        this.info(LogEvent.WORKER_START, `Worker started in ${mode} mode`);
    }

    /** Log worker stop. */
    workerStop(reason: string = "shutdown"): void {
        this.info(LogEvent.WORKER_STOP, `Worker stopped: ${reason}`);
    }

    /** Log child process exit. */
    processExit(exitCode: number): void {
        const level = exitCode === 0 ? LogLevel.INFO : LogLevel.WARN;
        this.log(LogEvent.PROCESS_EXIT, `Child process exited with code ${exitCode}`, {
            level,
            metadata: { exitCode },
        });
    }

    /** Log heartbeat sent. */
    heartbeatSent(): void {
        this.debug(LogEvent.HEARTBEAT_SENT, "Heartbeat sent");
    }

    /** Log heartbeat response received. */
    heartbeatReceived(rttMs: number): void {
        this.debug(LogEvent.HEARTBEAT_RECEIVED, `Heartbeat received (RTT: ${rttMs.toFixed(1)}ms)`, {
            durationMs: rttMs,
        });
    }

    /** Log missed heartbeat. */
    heartbeatMissed(consecutive: number, maxAllowed: number): void {
        this.warn(LogEvent.HEARTBEAT_MISSED, `Heartbeat missed (${consecutive}/${maxAllowed})`, {
            metadata: { consecutiveMisses: consecutive, maxAllowed },
        });
    }

    /** Log heartbeat timeout (too many misses). */
    heartbeatTimeout(misses: number): void {
        this.error(LogEvent.HEARTBEAT_TIMEOUT, `Heartbeat timeout after ${misses} consecutive misses`, {
            metadata: { totalMisses: misses },
        });
    }
}

/** Default handler that prints JSON to stdout. */
export function defaultJsonHandler(entry: LogEntry): void {
    console.log(JSON.stringify(entry));
}

/** Default handler that prints human-readable output. */
export function defaultPrettyHandler(entry: LogEntry): void {
    const timestamp = new Date(entry.timestamp).toLocaleTimeString("en-US", { hour12: false });
    const level = entry.level.toUpperCase().padEnd(5);
    const prefix = `[${timestamp}] [${level}]`;

    const parts: string[] = [prefix, entry.event, entry.message];

    if (entry.requestId) {
        parts.push(`req=${entry.requestId.slice(0, 8)}`);
    }
    if (entry.func) {
        parts.push(`fn=${entry.func}`);
    }
    if (entry.durationMs !== undefined) {
        parts.push(`${entry.durationMs.toFixed(1)}ms`);
    }
    if (entry.error) {
        parts.push(`error=${entry.error}`);
    }

    console.log(parts.join(" "));
}
