/**
 * Unit tests for ComlinkMessage class.
 * Tests message creation, serialization, and edge cases.
 * 
 * Run with: npx tsx tests/message.test.ts
 */

import {
    ComlinkMessage,
    MessageType,
    RemoteCallError,
    CircuitOpenError,
} from "../src/multifrost.js";

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
// TESTS: ComlinkMessage Construction
// ============================================================================

describe("ComlinkMessage: Basic Construction", () => {
    const msg = new ComlinkMessage();
    
    assert(msg.app === "comlink_ipc_v4", "Default app name should be set");
    assert(typeof msg.id === "string", "ID should be a string");
    assert(msg.id.length > 0, "ID should not be empty");
    assert(typeof msg.timestamp === "number", "Timestamp should be a number");
    assert(msg.timestamp > 0, "Timestamp should be positive");
});

describe("ComlinkMessage: Construction with Data", () => {
    const msg = new ComlinkMessage({
        id: "test-id-123",
        type: MessageType.CALL,
        function: "add",
        args: [1, 2, 3],
        namespace: "math",
    });
    
    assert(msg.id === "test-id-123", "ID should match provided value");
    assert(msg.type === MessageType.CALL, "Type should match");
    assert(msg.function === "add", "Function should match");
    assert(Array.isArray(msg.args), "Args should be an array");
    assert(msg.args?.length === 3, "Args should have 3 elements");
    assert(msg.namespace === "math", "Namespace should match");
});

describe("ComlinkMessage: Custom ID in Constructor", () => {
    const customId = "my-custom-uuid";
    const msg = new ComlinkMessage({ id: customId });
    
    assert(msg.id === customId, "Should use provided ID");
});

// ============================================================================
// TESTS: Message Type Creation
// ============================================================================

describe("ComlinkMessage.createCall", () => {
    const msg = ComlinkMessage.createCall("multiply", [5, 10], "math");
    
    assert(msg.type === MessageType.CALL, "Type should be CALL");
    assert(msg.function === "multiply", "Function should match");
    assert(Array.isArray(msg.args), "Args should be array");
    assert(msg.args?.[0] === 5, "First arg should be 5");
    assert(msg.args?.[1] === 10, "Second arg should be 10");
    assert(msg.namespace === "math", "Namespace should be math");
    assert(typeof msg.id === "string", "Should have auto-generated ID");
});

describe("ComlinkMessage.createCall with custom ID", () => {
    const customId = "call-xyz";
    const msg = ComlinkMessage.createCall("test", [], "default", customId, "client1");
    
    assert(msg.id === customId, "Should use provided ID");
    assert(msg.clientName === "client1", "Should have client name");
});

describe("ComlinkMessage.createResponse", () => {
    const result = { status: "ok", value: 42 };
    const msg = ComlinkMessage.createResponse(result, "req-id-123");
    
    assert(msg.type === MessageType.RESPONSE, "Type should be RESPONSE");
    assert(msg.id === "req-id-123", "ID should match request ID");
    assert(msg.result === result, "Result should match");
});

describe("ComlinkMessage.createResponse with primitives", () => {
    // Test with various result types
    const msg1 = ComlinkMessage.createResponse(42, "req-1");
    assert(msg1.result === 42, "Should handle number result");
    
    const msg2 = ComlinkMessage.createResponse("hello", "req-2");
    assert(msg2.result === "hello", "Should handle string result");
    
    const msg3 = ComlinkMessage.createResponse(null, "req-3");
    assert(msg3.result === null, "Should handle null result");
    
    const msg4 = ComlinkMessage.createResponse(true, "req-4");
    assert(msg4.result === true, "Should handle boolean result");
});

describe("ComlinkMessage.createError", () => {
    const errorMsg = "Division by zero";
    const msg = ComlinkMessage.createError(errorMsg, "req-id-456");
    
    assert(msg.type === MessageType.ERROR, "Type should be ERROR");
    assert(msg.id === "req-id-456", "ID should match request ID");
    assert(msg.error === errorMsg, "Error message should match");
});

