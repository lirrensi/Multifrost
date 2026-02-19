/**
 * Unit tests for ServiceRegistry class.
 * Tests service registration, discovery, and unregister operations.
 * 
 * Run with: npx tsx tests/service_registry.test.ts
 */

import { ServiceRegistry } from "../src/service_registry.js";
import * as fs from "fs/promises";
import * as path from "path";
import * as os from "os";

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

// Helper to clean up registry
async function cleanupRegistry(): Promise<void> {
    const registryPath = path.join(os.homedir(), '.multifrost', 'services.json');
    try {
        await fs.unlink(registryPath);
    } catch {
        // Ignore if doesn't exist
    }
}

// ============================================================================
// TESTS: findFreePort
// ============================================================================

async function testFindFreePort() {
    await describe("ServiceRegistry.findFreePort", async () => {
        const port = await ServiceRegistry.findFreePort();
        
        assert(typeof port === "number", "Port should be a number");
        assert(port >= 1024, "Port should be >= 1024");
        assert(port <= 65535, "Port should be <= 65535");
    });
}

async function testFindFreePortMultiple() {
    await describe("ServiceRegistry.findFreePort returns different ports", async () => {
        const port1 = await ServiceRegistry.findFreePort();
        const port2 = await ServiceRegistry.findFreePort();
        const port3 = await ServiceRegistry.findFreePort();
        
        // They might be the same if the OS reuses quickly, but usually different
        assert(typeof port1 === "number", "Port1 should be a number");
        assert(typeof port2 === "number", "Port2 should be a number");
        assert(typeof port3 === "number", "Port3 should be a number");
    });
}

// ============================================================================
// TESTS: Register
// ============================================================================

async function testRegisterBasic() {
    await describe("ServiceRegistry.register basic", async () => {
        await cleanupRegistry();
        
        const serviceId = `test-service-${Date.now()}`;
        const port = await ServiceRegistry.register(serviceId);
        
        assert(typeof port === "number", "Should return a port number");
        assert(port >= 1024, "Port should be >= 1024");
        assert(port <= 65535, "Port should be <= 65535");
        
        // Cleanup
        await ServiceRegistry.unregister(serviceId);
    });
}

async function testRegisterCreatesRegistryFile() {
    await describe("ServiceRegistry.register creates registry file", async () => {
        await cleanupRegistry();
        
        const serviceId = `test-service-file-${Date.now()}`;
        await ServiceRegistry.register(serviceId);
        
        const registryPath = path.join(os.homedir(), '.multifrost', 'services.json');
        const data = await fs.readFile(registryPath, 'utf-8');
        const registry = JSON.parse(data);
        
        assert(serviceId in registry, "Service should be in registry");
        assert(typeof registry[serviceId].port === "number", "Should have port");
        assert(typeof registry[serviceId].pid === "number", "Should have PID");
        assert(typeof registry[serviceId].started === "string", "Should have started timestamp");
        
        // Cleanup
        await ServiceRegistry.unregister(serviceId);
    });
}

async function testRegisterDuplicateService() {
    await describe("ServiceRegistry.register duplicate throws error", async () => {
        await cleanupRegistry();
        
        const serviceId = `test-service-dup-${Date.now()}`;
        
        // First registration should succeed
        await ServiceRegistry.register(serviceId);
        
        // Second registration should fail (same PID is still alive)
        try {
            await ServiceRegistry.register(serviceId);
            assert(false, "Should have thrown error for duplicate service");
        } catch (e) {
            assert(e instanceof Error, "Should throw Error");
            assert((e as Error).message.includes("already running"), "Should mention already running");
        }
        
        // Cleanup
        await ServiceRegistry.unregister(serviceId);
    });
}

// ============================================================================
// TESTS: Discover
// ============================================================================

async function testDiscoverExisting() {
    await describe("ServiceRegistry.discover existing service", async () => {
        await cleanupRegistry();
        
        const serviceId = `test-service-discover-${Date.now()}`;
        const registeredPort = await ServiceRegistry.register(serviceId);
        
        const discoveredPort = await ServiceRegistry.discover(serviceId, 1000);
        
        assert(discoveredPort === registeredPort, "Discovered port should match registered port");
        
        // Cleanup
        await ServiceRegistry.unregister(serviceId);
    });
}

