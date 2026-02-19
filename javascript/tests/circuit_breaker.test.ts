/**
 * Unit tests for Circuit Breaker behavior in ParentWorker.
 * Tests the failure counting, tripping, and recovery logic.
 * 
 * Run with: npx tsx tests/circuit_breaker.test.ts
 */

import {
    ParentWorker,
    CircuitOpenError,
    RemoteCallError,
    MessageType,
    ComlinkMessage,
} from "../src/multifrost.js";
import { fileURLToPath } from "url";
import { dirname } from "path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

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

function describe(name: string, fn: () => Promise<void>): Promise<void> {
    console.log(`\n=== ${name} ===`);
    return fn()
        .then(() => console.log(`  PASSED`))
        .catch((e) => console.log(`  FAILED: ${e instanceof Error ? e.message : e}`));
}

// ============================================================================
// TEST WORKER
// ============================================================================

class CircuitTestWorker {
    // This will be a child process worker
    alwaysFail(): void {
        throw new Error("Always fails!");
    }
    
    sometimesFail(shouldFail: boolean): string {
        if (shouldFail) {
            throw new Error("Failed as requested");
        }
        return "Success";
    }
    
    add(a: number, b: number): number {
        return a + b;
    }
}

// Worker entry point
if (process.argv.includes("--circuit-worker")) {
    // Simple worker that responds to calls
    const handlers: Record<string, (...args: unknown[]) => unknown> = {
        alwaysFail: () => { throw new Error("Always fails!"); },
        sometimesFail: (shouldFail: boolean) => {
            if (shouldFail) throw new Error("Failed as requested");
            return "Success";
        },
        add: (a: number, b: number) => a + b,
        echo: (val: unknown) => val,
    };
    
    // Import and use the child worker setup
    const { ChildWorker } = await import("../src/multifrost.js");
    
    class TestCircuitWorker extends ChildWorker {
        alwaysFail(): never {
            throw new Error("Always fails!");
        }
        
        sometimesFail(shouldFail: boolean): string {
            if (shouldFail) throw new Error("Failed as requested");
            return "Success";
        }
        
        add(a: number, b: number): number {
            return a + b;
        }
    }
    
    new TestCircuitWorker().run();
}

// ============================================================================
// TESTS: Circuit Breaker State Transitions
// ============================================================================

async function testCircuitBreakerInitialState() {
    await describe("Circuit Breaker: Initial state is closed", async () => {
        const worker = ParentWorker.spawn(
            `${__filename} --circuit-worker`,
            "npx tsx",
            { maxRestartAttempts: 3 }
        );
        
        // Check circuit breaker state before starting
        assert(!worker.circuitOpen, "Circuit should start closed");
        
        // Start the worker
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            assert(worker.isHealthy, "Worker should be healthy after start");
        } finally {
            await worker.stop();
        }
    });
}

async function testCircuitBreakerTripsAfterFailures() {
    await describe("Circuit Breaker: Trips after maxRestartAttempts failures", async () => {
        const worker = ParentWorker.spawn(
            `${__filename} --circuit-worker`,
            "npx tsx",
            { maxRestartAttempts: 3 }
        );
        
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // Trigger failures
            for (let i = 0; i < 3; i++) {
                try {
                    await worker.call.alwaysFail();
                } catch (e) {
                    // Expected
                }
            }
            
            // Circuit should now be open
            assert(worker.circuitOpen, "Circuit should be open after 3 failures");
            assert(!worker.isHealthy, "Worker should not be healthy");
        } finally {
            await worker.stop();
        }
    });
}

async function testCircuitBreakerRejectsWhenOpen() {
    await describe("Circuit Breaker: Rejects calls when open", async () => {
        const worker = ParentWorker.spawn(
            `${__filename} --circuit-worker`,
            "npx tsx",
            { maxRestartAttempts: 2 }
        );
        
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // Trigger enough failures to open circuit
            for (let i = 0; i < 2; i++) {
                try {
                    await worker.call.alwaysFail();
                } catch {
                    // Expected
                }
            }
            
            assert(worker.circuitOpen, "Circuit should be open");
            
            // Next call should be rejected immediately
            try {
                await worker.call.add(1, 2);
                assert(false, "Should have thrown CircuitOpenError");
            } catch (e) {
                assert(e instanceof CircuitOpenError, "Should throw CircuitOpenError");
            }
        } finally {
            await worker.stop();
        }
    });
}

async function testCircuitBreakerResetsOnSuccess() {
    await describe("Circuit Breaker: Resets on successful call", async () => {
        const worker = ParentWorker.spawn(
            `${__filename} --circuit-worker`,
            "npx tsx",
            { maxRestartAttempts: 5 }
        );
        
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // Trigger some failures (but not enough to open)
            for (let i = 0; i < 3; i++) {
                try {
                    await worker.call.sometimesFail(true);
                } catch {
                    // Expected
                }
            }
            
            assert(!worker.circuitOpen, "Circuit should still be closed (only 3 failures)");
            
            // Now succeed
            const result = await worker.call.sometimesFail(false);
            
            // After success, failures should be reset
            // Verify by triggering failures again - they should count from 0
            for (let i = 0; i < 4; i++) {
                try {
                    await worker.call.sometimesFail(true);
                } catch {
                    // Expected
                }
            }
            
            // Circuit should still be closed (only 4 failures since reset)
            assert(!worker.circuitOpen, "Circuit should be closed after success reset failures");
        } finally {
            await worker.stop();
        }
    });
}

