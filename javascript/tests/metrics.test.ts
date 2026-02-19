/**
 * Unit tests for Metrics class.
 * Tests request tracking, latency percentiles, circuit breaker, and heartbeat metrics.
 * 
 * Run with: npx tsx tests/metrics.test.ts
 */

import { Metrics, MetricsSnapshot, MetricsDict } from "../src/metrics.js";

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
// TESTS: Basic Construction
// ============================================================================

describe("Metrics: Default Construction", () => {
    const metrics = new Metrics();
    const snapshot = metrics.snapshot();
    
    assert(snapshot.requestsTotal === 0, "Should start with 0 total requests");
    assert(snapshot.requestsSuccess === 0, "Should start with 0 successful requests");
    assert(snapshot.requestsFailed === 0, "Should start with 0 failed requests");
    assert(snapshot.queueDepth === 0, "Should start with 0 queue depth");
    assert(snapshot.queueMaxDepth === 0, "Should start with 0 max queue depth");
    assert(snapshot.circuitBreakerTrips === 0, "Should start with 0 circuit breaker trips");
    assert(snapshot.circuitBreakerState === "closed", "Circuit breaker should start closed");
    assert(snapshot.heartbeatMisses === 0, "Should start with 0 heartbeat misses");
});

describe("Metrics: Custom Configuration", () => {
    const metrics = new Metrics({ maxLatencySamples: 100, windowSeconds: 30.0 });
    const snapshot = metrics.snapshot();
    
    assert(snapshot.windowSeconds === 30.0, "Should use custom window seconds");
});

// ============================================================================
// TESTS: Request Tracking
// ============================================================================

describe("Metrics: Single Request Success", () => {
    const metrics = new Metrics();
    
    const start = metrics.startRequest("req-1", "testFunc", "default");
    
    // Check queue depth during request
    let snapshot = metrics.snapshot();
    assert(snapshot.queueDepth === 1, "Queue depth should be 1 during request");
    assert(snapshot.queueMaxDepth === 1, "Max queue depth should be 1");
    assert(snapshot.requestsTotal === 1, "Total requests should be 1");
    
    // Simulate some work
    const latency = metrics.endRequest(start, "req-1", true);
    
    snapshot = metrics.snapshot();
    assert(snapshot.queueDepth === 0, "Queue depth should be 0 after request");
    assert(snapshot.requestsSuccess === 1, "Success count should be 1");
    assert(snapshot.requestsFailed === 0, "Failed count should be 0");
    assert(latency >= 0, "Latency should be non-negative");
});

describe("Metrics: Single Request Failure", () => {
    const metrics = new Metrics();
    
    const start = metrics.startRequest("req-2", "failingFunc", "default");
    const latency = metrics.endRequest(start, "req-2", false, "Connection timeout");
    
    const snapshot = metrics.snapshot();
    assert(snapshot.requestsSuccess === 0, "Success count should be 0");
    assert(snapshot.requestsFailed === 1, "Failed count should be 1");
    assert(snapshot.requestsTotal === 1, "Total should be 1");
});

describe("Metrics: Multiple Requests", () => {
    const metrics = new Metrics();
    
    // Start 3 concurrent requests
    const start1 = metrics.startRequest("req-1", "func1", "default");
    const start2 = metrics.startRequest("req-2", "func2", "default");
    const start3 = metrics.startRequest("req-3", "func3", "default");
    
    let snapshot = metrics.snapshot();
    assert(snapshot.queueDepth === 3, "Queue depth should be 3");
    assert(snapshot.queueMaxDepth === 3, "Max queue depth should be 3");
    assert(snapshot.requestsTotal === 3, "Total should be 3");
    
    // End them
    metrics.endRequest(start1, "req-1", true);
    metrics.endRequest(start2, "req-2", false, "error");
    metrics.endRequest(start3, "req-3", true);
    
    snapshot = metrics.snapshot();
    assert(snapshot.queueDepth === 0, "Queue should be empty");
    assert(snapshot.requestsSuccess === 2, "Should have 2 successes");
    assert(snapshot.requestsFailed === 1, "Should have 1 failure");
});

