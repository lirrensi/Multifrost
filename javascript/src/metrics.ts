/**
 * Metrics collection for observability.
 *
 * Tracks request latency, error rates, queue depth, and circuit breaker events.
 */

/** Metrics for a single completed request. */
export interface RequestMetrics {
    requestId: string;
    func: string;
    namespace: string;
    success: boolean;
    latencyMs: number;
    error?: string;
    timestamp: number;
}

/** Point-in-time snapshot of all metrics. */
export interface MetricsSnapshot {
    // Counters
    requestsTotal: number;
    requestsSuccess: number;
    requestsFailed: number;

    // Latency (milliseconds)
    latencyAvgMs: number;
    latencyP50Ms: number;
    latencyP95Ms: number;
    latencyP99Ms: number;
    latencyMinMs: number;
    latencyMaxMs: number;

    // Queue
    queueDepth: number;
    queueMaxDepth: number;

    // Circuit breaker
    circuitBreakerTrips: number;
    circuitBreakerState: "closed" | "open" | "half-open";

    // Heartbeat
    heartbeatRttAvgMs: number;
    heartbeatRttLastMs: number;
    heartbeatMisses: number;

    // Time window
    windowSeconds: number;

    // Timestamp
    timestamp: number;
}

/** Metrics dictionary for JSON serialization. */
export interface MetricsDict {
    requests: {
        total: number;
        success: number;
        failed: number;
        errorRate: number;
    };
    latencyMs: {
        avg: number;
        p50: number;
        p95: number;
        p99: number;
        min: number;
        max: number;
    };
    queue: {
        depth: number;
        maxDepth: number;
    };
    circuitBreaker: {
        trips: number;
        state: string;
    };
    heartbeat: {
        rttAvgMs: number;
        rttLastMs: number;
        misses: number;
    };
}

/**
 * Thread-safe metrics collector for ParentWorker.
 *
 * Usage:
 *     const metrics = new Metrics();
 *
 *     // Start tracking a request
 *     const start = metrics.startRequest("req-123", "myFunc", "default");
 *     // ... do work ...
 *     metrics.endRequest(start, "req-123", true);
 *
 *     // Get snapshot
 *     const snapshot = metrics.snapshot();
 *     console.log(`Avg latency: ${snapshot.latencyAvgMs}ms`);
 */
export class Metrics {
    private readonly maxLatencySamples: number;
    private readonly windowSeconds: number;

    // Counters
    private _requestsTotal: number = 0;
    private _requestsSuccess: number = 0;
    private _requestsFailed: number = 0;
    private _circuitBreakerTrips: number = 0;
    private _circuitBreakerState: "closed" | "open" | "half-open" = "closed";

    // Queue tracking
    private _queueDepth: number = 0;
    private _queueMaxDepth: number = 0;

    // Latency samples (circular buffer via array slice)
    private _latencies: number[] = [];

    // Heartbeat RTT samples
    private _heartbeatRtts: number[] = [];
    private _heartbeatMisses: number = 0;

    constructor(options?: { maxLatencySamples?: number; windowSeconds?: number }) {
        this.maxLatencySamples = options?.maxLatencySamples ?? 1000;
        this.windowSeconds = options?.windowSeconds ?? 60.0;
    }

    /**
     * Start tracking a request.
     * Returns start timestamp for later endRequest() call.
     */
    startRequest(requestId: string, func: string, namespace: string = "default"): number {
        this._requestsTotal++;
        this._queueDepth++;
        this._queueMaxDepth = Math.max(this._queueMaxDepth, this._queueDepth);
        return performance.now();
    }

    /**
     * End tracking a request.
     * Returns latency in milliseconds.
     */
    endRequest(
        startTime: number,
        requestId: string,
        success: boolean = true,
        error?: string
    ): number {
        const latencyMs = performance.now() - startTime;

        this._queueDepth--;

        if (success) {
            this._requestsSuccess++;
        } else {
            this._requestsFailed++;
        }

        // Store latency sample (circular buffer behavior)
        this._latencies.push(latencyMs);
        if (this._latencies.length > this.maxLatencySamples) {
            this._latencies.shift();
        }

        return latencyMs;
    }

    /** Record a circuit breaker trip event. */
    recordCircuitBreakerTrip(): void {
        this._circuitBreakerTrips++;
        this._circuitBreakerState = "open";
    }

    /** Record circuit breaker reset (closed). */
    recordCircuitBreakerReset(): void {
        this._circuitBreakerState = "closed";
    }

    /** Record circuit breaker entering half-open state. */
    recordCircuitBreakerHalfOpen(): void {
        this._circuitBreakerState = "half-open";
    }