describe("ComlinkMessage.createOutput", () => {
    const output = "Hello, World!";
    const msg = ComlinkMessage.createOutput(output, MessageType.STDOUT);
    
    assert(msg.type === MessageType.STDOUT, "Type should be STDOUT");
    assert(msg.output === output, "Output should match");
});

// ============================================================================
// TESTS: Serialization (pack/unpack)
// ============================================================================

describe("ComlinkMessage: Pack/Unpack Roundtrip", () => {
    const original = ComlinkMessage.createCall("add", [1, 2, 3], "math");
    const packed = original.pack();
    
    assert(Buffer.isBuffer(packed), "Pack should return Buffer");
    assert(packed.length > 0, "Packed data should not be empty");
    
    const unpacked = ComlinkMessage.unpack(packed);
    
    assert(unpacked.id === original.id, "ID should match after roundtrip");
    assert(unpacked.type === original.type, "Type should match after roundtrip");
    assert(unpacked.function === original.function, "Function should match after roundtrip");
    assert(unpacked.namespace === original.namespace, "Namespace should match after roundtrip");
});

describe("ComlinkMessage: Pack/Unpack with Complex Args", () => {
    const complexArgs = [
        { name: "test", values: [1, 2, 3] },
        [1, [2, [3]]],
        "string",
        123,
        true,
        null,
    ];
    const original = ComlinkMessage.createCall("complex", complexArgs, "test");
    const unpacked = ComlinkMessage.unpack(original.pack());
    
    assert(Array.isArray(unpacked.args), "Args should be array");
    assert(unpacked.args?.length === complexArgs.length, "Args length should match");
    assert(unpacked.args?.[0]?.name === "test", "Nested object should match");
});

describe("ComlinkMessage: Pack/Unpack Response", () => {
    const result = { nested: { deep: { value: 42 } } };
    const original = ComlinkMessage.createResponse(result, "req-789");
    const unpacked = ComlinkMessage.unpack(original.pack());
    
    assert(unpacked.type === MessageType.RESPONSE, "Type should be RESPONSE");
    assert(unpacked.id === "req-789", "ID should match");
    assert(unpacked.result?.nested?.deep?.value === 42, "Nested result should match");
});

describe("ComlinkMessage: Pack/Unpack Error", () => {
    const original = ComlinkMessage.createError("Something went wrong", "req-error");
    const unpacked = ComlinkMessage.unpack(original.pack());
    
    assert(unpacked.type === MessageType.ERROR, "Type should be ERROR");
    assert(unpacked.error === "Something went wrong", "Error should match");
});

// ============================================================================
// TESTS: Edge Cases
// ============================================================================

describe("ComlinkMessage: Empty Args", () => {
    const msg = ComlinkMessage.createCall("noArgs", [], "default");
    const unpacked = ComlinkMessage.unpack(msg.pack());
    
    assert(Array.isArray(unpacked.args), "Args should be array");
    assert(unpacked.args?.length === 0, "Args should be empty");
});

describe("ComlinkMessage: Undefined Args", () => {
    const msg = ComlinkMessage.createCall("noArgs", undefined as any, "default");
    const packed = msg.pack();
    
    assert(Buffer.isBuffer(packed), "Should pack successfully");
});

describe("ComlinkMessage: Unicode in Function Name", () => {
    const msg = ComlinkMessage.createCall("関数", [1, 2], "test");
    const unpacked = ComlinkMessage.unpack(msg.pack());
    
    assert(unpacked.function === "関数", "Unicode function name should survive");
});

describe("ComlinkMessage: Emoji in Args", () => {
    const msg = ComlinkMessage.createCall("emoji", ["🔥", "❄️", "🌐"], "test");
    const unpacked = ComlinkMessage.unpack(msg.pack());
    
    assert(unpacked.args?.[0] === "🔥", "Emoji should survive");
    assert(unpacked.args?.[1] === "❄️", "Emoji should survive");
    assert(unpacked.args?.[2] === "🌐", "Emoji should survive");
});

