import {
    ParentWorker,
    ChildWorker,
    ComlinkMessage,
    MessageType,
    RemoteCallError,
    CircuitOpenError,
    ParentHandle,
} from "./multifrost.js";
import { ServiceRegistry } from "./service_registry.js";
import { Metrics } from "./metrics.js";
import type { MetricsSnapshot, MetricsDict, RequestMetrics } from "./metrics.js";
import {
    StructuredLogger,
    LogLevel,
    LogEvent,
    defaultJsonHandler,
    defaultPrettyHandler,
} from "./logging.js";
import type { LogEntry, LogHandler } from "./logging.js";

export default {
    ParentWorker,
    ParentHandle,
    ChildWorker,
    ComlinkMessage,
    MessageType,
    RemoteCallError,
    CircuitOpenError,
    ServiceRegistry,
    Metrics,
    StructuredLogger,
    LogLevel,
    LogEvent,
    defaultJsonHandler,
    defaultPrettyHandler,
};

export {
    ParentWorker,
    ParentHandle,
    ChildWorker,
    ComlinkMessage,
    MessageType,
    RemoteCallError,
    CircuitOpenError,
    ServiceRegistry,
    // Metrics
    Metrics,
    // Logging
    StructuredLogger,
    LogLevel,
    LogEvent,
    defaultJsonHandler,
    defaultPrettyHandler,
};

// Type-only exports
export type { LogEntry, LogHandler, MetricsSnapshot, MetricsDict, RequestMetrics };