    /** Record a heartbeat round-trip time. */
    recordHeartbeatRtt(rttMs: number): void {
        this._heartbeatRtts.push(rttMs);
        if (this._heartbeatRtts.length > 100) {
            this._heartbeatRtts.shift();
        }
    }

    /** Record a missed heartbeat (timeout). */
    recordHeartbeatMiss(): void {
        this._heartbeatMisses++;
    }

    /** Get a point-in-time snapshot of all metrics. */
    snapshot(): MetricsSnapshot {
        const latencies = [...this._latencies];

        // Calculate percentiles
        let latencyAvg = 0;
        let latencyP50 = 0;
        let latencyP95 = 0;
        let latencyP99 = 0;
        let latencyMin = 0;
        let latencyMax = 0;

        if (latencies.length > 0) {
            const sorted = [...latencies].sort((a, b) => a - b);
            const n = sorted.length;
            const p50Idx = Math.floor(n * 0.5);
            const p95Idx = Math.floor(n * 0.95);
            const p99Idx = Math.floor(n * 0.99);

            latencyAvg = latencies.reduce((a, b) => a + b, 0) / n;
            latencyP50 = sorted[Math.min(p50Idx, n - 1)];
            latencyP95 = sorted[Math.min(p95Idx, n - 1)];
            latencyP99 = sorted[Math.min(p99Idx, n - 1)];
            latencyMin = sorted[0];
            latencyMax = sorted[n - 1];
        }

        // Calculate heartbeat RTT average
        let heartbeatRttAvg = 0;
        let heartbeatRttLast = 0;
        if (this._heartbeatRtts.length > 0) {
            heartbeatRttAvg = this._heartbeatRtts.reduce((a, b) => a + b, 0) / this._heartbeatRtts.length;
            heartbeatRttLast = this._heartbeatRtts[this._heartbeatRtts.length - 1];
        }

        return {
            requestsTotal: this._requestsTotal,
            requestsSuccess: this._requestsSuccess,
            requestsFailed: this._requestsFailed,
            latencyAvgMs: latencyAvg,
            latencyP50Ms: latencyP50,
            latencyP95Ms: latencyP95,
            latencyP99Ms: latencyP99,
            latencyMinMs: latencyMin,
            latencyMaxMs: latencyMax,
            queueDepth: this._queueDepth,
            queueMaxDepth: this._queueMaxDepth,
            circuitBreakerTrips: this._circuitBreakerTrips,
            circuitBreakerState: this._circuitBreakerState,
            heartbeatRttAvgMs: heartbeatRttAvg,
            heartbeatRttLastMs: heartbeatRttLast,
            heartbeatMisses: this._heartbeatMisses,
            windowSeconds: this.windowSeconds,
            timestamp: Date.now() / 1000,
        };
    }

    /** Reset all metrics. */
    reset(): void {
        this._requestsTotal = 0;
        this._requestsSuccess = 0;
        this._requestsFailed = 0;
        this._circuitBreakerTrips = 0;
        this._circuitBreakerState = "closed";
        this._queueDepth = 0;
        this._queueMaxDepth = 0;
        this._latencies = [];
        this._heartbeatRtts = [];
        this._heartbeatMisses = 0;
    }

    /** Get metrics as a dictionary (for logging/serialization). */
    toDict(): MetricsDict {
        const snapshot = this.snapshot();
        return {
            requests: {
                total: snapshot.requestsTotal,
                success: snapshot.requestsSuccess,
                failed: snapshot.requestsFailed,
                errorRate: snapshot.requestsTotal > 0
                    ? snapshot.requestsFailed / snapshot.requestsTotal
                    : 0.0,
            },
            latencyMs: {
                avg: Math.round(snapshot.latencyAvgMs * 100) / 100,
                p50: Math.round(snapshot.latencyP50Ms * 100) / 100,
                p95: Math.round(snapshot.latencyP95Ms * 100) / 100,
                p99: Math.round(snapshot.latencyP99Ms * 100) / 100,
                min: Math.round(snapshot.latencyMinMs * 100) / 100,
                max: Math.round(snapshot.latencyMaxMs * 100) / 100,
            },
            queue: {
                depth: snapshot.queueDepth,
                maxDepth: snapshot.queueMaxDepth,
            },
            circuitBreaker: {
                trips: snapshot.circuitBreakerTrips,
                state: snapshot.circuitBreakerState,
            },
            heartbeat: {
                rttAvgMs: Math.round(snapshot.heartbeatRttAvgMs * 100) / 100,
                rttLastMs: Math.round(snapshot.heartbeatRttLastMs * 100) / 100,
                misses: snapshot.heartbeatMisses,
            },
        };
    }
}
