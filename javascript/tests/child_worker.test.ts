/**
 * Unit tests for ChildWorker class.
 * Tests method dispatch, message handling, namespaces, and lifecycle.
 * 
 * Run with: npx tsx tests/child_worker.test.ts
 */

import {
    ChildWorker,
    ParentWorker,
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
// TEST WORKER CLASSES
// ============================================================================

class BasicChildWorker extends ChildWorker {
    add(a: number, b: number): number {
        return a + b;
    }
    
    multiply(a: number, b: number): number {
        return a * b;
    }
    
    greet(name: string): string {
        return `Hello, ${name}!`;
    }
    
    echo(value: unknown): unknown {
        return value;
    }
    
    async asyncOperation(ms: number): Promise<string> {
        await new Promise(r => setTimeout(r, ms));
        return `Waited ${ms}ms`;
    }
    
    throwError(message: string): never {
        throw new Error(message);
    }
    
    private privateMethod(): string {
        return "private";
    }
    
    _underscoreMethod(): string {
        return "underscore";
    }
}

class NamespacedWorker extends ChildWorker {
    protected namespace: string = "custom-namespace";
    
    specialAdd(a: number, b: number): number {
        return a + b + 1000;
    }
}

class ServiceIdWorker extends ChildWorker {
    constructor() {
        super("test-child-worker-service");
    }
    
    serviceMethod(): string {
        return "from-service";
    }
}

// Worker entry points
if (process.argv.includes("--basic-child-worker")) {
    new BasicChildWorker().run();
} else if (process.argv.includes("--namespaced-worker")) {
    new NamespacedWorker().run();
} else if (process.argv.includes("--service-worker")) {
    new ServiceIdWorker().run();
}

// ============================================================================
// TESTS: listFunctions
// ============================================================================

async function testListFunctions() {
    await describe("ChildWorker: listFunctions returns public methods", async () => {
        const worker = new BasicChildWorker();
        const functions = worker.listFunctions();
        
        assert(functions.includes("add"), "Should include 'add'");
        assert(functions.includes("multiply"), "Should include 'multiply'");
        assert(functions.includes("greet"), "Should include 'greet'");
        assert(functions.includes("echo"), "Should include 'echo'");
        assert(functions.includes("asyncOperation"), "Should include 'asyncOperation'");
        assert(functions.includes("throwError"), "Should include 'throwError'");
    });
}

async function testListFunctionsExcludesPrivate() {
    await describe("ChildWorker: listFunctions excludes private methods", async () => {
        const worker = new BasicChildWorker();
        const functions = worker.listFunctions();
        
        assert(!functions.includes("privateMethod"), "Should not include 'privateMethod'");
        assert(!functions.includes("_underscoreMethod"), "Should not include '_underscoreMethod'");
    });
}

async function testListFunctionsExcludesBaseMethods() {
    await describe("ChildWorker: listFunctions excludes base class methods", async () => {
        const worker = new BasicChildWorker();
        const functions = worker.listFunctions();
        
        assert(!functions.includes("start"), "Should not include 'start'");
        assert(!functions.includes("stop"), "Should not include 'stop'");
        assert(!functions.includes("listFunctions"), "Should not include 'listFunctions'");
        assert(!functions.includes("run"), "Should not include 'run'");
    });
}

// ============================================================================
// TESTS: Method Dispatch via ParentWorker
// ============================================================================

async function testBasicMethodDispatch() {
    await describe("ChildWorker: Basic method dispatch", async () => {
        const parent = ParentWorker.spawn(`${__filename} --basic-child-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            const result = await parent.call.add(10, 20);
            assert(result === 30, "add(10, 20) should return 30");
            
            const result2 = await parent.call.multiply(5, 6);
            assert(result2 === 30, "multiply(5, 6) should return 30");
        } finally {
            await parent.stop();
        }
    });
}

async function testStringMethodDispatch() {
    await describe("ChildWorker: String method dispatch", async () => {
        const parent = ParentWorker.spawn(`${__filename} --basic-child-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            const result = await parent.call.greet("World");
            assert(result === "Hello, World!", "greet('World') should return 'Hello, World!'");
        } finally {
            await parent.stop();
        }
    });
}

async function testAsyncMethodDispatch() {
    await describe("ChildWorker: Async method dispatch", async () => {
        const parent = ParentWorker.spawn(`${__filename} --basic-child-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            const result = await parent.call.asyncOperation(100);
            assert(result === "Waited 100ms", "Should return after async operation");
        } finally {
            await parent.stop();
        }
    });
}

async function testErrorMethodDispatch() {
    await describe("ChildWorker: Error method dispatch", async () => {
        const parent = ParentWorker.spawn(`${__filename} --basic-child-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            try {
                await parent.call.throwError("Custom error");
                assert(false, "Should have thrown");
            } catch (e) {
                assert((e as Error).message.includes("Custom error"), "Should have error message");
            }
        } finally {
            await parent.stop();
        }
    });
}

async function testPrivateMethodBlocked() {
    await describe("ChildWorker: Private method is blocked", async () => {
        const parent = ParentWorker.spawn(`${__filename} --basic-child-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            try {
                // Try to call private method
                await parent.callFunction("privateMethod", []);
                assert(false, "Should have thrown");
            } catch (e) {
                assert((e as Error).message.includes("private") || 
                       (e as Error).message.includes("not found"), 
                       "Should reject private method");
            }
        } finally {
            await parent.stop();
        }
    });
}

async function testUnderscoreMethodBlocked() {
    await describe("ChildWorker: Underscore method is blocked", async () => {
        const parent = ParentWorker.spawn(`${__filename} --basic-child-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            try {
                await parent.callFunction("_underscoreMethod", []);
                assert(false, "Should have thrown");
            } catch (e) {
                assert((e as Error).message.includes("private") || 
                       (e as Error).message.includes("Cannot call private"), 
                       "Should reject underscore method");
            }
        } finally {
            await parent.stop();
        }
    });
}

async function testNonExistentMethod() {
    await describe("ChildWorker: Non-existent method returns error", async () => {
        const parent = ParentWorker.spawn(`${__filename} --basic-child-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            try {
                await parent.call.nonExistentMethod();
                assert(false, "Should have thrown");
            } catch (e) {
                assert((e as Error).message.includes("not found") || 
                       (e as Error).message.includes("not callable"), 
                       "Should indicate method not found");
            }
        } finally {
            await parent.stop();
        }
    });
}

// ============================================================================
// TESTS: Namespace
// ============================================================================

async function testNamespaceFiltering() {
    await describe("ChildWorker: Namespace filtering", async () => {
        const parent = ParentWorker.spawn(`${__filename} --namespaced-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // Default namespace should NOT work
            try {
                await parent.call.specialAdd(10, 20);
                assert(false, "Should have failed with wrong namespace");
            } catch (e) {
                // Expected - namespace mismatch
            }
            
            // Correct namespace should work
            const result = await parent.callFunction(
                "specialAdd",
                [10, 20],
                undefined,
                "custom-namespace"
            );
            assert(result === 1030, "Should work with correct namespace");
        } finally {
            await parent.stop();
        }
    });
}

// ============================================================================
// TESTS: Argument Types
// ============================================================================

async function testVariousArgTypes() {
    await describe("ChildWorker: Various argument types", async () => {
        const parent = ParentWorker.spawn(`${__filename} --basic-child-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // String
            const r1 = await parent.call.echo("test string");
            assert(r1 === "test string", "Should echo string");
            
            // Number
            const r2 = await parent.call.echo(42);
            assert(r2 === 42, "Should echo number");
            
            // Object
            const obj = { a: 1, b: { c: 2 } };
            const r3 = await parent.call.echo(obj);
            assert((r3 as any).a === 1 && (r3 as any).b.c === 2, "Should echo object");
            
            // Array
            const arr = [1, 2, 3, "four"];
            const r4 = await parent.call.echo(arr);
            assert(Array.isArray(r4) && r4.length === 4, "Should echo array");
            
            // null
            const r5 = await parent.call.echo(null);
            assert(r5 === null, "Should echo null");
            
            // Boolean
            const r6 = await parent.call.echo(true);
            assert(r6 === true, "Should echo boolean");
        } finally {
            await parent.stop();
        }
    });
}

async function testEmptyArgs() {
    await describe("ChildWorker: Empty arguments", async () => {
        const parent = ParentWorker.spawn(`${__filename} --basic-child-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // Method with no args
            const result = await parent.callFunction("echo", []);
            assert(result === undefined, "Should return undefined for no args echo");
        } finally {
            await parent.stop();
        }
    });
}

async function testUnicodeArgs() {
    await describe("ChildWorker: Unicode arguments", async () => {
        const parent = ParentWorker.spawn(`${__filename} --basic-child-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            const unicode = "日本語 🎉 emoji";
            const result = await parent.call.echo(unicode);
            assert(result === unicode, "Should preserve unicode");
        } finally {
            await parent.stop();
        }
    });
}

async function testLargePayload() {
    await describe("ChildWorker: Large payload", async () => {
        const parent = ParentWorker.spawn(`${__filename} --basic-child-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // 50KB payload
            const largeString = "x".repeat(50000);
            const result = await parent.call.echo(largeString);
            assert(result === largeString, "Should handle large payload");
        } finally {
            await parent.stop();
        }
    });
}

// ============================================================================
// TESTS: Multiple Concurrent Calls
// ============================================================================

async function testConcurrentCalls() {
    await describe("ChildWorker: Concurrent calls", async () => {
        const parent = ParentWorker.spawn(`${__filename} --basic-child-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // Make 10 concurrent calls
            const promises = [];
            for (let i = 0; i < 10; i++) {
                promises.push(parent.call.add(i, i * 10));
            }
            
            const results = await Promise.all(promises);
            
            for (let i = 0; i < 10; i++) {
                assert(results[i] === i + i * 10, `Result ${i} should be correct`);
            }
        } finally {
            await parent.stop();
        }
    });
}

async function testMixedConcurrentCalls() {
    await describe("ChildWorker: Mixed concurrent calls", async () => {
        const parent = ParentWorker.spawn(`${__filename} --basic-child-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // Mix of add, multiply, greet
            const results = await Promise.all([
                parent.call.add(1, 2),
                parent.call.multiply(3, 4),
                parent.call.greet("Test"),
                parent.call.add(10, 20),
                parent.call.multiply(5, 6),
            ]);
            
            assert(results[0] === 3, "add should work");
            assert(results[1] === 12, "multiply should work");
            assert(results[2] === "Hello, Test!", "greet should work");
            assert(results[3] === 30, "add should work");
            assert(results[4] === 30, "multiply should work");
        } finally {
            await parent.stop();
        }
    });
}

// ============================================================================
// TESTS: Lifecycle
// ============================================================================

async function testWorkerStop() {
    await describe("ChildWorker: Stop functionality", async () => {
        const parent = ParentWorker.spawn(`${__filename} --basic-child-worker`, "npx tsx");
        await parent.start();
        await new Promise(r => setTimeout(r, 500));
        
        try {
            // Make a call to verify it works
            const result = await parent.call.add(1, 1);
            assert(result === 2, "Should work before stop");
        } finally {
            await parent.stop();
        }
        
        // After stop, new calls should fail
        try {
            await parent.call.add(1, 1);
            assert(false, "Should fail after stop");
        } catch {
            // Expected
        }
    });
}

// ============================================================================
// RUN ALL TESTS
// ============================================================================

async function runAllTests() {
    console.log("Running ChildWorker tests...\n");
    
    // listFunctions tests
    await testListFunctions();
    await testListFunctionsExcludesPrivate();
    await testListFunctionsExcludesBaseMethods();
    
    // Method dispatch tests
    await testBasicMethodDispatch();
    await testStringMethodDispatch();
    await testAsyncMethodDispatch();
    await testErrorMethodDispatch();
    await testPrivateMethodBlocked();
    await testUnderscoreMethodBlocked();
    await testNonExistentMethod();
    
    // Namespace tests
    await testNamespaceFiltering();
    
    // Argument type tests
    await testVariousArgTypes();
    await testEmptyArgs();
    await testUnicodeArgs();
    await testLargePayload();
    
    // Concurrency tests
    await testConcurrentCalls();
    await testMixedConcurrentCalls();
    
    // Lifecycle tests
    await testWorkerStop();
    
    // Summary
    console.log("\n" + "=".repeat(50));
    console.log(`CHILD WORKER TEST RESULTS: ${passed} passed, ${failed} failed`);
    console.log("=".repeat(50));
    
    if (failed > 0) {
        process.exit(1);
    }
}

// Check if we're running as a worker
if (process.argv.includes("--basic-child-worker") ||
    process.argv.includes("--namespaced-worker") ||
    process.argv.includes("--service-worker")) {
    // Worker mode - handled by worker classes above
} else {
    runAllTests().catch(console.error);
}