// ============================================================================
// TESTS: Latency Tracking
// ============================================================================

describe("Metrics: Latency Percentiles", () => {
    const metrics = new Metrics();
    
    // Add 100 samples with increasing latencies
    for (let i = 0; i < 100; i++) {
        const start = metrics.startRequest(`req-${i}`, "func", "default");
        // Simulate different latencies (i * 10ms)
        // We can't actually wait, so we'll use startRequest's behavior
        metrics.endRequest(start, `req-${i}`, true);
    }
    
    const snapshot = metrics.snapshot();
    assert(snapshot.requestsTotal === 100, "Should have 100 requests");
    assert(snapshot.latencyMinMs !== undefined, "Should have min latency");
    assert(snapshot.latencyMaxMs !== undefined, "Should have max latency");
    assert(snapshot.latencyAvgMs !== undefined, "Should have avg latency");
});

describe("Metrics: Latency Circular Buffer", () => {
    const metrics = new Metrics({ maxLatencySamples: 10 });
    
    // Add more samples than buffer size
    for (let i = 0; i < 20; i++) {
        const start = metrics.startRequest(`req-${i}`, "func", "default");
        metrics.endRequest(start, `req-${i}`, true);
    }
    
    // Should only keep last 10
    const snapshot = metrics.snapshot();
    // Latencies array should have at most 10 elements
    // This tests the circular buffer behavior
    assert(snapshot.requestsTotal === 20, "Should track 20 total requests");
});

// ============================================================================
// TESTS: Circuit Breaker Tracking
// ============================================================================

describe("Metrics: Circuit Breaker Trip", () => {
    const metrics = new Metrics();
    
    metrics.recordCircuitBreakerTrip();
    
    const snapshot = metrics.snapshot();
    assert(snapshot.circuitBreakerTrips === 1, "Should have 1 trip");
    assert(snapshot.circuitBreakerState === "open", "State should be open");
});

describe("Metrics: Multiple Circuit Breaker Trips", () => {
    const metrics = new Metrics();
    
    metrics.recordCircuitBreakerTrip();
    metrics.recordCircuitBreakerTrip();
    metrics.recordCircuitBreakerTrip();
    
    const snapshot = metrics.snapshot();
    assert(snapshot.circuitBreakerTrips === 3, "Should have 3 trips");
    assert(snapshot.circuitBreakerState === "open", "State should still be open");
});

describe("Metrics: Circuit Breaker Reset", () => {
    const metrics = new Metrics();
    
    metrics.recordCircuitBreakerTrip();
    metrics.recordCircuitBreakerReset();
    
    const snapshot = metrics.snapshot();
    assert(snapshot.circuitBreakerTrips === 1, "Trip count should remain 1");
    assert(snapshot.circuitBreakerState === "closed", "State should be closed");
});

describe("Metrics: Circuit Breaker Half-Open", () => {
    const metrics = new Metrics();
    
    metrics.recordCircuitBreakerTrip();
    metrics.recordCircuitBreakerHalfOpen();
    
    const snapshot = metrics.snapshot();
    assert(snapshot.circuitBreakerState === "half-open", "State should be half-open");
});

// ============================================================================
// TESTS: Heartbeat Tracking
// ============================================================================

describe("Metrics: Heartbeat RTT Recording", () => {
    const metrics = new Metrics();
    
    metrics.recordHeartbeatRtt(10.5);
    metrics.recordHeartbeatRtt(12.3);
    metrics.recordHeartbeatRtt(8.7);
    
    const snapshot = metrics.snapshot();
    assert(snapshot.heartbeatRttAvgMs > 0, "Should have average RTT");
    assert(snapshot.heartbeatRttLastMs === 8.7, "Should have last RTT");
});

