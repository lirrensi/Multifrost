/**
 * Unit tests for StructuredLogger class.
 * Tests structured logging, log levels, and convenience methods.
 * 
 * Run with: npx tsx tests/logging.test.ts
 */

import {
    StructuredLogger,
    LogLevel,
    LogEvent,
    LogEntry,
    defaultJsonHandler,
    defaultPrettyHandler,
} from "../src/logging.js";

// Simple test harness
let passed = 0;
let failed = 0;

function assert(condition: boolean, message: string): void {
    if (!condition) {
        failed++;
        throw new Error(`ASSERTION FAILED: ${message}`);
    }
    passed++;
}

function describe(name: string, fn: () => void): void {
    console.log(`\n=== ${name} ===`);
    try {
        fn();
        console.log(`  PASSED`);
    } catch (e) {
        console.log(`  FAILED: ${e instanceof Error ? e.message : e}`);
    }
}

// ============================================================================
// TESTS: LogLevel Enum
// ============================================================================

describe("LogLevel enum values", () => {
    assert(LogLevel.DEBUG === "debug", "DEBUG should be 'debug'");
    assert(LogLevel.INFO === "info", "INFO should be 'info'");
    assert(LogLevel.WARN === "warn", "WARN should be 'warn'");
    assert(LogLevel.ERROR === "error", "ERROR should be 'error'");
});

// ============================================================================
// TESTS: LogEvent Enum
// ============================================================================

describe("LogEvent enum values", () => {
    // Lifecycle
    assert(LogEvent.WORKER_START === "worker_start", "WORKER_START");
    assert(LogEvent.WORKER_STOP === "worker_stop", "WORKER_STOP");
    assert(LogEvent.WORKER_RESTART === "worker_restart", "WORKER_RESTART");
    
    // Requests
    assert(LogEvent.REQUEST_START === "request_start", "REQUEST_START");
    assert(LogEvent.REQUEST_END === "request_end", "REQUEST_END");
    assert(LogEvent.REQUEST_TIMEOUT === "request_timeout", "REQUEST_TIMEOUT");
    assert(LogEvent.REQUEST_ERROR === "request_error", "REQUEST_ERROR");
    
    // Circuit breaker
    assert(LogEvent.CIRCUIT_OPEN === "circuit_open", "CIRCUIT_OPEN");
    assert(LogEvent.CIRCUIT_CLOSE === "circuit_close", "CIRCUIT_CLOSE");
    assert(LogEvent.CIRCUIT_HALF_OPEN === "circuit_half_open", "CIRCUIT_HALF_OPEN");
    
    // Connection
    assert(LogEvent.SOCKET_CONNECT === "socket_connect", "SOCKET_CONNECT");
    assert(LogEvent.SOCKET_DISCONNECT === "socket_disconnect", "SOCKET_DISCONNECT");
    
    // Heartbeat
    assert(LogEvent.HEARTBEAT_SENT === "heartbeat_sent", "HEARTBEAT_SENT");
    assert(LogEvent.HEARTBEAT_RECEIVED === "heartbeat_received", "HEARTBEAT_RECEIVED");
    assert(LogEvent.HEARTBEAT_MISSED === "heartbeat_missed", "HEARTBEAT_MISSED");
    assert(LogEvent.HEARTBEAT_TIMEOUT === "heartbeat_timeout", "HEARTBEAT_TIMEOUT");
});

// ============================================================================
// TESTS: StructuredLogger Construction
// ============================================================================

describe("StructuredLogger: Default Construction", () => {
    const logger = new StructuredLogger();
    
    // Should not throw - no handler means silent logger
    logger.log(LogEvent.WORKER_START, "Test message");
    assert(true, "Should not throw without handler");
});

describe("StructuredLogger: With Handler", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; }
    });
    
    logger.log(LogEvent.WORKER_START, "Test message");
    
    assert(capturedEntry !== null, "Should capture entry");
    assert(capturedEntry?.event === LogEvent.WORKER_START, "Should have correct event");
    assert(capturedEntry?.message === "Test message", "Should have correct message");
    assert(capturedEntry?.level === "info", "Default level should be info");
    assert(typeof capturedEntry?.timestamp === "number", "Should have timestamp");
});