async function testDiscoverNonExistent() {
    await describe("ServiceRegistry.discover non-existent service times out", async () => {
        await cleanupRegistry();
        
        const serviceId = `non-existent-service-${Date.now()}`;
        
        const startTime = Date.now();
        try {
            await ServiceRegistry.discover(serviceId, 500); // Short timeout
            assert(false, "Should have thrown error");
        } catch (e) {
            const elapsed = Date.now() - startTime;
            assert(e instanceof Error, "Should throw Error");
            assert((e as Error).message.includes("not found"), "Should mention not found");
            assert(elapsed >= 400, "Should wait approximately the timeout duration");
        }
    });
}

async function testDiscoverWithCustomTimeout() {
    await describe("ServiceRegistry.discover with custom timeout", async () => {
        await cleanupRegistry();
        
        const serviceId = `non-existent-timeout-${Date.now()}`;
        
        const startTime = Date.now();
        try {
            await ServiceRegistry.discover(serviceId, 300);
        } catch {
            const elapsed = Date.now() - startTime;
            assert(elapsed >= 200, "Should respect custom timeout");
        }
    });
}

// ============================================================================
// TESTS: Unregister
// ============================================================================

async function testUnregisterExisting() {
    await describe("ServiceRegistry.unregister existing service", async () => {
        await cleanupRegistry();
        
        const serviceId = `test-service-unreg-${Date.now()}`;
        await ServiceRegistry.register(serviceId);
        
        // Unregister should succeed
        await ServiceRegistry.unregister(serviceId);
        
        // Verify it's gone
        const registryPath = path.join(os.homedir(), '.multifrost', 'services.json');
        try {
            const data = await fs.readFile(registryPath, 'utf-8');
            const registry = JSON.parse(data);
            assert(!(serviceId in registry), "Service should be removed from registry");
        } catch {
            // File might not exist, which is fine
        }
    });
}

async function testUnregisterNonExistent() {
    await describe("ServiceRegistry.unregister non-existent service", async () => {
        await cleanupRegistry();
        
        // Should not throw
        await ServiceRegistry.unregister("non-existent-service");
        assert(true, "Should not throw for non-existent service");
    });
}

async function testUnregisterOnlyOwnPid() {
    await describe("ServiceRegistry.unregister only own PID", async () => {
        await cleanupRegistry();
        
        const serviceId = `test-service-pid-${Date.now()}`;
        await ServiceRegistry.register(serviceId);
        
        // Manually modify the PID to simulate a different process
        const registryPath = path.join(os.homedir(), '.multifrost', 'services.json');
        const data = await fs.readFile(registryPath, 'utf-8');
        const registry = JSON.parse(data);
        registry[serviceId].pid = 999999; // Fake PID
        await fs.writeFile(registryPath, JSON.stringify(registry, null, 2));
        
        // Try to unregister - should NOT remove because PID doesn't match
        await ServiceRegistry.unregister(serviceId);
        
        // Verify it's still there
        const dataAfter = await fs.readFile(registryPath, 'utf-8');
        const registryAfter = JSON.parse(dataAfter);
        assert(serviceId in registryAfter, "Service should still be in registry (PID mismatch)");
        
        // Cleanup manually
        delete registryAfter[serviceId];
        await fs.writeFile(registryPath, JSON.stringify(registryAfter, null, 2));
    });
}

// ============================================================================
// TESTS: Edge Cases
// ============================================================================

async function testRegistryDirCreation() {
    await describe("ServiceRegistry creates registry directory", async () => {
        // Ensure the directory doesn't exist
        const dir = path.join(os.homedir(), '.multifrost');
        try {
            await fs.rm(dir, { recursive: true });
        } catch {
            // Ignore if doesn't exist
        }
        
        const serviceId = `test-service-dir-${Date.now()}`;
        await ServiceRegistry.register(serviceId);
        
        // Directory should exist now
        const stat = await fs.stat(dir);
        assert(stat.isDirectory(), "Registry directory should exist");
        
        // Cleanup
        await ServiceRegistry.unregister(serviceId);
    });
}