describe("ComlinkMessage: Large Payload", () => {
    // Create a large payload (100KB)
    const largeString = "x".repeat(100000);
    const msg = ComlinkMessage.createCall("large", [largeString], "test");
    const packed = msg.pack();
    const unpacked = ComlinkMessage.unpack(packed);
    
    assert(unpacked.args?.[0] === largeString, "Large payload should survive");
    assert(packed.length > 100000, "Packed size should be substantial");
});

describe("ComlinkMessage: NaN Handling", () => {
    // NaN should be sanitized to null
    const msg = ComlinkMessage.createCall("test", [NaN, Infinity, -Infinity], "test");
    const unpacked = ComlinkMessage.unpack(msg.pack());
    
    assert(unpacked.args?.[0] === null, "NaN should be null");
    assert(unpacked.args?.[1] === null, "Infinity should be null");
    assert(unpacked.args?.[2] === null, "-Infinity should be null");
});

describe("ComlinkMessage: Invalid Unpack Data", () => {
    try {
        ComlinkMessage.unpack(Buffer.from("invalid msgpack data"));
        assert(false, "Should have thrown an error");
    } catch (e) {
        assert(e instanceof Error, "Should throw Error");
        assert((e as Error).message.includes("Failed to unpack"), "Should have helpful message");
    }
});

describe("ComlinkMessage: toDict", () => {
    const msg = new ComlinkMessage({
        id: "test-id",
        type: MessageType.CALL,
        function: "test",
        args: [1, 2],
    });
    const dict = msg.toDict();
    
    assert(dict.app === "comlink_ipc_v4", "Should have app");
    assert(dict.id === "test-id", "Should have id");
    assert(dict.type === MessageType.CALL, "Should have type");
    assert(dict.function === "test", "Should have function");
    assert(Array.isArray(dict.args), "Should have args");
});

describe("ComlinkMessage: toDict excludes undefined", () => {
    const msg = new ComlinkMessage({
        id: "test-id",
        type: MessageType.RESPONSE,
    });
    const dict = msg.toDict();
    
    assert(dict.result === undefined, "Should not include undefined result");
    assert(dict.error === undefined, "Should not include undefined error");
    assert(!("result" in dict), "Should not have result key");
    assert(!("error" in dict), "Should not have error key");
});

// ============================================================================
// TESTS: Message Types Enum
// ============================================================================

describe("MessageType enum values", () => {
    assert(MessageType.CALL === "call", "CALL should be 'call'");
    assert(MessageType.RESPONSE === "response", "RESPONSE should be 'response'");
    assert(MessageType.ERROR === "error", "ERROR should be 'error'");
    assert(MessageType.STDOUT === "stdout", "STDOUT should be 'stdout'");
    assert(MessageType.STDERR === "stderr", "STDERR should be 'stderr'");
    assert(MessageType.HEARTBEAT === "heartbeat", "HEARTBEAT should be 'heartbeat'");
    assert(MessageType.SHUTDOWN === "shutdown", "SHUTDOWN should be 'shutdown'");
});

// ============================================================================
// TESTS: Custom Errors
// ============================================================================

describe("RemoteCallError", () => {
    const error = new RemoteCallError("Function failed");
    
    assert(error instanceof Error, "Should be Error");
    assert(error.name === "RemoteCallError", "Should have correct name");
    assert(error.message === "Function failed", "Should have message");
});

describe("CircuitOpenError", () => {
    const error = new CircuitOpenError("Circuit breaker tripped");
    
    assert(error instanceof Error, "Should be Error");
    assert(error.name === "CircuitOpenError", "Should have correct name");
    assert(error.message === "Circuit breaker tripped", "Should have message");
});

// ============================================================================
// SUMMARY
// ============================================================================

console.log("\n" + "=".repeat(50));
console.log(`MESSAGE TEST RESULTS: ${passed} passed, ${failed} failed`);
console.log("=".repeat(50));

if (failed > 0) {
    process.exit(1);
}