describe("StructuredLogger: With Context", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
        workerId: "worker-123",
        serviceId: "my-service",
    });
    
    logger.log(LogEvent.WORKER_START, "Test");
    
    assert(capturedEntry?.workerId === "worker-123", "Should have workerId");
    assert(capturedEntry?.serviceId === "my-service", "Should have serviceId");
});

// ============================================================================
// TESTS: Log Level Filtering
// ============================================================================

describe("StructuredLogger: Log Level Filtering - Default INFO", () => {
    const captured: LogEntry[] = [];
    const logger = new StructuredLogger({
        handler: (entry) => { captured.push(entry); },
        // Default level is INFO
    });
    
    logger.debug(LogEvent.REQUEST_START, "Debug message");
    logger.info(LogEvent.WORKER_START, "Info message");
    logger.warn(LogEvent.HEARTBEAT_MISSED, "Warn message");
    logger.error(LogEvent.REQUEST_ERROR, "Error message");
    
    assert(captured.length === 3, "Should capture 3 messages (DEBUG filtered out)");
    assert(captured[0].level === "info", "First should be info");
    assert(captured[1].level === "warn", "Second should be warn");
    assert(captured[2].level === "error", "Third should be error");
});

describe("StructuredLogger: Log Level Filtering - DEBUG", () => {
    const captured: LogEntry[] = [];
    const logger = new StructuredLogger({
        handler: (entry) => { captured.push(entry); },
        level: LogLevel.DEBUG,
    });
    
    logger.debug(LogEvent.REQUEST_START, "Debug");
    logger.info(LogEvent.WORKER_START, "Info");
    logger.warn(LogEvent.HEARTBEAT_MISSED, "Warn");
    logger.error(LogEvent.REQUEST_ERROR, "Error");
    
    assert(captured.length === 4, "Should capture all 4 messages");
});

describe("StructuredLogger: Log Level Filtering - ERROR only", () => {
    const captured: LogEntry[] = [];
    const logger = new StructuredLogger({
        handler: (entry) => { captured.push(entry); },
        level: LogLevel.ERROR,
    });
    
    logger.debug(LogEvent.REQUEST_START, "Debug");
    logger.info(LogEvent.WORKER_START, "Info");
    logger.warn(LogEvent.HEARTBEAT_MISSED, "Warn");
    logger.error(LogEvent.REQUEST_ERROR, "Error");
    
    assert(captured.length === 1, "Should capture only ERROR");
    assert(captured[0].level === "error", "Should be error level");
});

// ============================================================================
// TESTS: Log Methods
// ============================================================================

describe("StructuredLogger: log() with options", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
    });
    
    logger.log(LogEvent.REQUEST_END, "Request completed", {
        requestId: "req-123",
        func: "add",
        namespace: "math",
        durationMs: 42.5,
        success: true,
        correlationId: "corr-456",
    });
    
    assert(capturedEntry?.requestId === "req-123", "Should have requestId");
    assert(capturedEntry?.func === "add", "Should have func");
    assert(capturedEntry?.namespace === "math", "Should have namespace");
    assert(capturedEntry?.durationMs === 42.5, "Should have durationMs");
    assert(capturedEntry?.success === true, "Should have success");
    assert(capturedEntry?.correlationId === "corr-456", "Should have correlationId");
});

describe("StructuredLogger: log() with error info", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
    });
    
    logger.log(LogEvent.REQUEST_ERROR, "Request failed", {
        error: "Timeout exceeded",
        errorType: "TimeoutError",
    });
    
    assert(capturedEntry?.error === "Timeout exceeded", "Should have error");
    assert(capturedEntry?.errorType === "TimeoutError", "Should have errorType");
});

describe("StructuredLogger: log() with metadata", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
    });
    
    logger.log(LogEvent.WORKER_START, "Worker started", {
        metadata: { pid: 12345, port: 8080 },
    });
    
    assert(capturedEntry?.metadata?.pid === 12345, "Should have metadata pid");
    assert(capturedEntry?.metadata?.port === 8080, "Should have metadata port");
});

// ============================================================================
// TESTS: Convenience Methods
// ============================================================================