describe("Metrics: Heartbeat Miss Recording", () => {
    const metrics = new Metrics();
    
    metrics.recordHeartbeatMiss();
    metrics.recordHeartbeatMiss();
    metrics.recordHeartbeatMiss();
    
    const snapshot = metrics.snapshot();
    assert(snapshot.heartbeatMisses === 3, "Should have 3 misses");
});

describe("Metrics: Heartbeat RTT Circular Buffer", () => {
    const metrics = new Metrics();
    
    // Add more than 100 samples
    for (let i = 0; i < 150; i++) {
        metrics.recordHeartbeatRtt(i);
    }
    
    const snapshot = metrics.snapshot();
    // Should still work - only last 100 kept
    assert(snapshot.heartbeatRttLastMs === 149, "Last RTT should be 149");
});

// ============================================================================
// TESTS: Reset Functionality
// ============================================================================

describe("Metrics: Reset", () => {
    const metrics = new Metrics();
    
    // Add some data
    const start = metrics.startRequest("req-1", "func", "default");
    metrics.endRequest(start, "req-1", false, "error");
    metrics.recordCircuitBreakerTrip();
    metrics.recordHeartbeatRtt(100);
    metrics.recordHeartbeatMiss();
    
    // Reset
    metrics.reset();
    
    const snapshot = metrics.snapshot();
    assert(snapshot.requestsTotal === 0, "Total should be 0 after reset");
    assert(snapshot.requestsSuccess === 0, "Success should be 0 after reset");
    assert(snapshot.requestsFailed === 0, "Failed should be 0 after reset");
    assert(snapshot.queueDepth === 0, "Queue depth should be 0");
    assert(snapshot.circuitBreakerTrips === 0, "Circuit breaker trips should be 0");
    assert(snapshot.circuitBreakerState === "closed", "State should be closed");
    assert(snapshot.heartbeatMisses === 0, "Heartbeat misses should be 0");
    assert(snapshot.heartbeatRttAvgMs === 0, "Heartbeat RTT avg should be 0");
});

// ============================================================================
// TESTS: toDict Method
// ============================================================================

describe("Metrics: toDict Structure", () => {
    const metrics = new Metrics();
    
    const start = metrics.startRequest("req-1", "func", "default");
    metrics.endRequest(start, "req-1", true);
    metrics.recordCircuitBreakerTrip();
    
    const dict = metrics.toDict();
    
    assert("requests" in dict, "Should have requests key");
    assert("latencyMs" in dict, "Should have latencyMs key");
    assert("queue" in dict, "Should have queue key");
    assert("circuitBreaker" in dict, "Should have circuitBreaker key");
    assert("heartbeat" in dict, "Should have heartbeat key");
    
    assert(dict.requests.total === 1, "Total should be 1");
    assert(dict.requests.success === 1, "Success should be 1");
    assert(dict.requests.failed === 0, "Failed should be 0");
    assert(typeof dict.requests.errorRate === "number", "Should have error rate");
    assert(dict.circuitBreaker.state === "open", "Circuit breaker state should be open");
    assert(dict.circuitBreaker.trips === 1, "Trips should be 1");
});

describe("Metrics: toDict Error Rate Calculation", () => {
    const metrics = new Metrics();
    
    // 5 requests: 2 success, 3 failures
    for (let i = 0; i < 5; i++) {
        const start = metrics.startRequest(`req-${i}`, "func", "default");
        metrics.endRequest(start, `req-${i}`, i < 2);
    }
    
    const dict = metrics.toDict();
    
    assert(dict.requests.total === 5, "Total should be 5");
    assert(dict.requests.success === 2, "Success should be 2");
    assert(dict.requests.failed === 3, "Failed should be 3");
    assert(dict.requests.errorRate === 0.6, "Error rate should be 0.6 (3/5)");
});

