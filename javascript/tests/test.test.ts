/**
 * Quick test for Multifrost v4 - spawn and connect modes.
 * Run with: npx tsx tests/test.test.ts
 */

import { ParentWorker, ChildWorker, ServiceRegistry } from "../src/index.js";
import { spawn, ChildProcess } from "child_process";
import { fileURLToPath } from "url";
import { dirname } from "path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

class TestWorker extends ChildWorker {
    add(a: number, b: number): number {
        return a + b;
    }

    async asyncAdd(a: number, b: number): Promise<number> {
        await new Promise(r => setTimeout(r, 100));
        return a + b;
    }
}

class ServiceWorker extends ChildWorker {
    constructor() {
        super("test-service-v4-js");
    }

    multiply(a: number, b: number): number {
        return a * b;
    }
}

// Worker entry point check
if (process.argv.includes("--worker")) {
    new TestWorker().run();
} else if (process.argv.includes("--service-worker")) {
    new ServiceWorker().run();
} else {
    // Main test runner
    (async () => {
        console.log("Running Multifrost v4 JavaScript tests...");
        await testSpawnMode();
        await testConnectMode();
        console.log("\nAll tests passed!");
    })().catch(console.error);
}

async function testSpawnMode(): Promise<void> {
    console.log("\n=== Test: Spawn Mode ===");

    // Spawn ourselves with --worker flag
    const worker = ParentWorker.spawn(`${__filename} --worker`, "npx tsx");
    await worker.start();

    // Wait for connection
    await new Promise(r => setTimeout(r, 500));

    try {
        const result = await worker.call.add(5, 3);
        console.assert(result === 8, `Expected 8, got ${result}`);
        console.log(`  add(5, 3) = ${result} OK`);

        const result2 = await worker.call.asyncAdd(10, 20);
        console.assert(result2 === 30, `Expected 30, got ${result2}`);
        console.log(`  asyncAdd(10, 20) = ${result2} OK`);
    } finally {
        await worker.stop();
    }

    console.log("  Spawn mode: PASSED");
}

async function testConnectMode(): Promise<void> {
    console.log("\n=== Test: Connect Mode ===");

    // Start service worker as subprocess
    const serviceProcess: ChildProcess = spawn("npx", ["tsx", __filename, "--service-worker"], {
        shell: true,
        stdio: "inherit"
    });

    // Wait for service to register
    await new Promise(r => setTimeout(r, 2000));

    try {
        const parent = await ParentWorker.connect("test-service-v4-js", 5000);
        await parent.start();

        // Wait for connection
        await new Promise(r => setTimeout(r, 300));

        const result = await parent.call.multiply(4, 7);
        console.assert(result === 28, `Expected 28, got ${result}`);
        console.log(`  multiply(4, 7) = ${result} OK`);

        await parent.stop();
        console.log("  Connect mode: PASSED");
    } finally {
        serviceProcess.kill();
        await ServiceRegistry.unregister("test-service-v4-js");
    }
}