describe("StructuredLogger: requestStart()", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
        level: LogLevel.DEBUG,
    });
    
    logger.requestStart({
        requestId: "req-789",
        func: "multiply",
        namespace: "math",
        correlationId: "corr-111",
    });
    
    assert(capturedEntry?.event === LogEvent.REQUEST_START, "Should be REQUEST_START");
    assert(capturedEntry?.requestId === "req-789", "Should have requestId");
    assert(capturedEntry?.func === "multiply", "Should have func");
    assert(capturedEntry?.namespace === "math", "Should have namespace");
    assert(capturedEntry?.level === "debug", "Should be debug level");
});

describe("StructuredLogger: requestEnd() success", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
    });
    
    logger.requestEnd({
        requestId: "req-111",
        func: "add",
        durationMs: 15.5,
        success: true,
    });
    
    assert(capturedEntry?.event === LogEvent.REQUEST_END, "Should be REQUEST_END");
    assert(capturedEntry?.success === true, "Should have success true");
    assert(capturedEntry?.level === "info", "Should be info level for success");
});

describe("StructuredLogger: requestEnd() failure", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
    });
    
    logger.requestEnd({
        requestId: "req-222",
        func: "divide",
        durationMs: 5.0,
        success: false,
        error: "Division by zero",
    });
    
    assert(capturedEntry?.event === LogEvent.REQUEST_ERROR, "Should be REQUEST_ERROR for failure");
    assert(capturedEntry?.success === false, "Should have success false");
    assert(capturedEntry?.error === "Division by zero", "Should have error");
    assert(capturedEntry?.level === "warn", "Should be warn level for failure");
});

describe("StructuredLogger: circuitOpen()", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
    });
    
    logger.circuitOpen(5);
    
    assert(capturedEntry?.event === LogEvent.CIRCUIT_OPEN, "Should be CIRCUIT_OPEN");
    assert(capturedEntry?.level === "warn", "Should be warn level");
    assert(capturedEntry?.message.includes("5"), "Should mention 5 failures");
});

describe("StructuredLogger: circuitClose()", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
    });
    
    logger.circuitClose();
    
    assert(capturedEntry?.event === LogEvent.CIRCUIT_CLOSE, "Should be CIRCUIT_CLOSE");
    assert(capturedEntry?.level === "info", "Should be info level");
});

describe("StructuredLogger: workerStart()", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
    });
    
    logger.workerStart("spawn");
    
    assert(capturedEntry?.event === LogEvent.WORKER_START, "Should be WORKER_START");
    assert(capturedEntry?.message.includes("spawn"), "Should mention spawn mode");
});

describe("StructuredLogger: workerStop()", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
    });
    
    logger.workerStop("shutdown");
    
    assert(capturedEntry?.event === LogEvent.WORKER_STOP, "Should be WORKER_STOP");
    assert(capturedEntry?.message.includes("shutdown"), "Should mention shutdown");
});

describe("StructuredLogger: processExit()", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
    });
    
    logger.processExit(0);
    assert(capturedEntry?.level === "info", "Exit code 0 should be info");
    assert(capturedEntry?.metadata?.exitCode === 0, "Should have exit code 0");
    
    logger.processExit(1);
    assert(capturedEntry?.level === "warn", "Non-zero exit should be warn");
});

describe("StructuredLogger: heartbeatSent()", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
        level: LogLevel.DEBUG,
    });
    
    logger.heartbeatSent();
    
    assert(capturedEntry?.event === LogEvent.HEARTBEAT_SENT, "Should be HEARTBEAT_SENT");
    assert(capturedEntry?.level === "debug", "Should be debug level");
});

describe("StructuredLogger: heartbeatReceived()", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
        level: LogLevel.DEBUG,
    });
    
    logger.heartbeatReceived(12.5);
    
    assert(capturedEntry?.event === LogEvent.HEARTBEAT_RECEIVED, "Should be HEARTBEAT_RECEIVED");
    assert(capturedEntry?.durationMs === 12.5, "Should have RTT duration");
});

describe("StructuredLogger: heartbeatMissed()", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
    });
    
    logger.heartbeatMissed(2, 3);
    
    assert(capturedEntry?.event === LogEvent.HEARTBEAT_MISSED, "Should be HEARTBEAT_MISSED");
    assert(capturedEntry?.level === "warn", "Should be warn level");
    assert(capturedEntry?.metadata?.consecutiveMisses === 2, "Should have consecutive misses");
    assert(capturedEntry?.metadata?.maxAllowed === 3, "Should have max allowed");
});