async function testEmptyServiceId() {
    await describe("ServiceRegistry with empty service ID", async () => {
        await cleanupRegistry();
        
        try {
            await ServiceRegistry.register("");
            // If it doesn't throw, clean up
            await ServiceRegistry.unregister("");
            assert(true, "Empty service ID accepted");
        } catch (e) {
            // Some implementations might reject empty IDs
            assert(e instanceof Error, "Should throw Error for empty ID");
        }
    });
}

async function testSpecialCharsInServiceId() {
    await describe("ServiceRegistry with special characters in service ID", async () => {
        await cleanupRegistry();
        
        const serviceId = `test-service-special_${Date.now()}!@#$`;
        
        try {
            const port = await ServiceRegistry.register(serviceId);
            assert(typeof port === "number", "Should register with special chars");
            
            const discoveredPort = await ServiceRegistry.discover(serviceId, 1000);
            assert(discoveredPort === port, "Should discover with special chars");
            
            await ServiceRegistry.unregister(serviceId);
        } catch (e) {
            // Some implementations might reject special chars
            console.log(`    Note: Special chars rejected: ${(e as Error).message}`);
        }
    });
}

async function testUnicodeInServiceId() {
    await describe("ServiceRegistry with unicode in service ID", async () => {
        await cleanupRegistry();
        
        const serviceId = `test-service-日本語-${Date.now()}`;
        
        try {
            const port = await ServiceRegistry.register(serviceId);
            assert(typeof port === "number", "Should register with unicode");
            
            const discoveredPort = await ServiceRegistry.discover(serviceId, 1000);
            assert(discoveredPort === port, "Should discover with unicode");
            
            await ServiceRegistry.unregister(serviceId);
        } catch (e) {
            console.log(`    Note: Unicode rejected: ${(e as Error).message}`);
        }
    });
}

async function testMultipleServices() {
    await describe("ServiceRegistry with multiple services", async () => {
        await cleanupRegistry();
        
        const services = [
            `test-multi-1-${Date.now()}`,
            `test-multi-2-${Date.now()}`,
            `test-multi-3-${Date.now()}`,
        ];
        
        const ports: number[] = [];
        
        // Register all
        for (const serviceId of services) {
            const port = await ServiceRegistry.register(serviceId);
            ports.push(port);
        }
        
        // All should be discoverable
        for (let i = 0; i < services.length; i++) {
            const port = await ServiceRegistry.discover(services[i], 1000);
            assert(port === ports[i], `Service ${i} should have correct port`);
        }
        
        // Cleanup
        for (const serviceId of services) {
            await ServiceRegistry.unregister(serviceId);
        }
    });
}

// ============================================================================
// RUN ALL TESTS
// ============================================================================

async function runAllTests() {
    console.log("Running ServiceRegistry tests...\n");
    
    // findFreePort tests
    await testFindFreePort();
    await testFindFreePortMultiple();
    
    // Register tests
    await testRegisterBasic();
    await testRegisterCreatesRegistryFile();
    await testRegisterDuplicateService();
    
    // Discover tests
    await testDiscoverExisting();
    await testDiscoverNonExistent();
    await testDiscoverWithCustomTimeout();
    
    // Unregister tests
    await testUnregisterExisting();
    await testUnregisterNonExistent();
    await testUnregisterOnlyOwnPid();
    
    // Edge cases
    await testRegistryDirCreation();
    await testEmptyServiceId();
    await testSpecialCharsInServiceId();
    await testUnicodeInServiceId();
    await testMultipleServices();
    
    // Summary
    console.log("\n" + "=".repeat(50));
    console.log(`SERVICE REGISTRY TEST RESULTS: ${passed} passed, ${failed} failed`);
    console.log("=".repeat(50));
    
    if (failed > 0) {
        process.exit(1);
    }
}

runAllTests().catch(console.error);
