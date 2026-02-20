/**
 * E2E Tests for Multifrost - JavaScript Parent Tests
 * These tests verify cross-language interoperability using the same pattern as parent_worker.test.ts.
 * 
 * Run with: npx tsx tests/e2e.test.ts
 */

import { ParentWorker, ChildWorker, RemoteCallError } from "../src/multifrost.js";
import { fileURLToPath } from "url";
import { dirname, join } from "path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Paths to worker scripts
const PYTHON_WORKER = join(__dirname, "..", "e2e", "workers", "math_worker.py");

// Test state
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
        .catch((e) => {
            console.log(`  FAILED: ${e instanceof Error ? e.message : e}`);
            failed++;
        });
}

// ============================================================================
// TEST WORKER (embedded in same file for reliable module loading)
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
    
    divide(a: number, b: number): number {
        if (b === 0) {
            throw new Error("Cannot divide by zero");
        }
        return a / b;
    }
    
    factorial(n: number): number {
        if (n < 0) {
            throw new Error("Factorial not defined for negative numbers");
        }
        if (n > 100) {
            throw new Error("Input too large");
        }
        
        let result = 1;
        for (let i = 2; i <= n; i++) {
            result *= i;
        }
        return result;
    }
    
    async asyncAdd(a: number, b: number): Promise<number> {
        await new Promise(r => setTimeout(r, 50));
        return a + b;
    }
    
    echo(value: unknown): unknown {
        return value;
    }
    
    getInfo(): { language: string; pid: number; version: string } {
        return {
            language: "javascript",
            pid: process.pid,
            version: process.version,
        };
    }

    get_info(): { language: string; pid: number; version: string } {
        return {
            language: "javascript",
            pid: process.pid,
            version: process.version,
        };
    }
    
    throwError(message: string): never {
        throw new Error(message);
    }

    throw_error(message: string): never {
        throw new Error(message);
    }
}

// Worker entry point
if (process.argv.includes("--e2e-worker")) {
    new TestParentWorker().run();
}

// ============================================================================
// JS PARENT -> JS CHILD TESTS (using self as worker)
// ============================================================================

async function testJSParentJSChildBasicCall() {
    await describe("JS Parent -> JS Child: Basic calls", async () => {
        const worker = ParentWorker.spawn(`${__filename} --e2e-worker`, "npx tsx");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 1500));  // Give child time to start

            const result = await worker.call.add(10, 20);
            assert(result === 30, `Expected 30, got ${result}`);

            const result2 = await worker.call.multiply(5, 6);
            assert(result2 === 30, `Expected 30, got ${result2}`);
        } finally {
            await worker.stop();
        }
    });
}

async function testJSParentJSChildVariousTypes() {
    await describe("JS Parent -> JS Child: Various types", async () => {
        const worker = ParentWorker.spawn(`${__filename} --e2e-worker`, "npx tsx");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 1500));

            // String
            let result = await worker.call.echo("hello");
            assert(result === "hello", `Expected "hello", got ${result}`);

            // Number
            result = await worker.call.echo(42);
            assert(result === 42, `Expected 42, got ${result}`);

            // Float
            result = await worker.call.echo(3.14);
            assert(result === 3.14, `Expected 3.14, got ${result}`);

            // Boolean
            result = await worker.call.echo(true);
            assert(result === true, `Expected true, got ${result}`);

            // Array
            result = await worker.call.echo([1, 2, 3]);
            assert(Array.isArray(result) && (result as number[]).length === 3, "Should echo array");

            // Object
            result = await worker.call.echo({ a: 1, b: "test" });
            assert(typeof result === "object" && (result as any).a === 1, "Should echo object");
        } finally {
            await worker.stop();
        }
    });
}

async function testJSParentJSChildErrorHandling() {
    await describe("JS Parent -> JS Child: Error handling", async () => {
        const worker = ParentWorker.spawn(`${__filename} --e2e-worker`, "npx tsx");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 1500));

            try {
                await worker.call.throwError("JS test error");
                assert(false, "Should have thrown");
            } catch (e) {
                assert(e instanceof RemoteCallError, "Should be RemoteCallError");
                assert((e as Error).message.includes("JS test error"), "Should include error message");
            }
        } finally {
            await worker.stop();
        }
    });
}

async function testJSParentJSChildFactorial() {
    await describe("JS Parent -> JS Child: Factorial", async () => {
        const worker = ParentWorker.spawn(`${__filename} --e2e-worker`, "npx tsx");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 1500));

            const result = await worker.call.factorial(10);
            assert(result === 3628800, `Expected 3628800, got ${result}`);
        } finally {
            await worker.stop();
        }
    });
}

async function testJSParentJSChildAsyncMethod() {
    await describe("JS Parent -> JS Child: Async method", async () => {
        const worker = ParentWorker.spawn(`${__filename} --e2e-worker`, "npx tsx");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 1500));

            const result = await worker.call.asyncAdd(10, 20);
            assert(result === 30, `Expected 30, got ${result}`);
        } finally {
            await worker.stop();
        }
    });
}


// ============================================================================
// JS PARENT -> PYTHON CHILD TESTS (cross-language!)
// ============================================================================

