/**
 * Minimal E2E Tests for Multifrost - JavaScript Parent
 * Tests cross-language interoperability (JS parent -> Python child)
 * 
 * Run with: npx tsx tests/e2e_minimal.test.ts
 */

import { ParentWorker, RemoteCallError } from "../src/multifrost.js";
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
// JS PARENT -> PYTHON CHILD TESTS (cross-language!)
// ============================================================================

async function testJSParentPythonChildBasicCall() {
    await describe("JS -> Python: Basic add/multiply", async () => {
        const worker = ParentWorker.spawn(PYTHON_WORKER, "python");
        
        try {
            await worker.start();
            // Wait for child to be ready (connection + setup time)
            await new Promise(r => setTimeout(r, 2000));

            const result = await worker.call.add(10, 20);
            assert(result === 30, `add(10, 20) = ${result}, expected 30`);

            const result2 = await worker.call.multiply(5, 6);
            assert(result2 === 30, `multiply(5, 6) = ${result2}, expected 30`);
        } finally {
            // Force stop
            try { await worker.stop(); } catch {}
        }
    });
}

async function testJSParentPythonChildVariousTypes() {
    await describe("JS -> Python: Various types", async () => {
        const worker = ParentWorker.spawn(PYTHON_WORKER, "python");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 2000));

            // String
            let result = await worker.call.echo("hello");
            assert(result === "hello", `echo("hello") = ${result}`);

            // Number
            result = await worker.call.echo(42);
            assert(result === 42, `echo(42) = ${result}`);

            // Float
            result = await worker.call.echo(3.14);
            assert(result === 3.14, `echo(3.14) = ${result}`);

            // Boolean
            result = await worker.call.echo(true);
            assert(result === true, `echo(true) = ${result}`);

            // Array
            result = await worker.call.echo([1, 2, 3]);
            assert(Array.isArray(result), "Should return array");
            assert((result as number[]).length === 3, "Should have 3 elements");

            // Object
            result = await worker.call.echo({ a: 1, b: "test" });
            assert(typeof result === "object", "Should return object");
            assert((result as any).a === 1, "Object should have a=1");
        } finally {
            try { await worker.stop(); } catch {}
        }
    });
}

async function testJSParentPythonChildErrorHandling() {
    await describe("JS -> Python: Error handling", async () => {
        const worker = ParentWorker.spawn(PYTHON_WORKER, "python");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 2000));

            // Test throw_error
            try {
                await worker.call.throw_error("Python test error");
                assert(false, "Should have thrown");
            } catch (e) {
                assert(e instanceof RemoteCallError, "Should be RemoteCallError");
                assert((e as Error).message.includes("Python test error"), 
                    `Error should contain "Python test error", got: ${(e as Error).message}`);
            }

            // Test divide by zero
            try {
                await worker.call.divide(10, 0);
                assert(false, "Should have thrown");
            } catch (e) {
                assert(e instanceof RemoteCallError, "Should be RemoteCallError");
            }
        } finally {
            try { await worker.stop(); } catch {}
        }
    });
}

async function testJSParentPythonChildInfo() {
    await describe("JS -> Python: Get worker info", async () => {
        const worker = ParentWorker.spawn(PYTHON_WORKER, "python");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 2000));

            const info = await worker.call.get_info();
            assert(info.language === "python", `Expected language "python", got ${info.language}`);
            assert(typeof info.pid === "number", "Should have numeric pid");
            assert(typeof info.version === "string", "Should have version string");
        } finally {
            try { await worker.stop(); } catch {}
        }
    });
}

async function testJSParentPythonChildFactorial() {
    await describe("JS -> Python: Factorial computation", async () => {
        const worker = ParentWorker.spawn(PYTHON_WORKER, "python");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 2000));

            const result = await worker.call.factorial(10);
            assert(result === 3628800, `factorial(10) = ${result}, expected 3628800`);
        } finally {
            try { await worker.stop(); } catch {}
        }
    });
}

async function testJSParentPythonChildAsyncMethod() {
    await describe("JS -> Python: Async method", async () => {
        const worker = ParentWorker.spawn(PYTHON_WORKER, "python");
        
        try {
            await worker.start();
            await new Promise(r => setTimeout(r, 2000));

            const result = await worker.call.async_add(10, 20);
            assert(result === 30, `async_add(10, 20) = ${result}, expected 30`);
        } finally {
            try { await worker.stop(); } catch {}
        }
    });
}


// ============================================================================
// RUN ALL TESTS
// ============================================================================

async function runAllTests() {
    console.log("Running Multifrost E2E Tests (JavaScript Parent -> Python Child)");
    console.log("=".repeat(60));
    console.log(`Python Worker: ${PYTHON_WORKER}`);
    console.log("=".repeat(60));

    // JS Parent -> Python Child tests (cross-language!)
    await testJSParentPythonChildBasicCall();
    await testJSParentPythonChildVariousTypes();
    await testJSParentPythonChildErrorHandling();
    await testJSParentPythonChildInfo();
    await testJSParentPythonChildFactorial();
    await testJSParentPythonChildAsyncMethod();

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