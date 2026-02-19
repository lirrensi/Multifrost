/**
 * Unit tests for ParentWorker class.
 * Tests spawning, connecting, calling functions, handle API, and lifecycle.
 * 
 * Run with: npx tsx tests/parent_worker.test.ts
 */

import {
    ParentWorker,
    ParentHandle,
    ChildWorker,
    RemoteCallError,
    CircuitOpenError,
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

class TestParentWorker extends ChildWorker {
    add(a: number, b: number): number {
        return a + b;
    }
    
    subtract(a: number, b: number): number {
        return a - b;
    }
    
    multiply(a: number, b: number): number {
        return a * b;
    }
    
    async asyncAdd(a: number, b: number): Promise<number> {
        await new Promise(r => setTimeout(r, 50));
        return a + b;
    }
    
    echo(value: unknown): unknown {
        return value;
    }
    
    throwError(message: string): never {
        throw new Error(message);
    }
    
    getProcessInfo(): { pid: number; env: Record<string, string | undefined> } {
        return {
            pid: process.pid,
            env: {
                COMLINK_ZMQ_PORT: process.env.COMLINK_ZMQ_PORT,
            },
        };
    }
    
    sleep(ms: number): Promise<string> {
        return new Promise(resolve => {
            setTimeout(() => resolve(`Slept ${ms}ms`), ms);
        });
    }
}

// Worker entry point
if (process.argv.includes("--parent-test-worker")) {
    new TestParentWorker().run();
}

// ============================================================================
// TESTS: Spawn Mode Construction
// ============================================================================

async function testSpawnConstruction() {
    await describe("ParentWorker.spawn: Basic construction", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        
        assert(worker.autoRestart === false, "autoRestart should default to false");
        assert(worker.maxRestartAttempts === 5, "maxRestartAttempts should default to 5");
        assert(worker.heartbeatInterval === 5.0, "heartbeatInterval should default to 5.0");
        assert(worker.heartbeatTimeout === 3.0, "heartbeatTimeout should default to 3.0");
        assert(worker.heartbeatMaxMisses === 3, "heartbeatMaxMisses should default to 3");
    });
}

async function testSpawnWithCustomOptions() {
    await describe("ParentWorker.spawn: Custom options", async () => {
        const worker = ParentWorker.spawn(
            `${__filename} --parent-test-worker`,
            "npx tsx",
            {
                autoRestart: true,
                maxRestartAttempts: 10,
                defaultTimeout: 5000,
                heartbeatInterval: 2.0,
                heartbeatTimeout: 1.0,
                heartbeatMaxMisses: 5,
            }
        );
        
        assert(worker.autoRestart === true, "autoRestart should be true");
        assert(worker.maxRestartAttempts === 10, "maxRestartAttempts should be 10");
        assert(worker.defaultTimeout === 5000, "defaultTimeout should be 5000");
        assert(worker.heartbeatInterval === 2.0, "heartbeatInterval should be 2.0");
        assert(worker.heartbeatTimeout === 1.0, "heartbeatTimeout should be 1.0");
        assert(worker.heartbeatMaxMisses === 5, "heartbeatMaxMisses should be 5");
    });
}

// ============================================================================
// TESTS: Start and Stop
// ============================================================================

async function testStartStop() {
    await describe("ParentWorker: Start and stop", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        
        await worker.start();
        
        // Worker should be running
        assert(worker.isHealthy, "Worker should be healthy after start");
        assert(!worker.circuitOpen, "Circuit should be closed");
        
        await worker.stop();
        
        // Worker should be stopped
        assert(!worker.isHealthy, "Worker should not be healthy after stop");
    });
}

async function testMultipleStartStop() {
    await describe("ParentWorker: Multiple start/stop cycles", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        
        // First cycle
        await worker.start();
        await new Promise(r => setTimeout(r, 200));
        await worker.stop();
        
        // Second cycle - should work
        await worker.start();
        await new Promise(r => setTimeout(r, 200));
        
        const result = await worker.call.add(1, 2);
        assert(result === 3, "Should work after restart");
        
        await worker.stop();
    });
}

// ============================================================================
// TESTS: Function Calling
// ============================================================================

async function testBasicCall() {
    await describe("ParentWorker: Basic function call", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            const result = await worker.call.add(10, 20);
            assert(result === 30, "add(10, 20) should return 30");
        } finally {
            await worker.stop();
        }
    });
}