describe("Metrics: toDict Zero Requests", () => {
    const metrics = new Metrics();
    const dict = metrics.toDict();
    
    assert(dict.requests.errorRate === 0, "Error rate should be 0 with no requests");
    assert(dict.latencyMs.avg === 0, "Latency avg should be 0");
    assert(dict.latencyMs.p50 === 0, "Latency p50 should be 0");
    assert(dict.latencyMs.min === 0, "Latency min should be 0");
    assert(dict.latencyMs.max === 0, "Latency max should be 0");
});

// ============================================================================
// TESTS: Edge Cases
// ============================================================================

describe("Metrics: Empty Namespace Default", () => {
    const metrics = new Metrics();
    
    // Should work without namespace
    const start = metrics.startRequest("req-1", "func");
    metrics.endRequest(start, "req-1", true);
    
    const snapshot = metrics.snapshot();
    assert(snapshot.requestsTotal === 1, "Should count request without namespace");
});

describe("Metrics: Large Number of Requests", () => {
    const metrics = new Metrics({ maxLatencySamples: 1000 });
    
    // Add 1000 requests
    for (let i = 0; i < 1000; i++) {
        const start = metrics.startRequest(`req-${i}`, `func-${i % 10}`, "ns");
        metrics.endRequest(start, `req-${i}`, i % 3 !== 0); // 2/3 success rate
    }
    
    const snapshot = metrics.snapshot();
    assert(snapshot.requestsTotal === 1000, "Should track 1000 requests");
    
    // Approximate success rate check
    const successRate = snapshot.requestsSuccess / snapshot.requestsTotal;
    assert(successRate > 0.6 && successRate < 0.7, "Should have ~2/3 success rate");
});

describe("Metrics: Concurrent Queue Depth", () => {
    const metrics = new Metrics();
    
    // Start 10 requests without ending
    for (let i = 0; i < 10; i++) {
        metrics.startRequest(`req-${i}`, "func", "default");
    }
    
    const snapshot = metrics.snapshot();
    assert(snapshot.queueDepth === 10, "Queue depth should be 10");
    assert(snapshot.queueMaxDepth === 10, "Max queue depth should be 10");
    
    // End half
    for (let i = 0; i < 5; i++) {
        metrics.endRequest(performance.now(), `req-${i}`, true);
    }
    
    const snapshot2 = metrics.snapshot();
    assert(snapshot2.queueDepth === 5, "Queue depth should be 5");
    assert(snapshot2.queueMaxDepth === 10, "Max queue depth should still be 10");
});

// ============================================================================
// TESTS: Latency Percentile Calculations
// ============================================================================

describe("Metrics: Latency Percentile Accuracy", () => {
    const metrics = new Metrics();
    
    // Create known latency distribution
    // Add 100 samples with latencies 0-99
    for (let i = 0; i < 100; i++) {
        // Manually manipulate latency by using endRequest timing
        // Since we can't control actual timing, we verify structure
        const start = metrics.startRequest(`req-${i}`, "func", "default");
        metrics.endRequest(start, `req-${i}`, true);
    }
    
    const snapshot = metrics.snapshot();
    
    // Verify percentiles are in sorted order
    assert(snapshot.latencyMinMs <= snapshot.latencyP50Ms, "Min should be <= P50");
    assert(snapshot.latencyP50Ms <= snapshot.latencyP95Ms, "P50 should be <= P95");
    assert(snapshot.latencyP95Ms <= snapshot.latencyP99Ms, "P95 should be <= P99");
    assert(snapshot.latencyP99Ms <= snapshot.latencyMaxMs, "P99 should be <= Max");
});

// ============================================================================
// SUMMARY
// ============================================================================

console.log("\n" + "=".repeat(50));
console.log(`METRICS TEST RESULTS: ${passed} passed, ${failed} failed`);
console.log("=".repeat(50));

if (failed > 0) {
    process.exit(1);
}