async function testJSParentPythonChildBasicCall() {
    await describe("JS Parent -> Python Child: Basic calls", async () => {
        const worker = ParentWorker.spawn(PYTHON_WORKER, "python");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 1500));

            const result = await worker.call.add(10, 20);
            assert(result === 30, `Expected 30, got ${result}`);

            const result2 = await worker.call.multiply(5, 6);
            assert(result2 === 30, `Expected 30, got ${result2}`);
        } finally {
            await worker.stop();
        }
    });
}

async function testJSParentPythonChildVariousTypes() {
    await describe("JS Parent -> Python Child: Various types", async () => {
        const worker = ParentWorker.spawn(PYTHON_WORKER, "python");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 1500));

            // String
            let result = await worker.call.echo("hello");
            assert(result === "hello", `Expected "hello", got ${result}`);

            // Number
            result = await worker.call.echo(42);
            assert(result === 42, `Expected 42, got ${result}`);

            // Float
            result = await worker.call.echo(3.14);
            assert(result === 3.14, `Expected 3.14, got ${result}`);

            // Boolean
            result = await worker.call.echo(true);
            assert(result === true, `Expected true, got ${result}`);

            // None (null in JS)
            result = await worker.call.echo(null);
            assert(result === null, `Expected null, got ${result}`);

            // Array
            result = await worker.call.echo([1, 2, 3]);
            assert(Array.isArray(result) && (result as number[]).length === 3, "Should echo array");

            // Object
            result = await worker.call.echo({ a: 1, b: "test" });
            assert(typeof result === "object" && (result as any).a === 1, "Should echo object");
        } finally {
            await worker.stop();
        }
    });
}

async function testJSParentPythonChildErrorHandling() {
    await describe("JS Parent -> Python Child: Error handling", async () => {
        const worker = ParentWorker.spawn(PYTHON_WORKER, "python");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 1500));

            try {
                await worker.call.throw_error("Python test error");
                assert(false, "Should have thrown");
            } catch (e) {
                assert(e instanceof RemoteCallError, "Should be RemoteCallError");
                assert((e as Error).message.includes("Python test error"), "Should include error message");
            }

            // Test divide by zero
            try {
                await worker.call.divide(10, 0);
                assert(false, "Should have thrown");
            } catch (e) {
                assert(e instanceof RemoteCallError, "Should be RemoteCallError");
            }
        } finally {
            await worker.stop();
        }
    });
}

async function testJSParentPythonChildInfo() {
    await describe("JS Parent -> Python Child: Get info", async () => {
        const worker = ParentWorker.spawn(PYTHON_WORKER, "python");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 1500));

            const info = await worker.call.get_info();
            assert(info.language === "python", `Expected language "python", got ${info.language}`);
            assert(typeof info.pid === "number", "Should have pid");
        } finally {
            await worker.stop();
        }
    });
}

async function testJSParentPythonChildFactorial() {
    await describe("JS Parent -> Python Child: Factorial", async () => {
        const worker = ParentWorker.spawn(PYTHON_WORKER, "python");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 1500));

            const result = await worker.call.factorial(10);
            assert(result === 3628800, `Expected 3628800, got ${result}`);
        } finally {
            await worker.stop();
        }
    });
}

async function testJSParentPythonChildAsyncMethod() {
    await describe("JS Parent -> Python Child: Async method", async () => {
        const worker = ParentWorker.spawn(PYTHON_WORKER, "python");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 1500));

            const result = await worker.call.async_add(10, 20);
            assert(result === 30, `Expected 30, got ${result}`);
        } finally {
            await worker.stop();
        }
    });
}


// ============================================================================
// TIMEOUT AND CIRCUIT BREAKER TESTS
// ============================================================================

async function testCircuitBreakerState() {
    await describe("Circuit Breaker: State tracking", async () => {
        const worker = ParentWorker.spawn(PYTHON_WORKER, "python");
        
        // Before start, should not be healthy
        assert(!worker.isHealthy, "Should not be healthy before start");
        
        await worker.start();
        await new Promise(r => setTimeout(r, 500));

        // After start, should be healthy
        assert(worker.isHealthy, "Should be healthy after start");
        
        await worker.stop();
        
        // After stop, should not be healthy
        assert(!worker.isHealthy, "Should not be healthy after stop");
    });
}


// ============================================================================
// RUN ALL TESTS
// ============================================================================

async function runAllTests() {
    console.log("Running Multifrost E2E Tests (JavaScript Parent)");
    console.log("=".repeat(60));
    console.log(`Python Worker: ${PYTHON_WORKER}`);
    console.log("=".repeat(60));

    // JS Parent -> JS Child tests
    await testJSParentJSChildBasicCall();
    await testJSParentJSChildVariousTypes();
    await testJSParentJSChildErrorHandling();
    await testJSParentJSChildFactorial();
    await testJSParentJSChildAsyncMethod();

    // JS Parent -> Python Child tests (cross-language!)
    await testJSParentPythonChildBasicCall();
    await testJSParentPythonChildVariousTypes();
    await testJSParentPythonChildErrorHandling();
    await testJSParentPythonChildInfo();
    await testJSParentPythonChildFactorial();
    await testJSParentPythonChildAsyncMethod();

    // Circuit breaker
    await testCircuitBreakerState();

    // Summary
    console.log("\n" + "=".repeat(60));
    console.log(`E2E TEST RESULTS: ${passed} passed, ${failed} failed`);
    console.log("=".repeat(60));
    
    if (failed > 0) {
        process.exit(1);
    }
}

// Run if this is the entry point
runAllTests().catch(console.error);