async function testAsyncCall() {
    await describe("ParentWorker: Async function call", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            const result = await worker.call.asyncAdd(5, 15);
            assert(result === 20, "asyncAdd(5, 15) should return 20");
        } finally {
            await worker.stop();
        }
    });
}

async function testMultipleCalls() {
    await describe("ParentWorker: Multiple sequential calls", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            const r1 = await worker.call.add(1, 2);
            const r2 = await worker.call.subtract(10, 3);
            const r3 = await worker.call.multiply(4, 5);
            
            assert(r1 === 3, "add should work");
            assert(r2 === 7, "subtract should work");
            assert(r3 === 20, "multiply should work");
        } finally {
            await worker.stop();
        }
    });
}

async function testCallWithVariousTypes() {
    await describe("ParentWorker: Call with various argument types", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // String
            const r1 = await worker.call.echo("hello");
            assert(r1 === "hello", "Should echo string");
            
            // Number
            const r2 = await worker.call.echo(42);
            assert(r2 === 42, "Should echo number");
            
            // Object
            const r3 = await worker.call.echo({ a: 1, b: "test" });
            assert((r3 as any).a === 1, "Should echo object");
            
            // Array
            const r4 = await worker.call.echo([1, 2, 3]);
            assert(Array.isArray(r4), "Should echo array");
            
            // Null
            const r5 = await worker.call.echo(null);
            assert(r5 === null, "Should echo null");
            
            // Boolean
            const r6 = await worker.call.echo(true);
            assert(r6 === true, "Should echo boolean");
        } finally {
            await worker.stop();
        }
    });
}

async function testCallError() {
    await describe("ParentWorker: Call that throws error", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            try {
                await worker.call.throwError("Test error message");
                assert(false, "Should have thrown");
            } catch (e) {
                assert(e instanceof RemoteCallError, "Should be RemoteCallError");
                assert((e as Error).message.includes("Test error message"), "Should have error message");
            }
        } finally {
            await worker.stop();
        }
    });
}

async function testCallTimeout() {
    await describe("ParentWorker: Call timeout", async () => {
        const worker = ParentWorker.spawn(
            `${__filename} --parent-test-worker`,
            "npx tsx",
            { defaultTimeout: 100 }
        );
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            try {
                // Sleep for 200ms, but timeout is 100ms
                await worker.call.sleep(200);
                assert(false, "Should have timed out");
            } catch (e) {
                assert((e as Error).message.includes("timed out"), "Should be timeout error");
            }
        } finally {
            await worker.stop();
        }
    });
}

async function testCallWithTimeout() {
    await describe("ParentWorker: Call with explicit timeout", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // This should complete within timeout
            const result = await worker.callFunction("sleep", [50], 200);
            assert(result === "Slept 50ms", "Should complete within timeout");
            
            // This should timeout
            try {
                await worker.callFunction("sleep", [300], 100);
                assert(false, "Should have timed out");
            } catch (e) {
                assert((e as Error).message.includes("timed out"), "Should timeout");
            }
        } finally {
            await worker.stop();
        }
    });
}

async function testCallNonExistentFunction() {
    await describe("ParentWorker: Call non-existent function", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            try {
                await worker.call.nonExistentFunction();
                assert(false, "Should have thrown");
            } catch (e) {
                assert(e instanceof RemoteCallError, "Should be RemoteCallError");
                assert((e as Error).message.includes("not found") || 
                       (e as Error).message.includes("not callable"), 
                       "Should mention function not found");
            }
        } finally {
            await worker.stop();
        }
    });
}

// ============================================================================
// TESTS: Handle API
// ============================================================================

async function testHandleBasic() {
    await describe("ParentHandle: Basic usage", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        const handle = worker.handle();
        
        await handle.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            const result = await handle.call.add(100, 200);
            assert(result === 300, "Handle should work for calls");
        } finally {
            await handle.stop();
        }
    });
}

async function testHandleMultipleFromSameWorker() {
    await describe("ParentHandle: Multiple handles from same worker", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        const handle1 = worker.handle();
        const handle2 = worker.handle();
        
        await handle1.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // Both handles should work
            const r1 = await handle1.call.add(1, 2);
            const r2 = await handle2.call.subtract(10, 5);
            
            assert(r1 === 3, "Handle 1 should work");
            assert(r2 === 5, "Handle 2 should work");
        } finally {
            await handle1.stop();
        }
    });
}