async function testCircuitBreakerWithTimeout() {
    await describe("Circuit Breaker: Counts timeout as failure", async () => {
        const worker = ParentWorker.spawn(
            `${__filename} --circuit-worker`,
            "npx tsx",
            { maxRestartAttempts: 3, defaultTimeout: 100 }
        );
        
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // Create a slow function that will timeout
            // We'll use callFunction with a short timeout
            for (let i = 0; i < 3; i++) {
                try {
                    // Using a non-existent function will likely timeout or error
                    await worker.callFunction("nonExistent", [], 50);
                } catch {
                    // Expected - either timeout or not found
                }
            }
            
            // Multiple failures should open circuit
            assert(worker.circuitOpen, "Circuit should be open after timeouts");
        } finally {
            await worker.stop();
        }
    });
}

async function testCircuitBreakerCustomThreshold() {
    await describe("Circuit Breaker: Custom threshold", async () => {
        const worker = ParentWorker.spawn(
            `${__filename} --circuit-worker`,
            "npx tsx",
            { maxRestartAttempts: 10 }
        );
        
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // Trigger 5 failures
            for (let i = 0; i < 5; i++) {
                try {
                    await worker.call.alwaysFail();
                } catch {
                    // Expected
                }
            }
            
            // Circuit should still be closed (threshold is 10)
            assert(!worker.circuitOpen, "Circuit should be closed with 5 failures (threshold 10)");
            
            // Trigger more failures
            for (let i = 0; i < 5; i++) {
                try {
                    await worker.call.alwaysFail();
                } catch {
                    // Expected
                }
            }
            
            // Now circuit should be open
            assert(worker.circuitOpen, "Circuit should be open with 10 failures");
        } finally {
            await worker.stop();
        }
    });
}

async function testCircuitBreakerErrorMessage() {
    await describe("Circuit Breaker: Error message contains failure count", async () => {
        const worker = ParentWorker.spawn(
            `${__filename} --circuit-worker`,
            "npx tsx",
            { maxRestartAttempts: 2 }
        );
        
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // Trigger failures
            for (let i = 0; i < 2; i++) {
                try {
                    await worker.call.alwaysFail();
                } catch {
                    // Expected
                }
            }
            
            // Try to call and catch the error
            try {
                await worker.call.add(1, 1);
                assert(false, "Should have thrown");
            } catch (e) {
                assert(e instanceof CircuitOpenError, "Should be CircuitOpenError");
                assert((e as Error).message.includes("2"), "Error should mention 2 failures");
            }
        } finally {
            await worker.stop();
        }
    });
}

async function testCircuitBreakerSuccessAfterFailures() {
    await describe("Circuit Breaker: Success resets failure count", async () => {
        const worker = ParentWorker.spawn(
            `${__filename} --circuit-worker`,
            "npx tsx",
            { maxRestartAttempts: 5 }
        );
        
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // 2 failures
            try { await worker.call.alwaysFail(); } catch {}
            try { await worker.call.alwaysFail(); } catch {}
            
            // 1 success - should reset
            await worker.call.add(5, 3);
            
            // 4 more failures (would be 6 total without reset)
            try { await worker.call.alwaysFail(); } catch {}
            try { await worker.call.alwaysFail(); } catch {}
            try { await worker.call.alwaysFail(); } catch {}
            try { await worker.call.alwaysFail(); } catch {}
            
            // Circuit should still be closed (only 4 failures since reset)
            assert(!worker.circuitOpen, "Circuit should be closed (4 < 5)");
        } finally {
            await worker.stop();
        }
    });
}

// ============================================================================
// RUN ALL TESTS
// ============================================================================

async function runAllTests() {
    console.log("Running Circuit Breaker tests...\n");
    
    await testCircuitBreakerInitialState();
    await testCircuitBreakerTripsAfterFailures();
    await testCircuitBreakerRejectsWhenOpen();
    await testCircuitBreakerResetsOnSuccess();
    await testCircuitBreakerWithTimeout();
    await testCircuitBreakerCustomThreshold();
    await testCircuitBreakerErrorMessage();
    await testCircuitBreakerSuccessAfterFailures();
    
    // Summary
    console.log("\n" + "=".repeat(50));
    console.log(`CIRCUIT BREAKER TEST RESULTS: ${passed} passed, ${failed} failed`);
    console.log("=".repeat(50));
    
    if (failed > 0) {
        process.exit(1);
    }
}

// Check if we're running as the worker
if (process.argv.includes("--circuit-worker")) {
    // Worker mode - handled above in the dynamic import
} else {
    runAllTests().catch(console.error);
}