describe("StructuredLogger: heartbeatTimeout()", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
    });
    
    logger.heartbeatTimeout(5);
    
    assert(capturedEntry?.event === LogEvent.HEARTBEAT_TIMEOUT, "Should be HEARTBEAT_TIMEOUT");
    assert(capturedEntry?.level === "error", "Should be error level");
    assert(capturedEntry?.metadata?.totalMisses === 5, "Should have total misses");
});

// ============================================================================
// TESTS: Handler Management
// ============================================================================

describe("StructuredLogger: setHandler()", () => {
    const entries1: LogEntry[] = [];
    const entries2: LogEntry[] = [];
    
    const logger = new StructuredLogger({
        handler: (entry) => { entries1.push(entry); },
    });
    
    logger.info(LogEvent.WORKER_START, "First");
    
    logger.setHandler((entry) => { entries2.push(entry); });
    
    logger.info(LogEvent.WORKER_STOP, "Second");
    
    assert(entries1.length === 1, "First handler should have 1 entry");
    assert(entries2.length === 1, "Second handler should have 1 entry");
    assert(entries1[0].message === "First", "First entry should be 'First'");
    assert(entries2[0].message === "Second", "Second entry should be 'Second'");
});

describe("StructuredLogger: setContext()", () => {
    let capturedEntry: LogEntry | null = null;
    const logger = new StructuredLogger({
        handler: (entry) => { capturedEntry = entry; },
        workerId: "old-worker",
    });
    
    logger.info(LogEvent.WORKER_START, "Before");
    assert(capturedEntry?.workerId === "old-worker", "Should have old workerId");
    
    logger.setContext({ workerId: "new-worker", serviceId: "new-service" });
    
    logger.info(LogEvent.WORKER_START, "After");
    assert(capturedEntry?.workerId === "new-worker", "Should have new workerId");
    assert(capturedEntry?.serviceId === "new-service", "Should have new serviceId");
});

// ============================================================================
// TESTS: Default Handlers
// ============================================================================

describe("defaultJsonHandler", () => {
    let output = "";
    const originalLog = console.log;
    console.log = (msg: string) => { output = msg; };
    
    try {
        const entry: LogEntry = {
            event: LogEvent.WORKER_START,
            level: "info",
            message: "Test",
            timestamp: 1700000000000,
        };
        
        defaultJsonHandler(entry);
        
        const parsed = JSON.parse(output);
        assert(parsed.event === "worker_start", "Should have event");
        assert(parsed.message === "Test", "Should have message");
    } finally {
        console.log = originalLog;
    }
});

describe("defaultPrettyHandler", () => {
    let output = "";
    const originalLog = console.log;
    console.log = (...args: any[]) => { output = args.join(" "); };
    
    try {
        const entry: LogEntry = {
            event: LogEvent.REQUEST_END,
            level: "info",
            message: "Completed add",
            timestamp: Date.now(),
            requestId: "req-12345678",
            func: "add",
            durationMs: 42.5,
        };
        
        defaultPrettyHandler(entry);
        
        assert(output.includes("request_end"), "Should include event");
        assert(output.includes("Completed add"), "Should include message");
        assert(output.includes("req="), "Should include requestId prefix");
        assert(output.includes("fn=add"), "Should include func");
    } finally {
        console.log = originalLog;
    }
});

// ============================================================================
// TESTS: Handler Error Handling
// ============================================================================

describe("StructuredLogger: Handler throws error", () => {
    const logger = new StructuredLogger({
        handler: () => { throw new Error("Handler error"); },
    });
    
    // Should not throw - logger catches handler errors
    let threw = false;
    try {
        logger.info(LogEvent.WORKER_START, "Test");
    } catch {
        threw = true;
    }
    
    assert(!threw, "Should not throw when handler throws");
});

// ============================================================================
// SUMMARY
// ============================================================================

console.log("\n" + "=".repeat(50));
console.log(`LOGGING TEST RESULTS: ${passed} passed, ${failed} failed`);
console.log("=".repeat(50));

if (failed > 0) {
    process.exit(1);
}