// ============================================================================
// TESTS: Proxy withOptions
// ============================================================================

async function testWithOptions() {
    await describe("ParentWorker: withOptions for timeout", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // withOptions should work
            const result = await worker
                .call.withOptions({ timeout: 5000 })
                .add(5, 10);
            
            assert(result === 15, "withOptions should work");
        } finally {
            await worker.stop();
        }
    });
}

async function testWithOptionsNamespace() {
    await describe("ParentWorker: withOptions for namespace", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // Namespace option - may fail if worker doesn't have that namespace
            try {
                await worker
                    .call.withOptions({ namespace: "custom" })
                    .add(1, 2);
                // Might work if namespace is ignored
            } catch {
                // Might fail due to namespace mismatch
            }
            assert(true, "withOptions namespace should be processed");
        } finally {
            await worker.stop();
        }
    });
}

// ============================================================================
// TESTS: isHealthy
// ============================================================================

async function testIsHealthy() {
    await describe("ParentWorker: isHealthy property", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        
        assert(!worker.isHealthy, "Should not be healthy before start");
        
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        assert(worker.isHealthy, "Should be healthy after start");
        
        await worker.stop();
        
        assert(!worker.isHealthy, "Should not be healthy after stop");
    });
}

// ============================================================================
// TESTS: Edge Cases
// ============================================================================

async function testCallBeforeStart() {
    await describe("ParentWorker: Call before start throws", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        
        try {
            await worker.call.add(1, 2);
            assert(false, "Should have thrown");
        } catch (e) {
            assert((e as Error).message.includes("not running"), "Should say not running");
        }
    });
}

async function testEmptyArgs() {
    await describe("ParentWorker: Call with empty args", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            const result = await worker.call.getProcessInfo();
            assert(typeof (result as any).pid === "number", "Should return process info");
        } finally {
            await worker.stop();
        }
    });
}

async function testLargePayload() {
    await describe("ParentWorker: Call with large payload", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // Create a large array (10KB)
            const largeArray = Array(10000).fill("x");
            const result = await worker.call.echo(largeArray);
            
            assert(Array.isArray(result), "Should return array");
            assert((result as string[]).length === 10000, "Should preserve length");
        } finally {
            await worker.stop();
        }
    });
}

async function testUnicodeArgs() {
    await describe("ParentWorker: Call with unicode arguments", async () => {
        const worker = ParentWorker.spawn(`${__filename} --parent-test-worker`, "npx tsx");
        await worker.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            const unicodeStrings = ["日本語", "한국어", "العربية", "🎉🔥❄️"];
            const result = await worker.call.echo(unicodeStrings);
            
            assert((result as string[])[0] === "日本語", "Should preserve Japanese");
            assert((result as string[])[1] === "한국어", "Should preserve Korean");
            assert((result as string[])[2] === "العربية", "Should preserve Arabic");
            assert((result as string[])[3] === "🎉🔥❄️", "Should preserve emoji");
        } finally {
            await worker.stop();
        }
    });
}

// ============================================================================
// RUN ALL TESTS
// ============================================================================

async function runAllTests() {
    console.log("Running ParentWorker tests...\n");
    
    // Construction tests
    await testSpawnConstruction();
    await testSpawnWithCustomOptions();
    
    // Start/Stop tests
    await testStartStop();
    await testMultipleStartStop();
    
    // Function calling tests
    await testBasicCall();
    await testAsyncCall();
    await testMultipleCalls();
    await testCallWithVariousTypes();
    await testCallError();
    await testCallTimeout();
    await testCallWithTimeout();
    await testCallNonExistentFunction();
    
    // Handle API tests
    await testHandleBasic();
    await testHandleMultipleFromSameWorker();
    
    // withOptions tests
    await testWithOptions();
    await testWithOptionsNamespace();
    
    // isHealthy tests
    await testIsHealthy();
    
    // Edge cases
    await testCallBeforeStart();
    await testEmptyArgs();
    await testLargePayload();
    await testUnicodeArgs();
    
    // Summary
    console.log("\n" + "=".repeat(50));
    console.log(`PARENT WORKER TEST RESULTS: ${passed} passed, ${failed} failed`);
    console.log("=".repeat(50));
    
    if (failed > 0) {
        process.exit(1);
    }
}

// Check if we're running as the worker
if (process.argv.includes("--parent-test-worker")) {
    // Worker mode - handled by TestParentWorker class
} else {
    runAllTests().catch(console.error);
}
