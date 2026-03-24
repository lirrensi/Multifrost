/**
 * FILE: javascript/tests/v5_interop.test.ts
 * PURPOSE: Exercise Node-to-Rust and Rust-to-Node interoperability over the v5 router surface.
 * OWNS: Cross-language process orchestration, example-based interop checks, and exit-code validation.
 * EXPORTS: None
 * DOCS: docs/spec.md, docs/msgpack_interop.md, agent_chat/node_v5_api_surface_2026-03-24.md
 */

import assert from "node:assert/strict";
import { type ChildProcess } from "node:child_process";
import { connect } from "../src/connection.js";
import { examplePath, getFreePort, javascriptRoot, rustRoot, spawnBinary, waitForExit, waitForPeerExists } from "./v5_test_support.js";
import { ensureRouterBinaryEnv } from "./v5_test_support.js";
import { spawn } from "../src/process.js";

type TestFn = () => Promise<void> | void;

async function run(name: string, fn: TestFn): Promise<void> {
    try {
        await fn();
        console.log(`ok - ${name}`);
    } catch (error) {
        console.error(`not ok - ${name}`);
        throw error;
    }
}

function collectOutput(child: ChildProcess): { stdout: string; stderr: string } {
    const output = { stdout: "", stderr: "" };

    child.stdout?.setEncoding("utf8");
    child.stderr?.setEncoding("utf8");
    child.stdout?.on("data", chunk => {
        output.stdout += String(chunk);
    });
    child.stderr?.on("data", chunk => {
        output.stderr += String(chunk);
    });

    return output;
}

async function spawnNodeMathService(routerPort: number) {
    ensureRouterBinaryEnv();
    const serviceEntrypoint = examplePath("math_worker_service.ts");
    return await spawn(
        serviceEntrypoint,
        `node ./node_modules/tsx/dist/cli.mjs ${serviceEntrypoint}`,
        {
            cwd: javascriptRoot(),
            routerPort,
        }
    );
}

async function testNodeCallerToRustService(): Promise<void> {
    const routerPort = await getFreePort();
    const routerBin = ensureRouterBinaryEnv();
    const rustService = spawnBinary(
        "cargo",
        ["run", "--quiet", "--example", "e2e_math_worker", "--", "--service"],
        {
            cwd: rustRoot(),
            env: {
                MULTIFROST_ROUTER_PORT: String(routerPort),
                MULTIFROST_ROUTER_BIN: routerBin,
            },
        }
    );

    const connection = connect("math-service", {
        routerPort,
    });
    const handle = connection.handle();

    try {
        await handle.start();
        await waitForPeerExists(handle, "math-service");

        assert.equal(await handle.call.add(10, 20), 30);
        assert.equal(await handle.call.multiply(7, 8), 56);
        assert.equal(await handle.call.factorial(5), 120);
    } finally {
        await handle.stop().catch(() => undefined);
        rustService.kill();
        await waitForExit(rustService).catch(() => undefined);
    }
}

async function testRustCallerToNodeService(): Promise<void> {
    const routerPort = await getFreePort();
    const routerBin = ensureRouterBinaryEnv();
    const nodeService = await spawnNodeMathService(routerPort);
    const probe = connect("math-service", { routerPort });
    const probeHandle = probe.handle();

    try {
        await probeHandle.start();
        await waitForPeerExists(probeHandle, "math-service");

        const rustClient = spawnBinary(
            "cargo",
            ["run", "--quiet", "--example", "connect", "--", "--target", "math-service", "--timeout-ms", "10000"],
            {
                cwd: rustRoot(),
                env: {
                    MULTIFROST_ROUTER_PORT: String(routerPort),
                    MULTIFROST_ROUTER_BIN: routerBin,
                },
            }
        );
        const output = collectOutput(rustClient);
        const exit = await waitForExit(rustClient);

        assert.equal(exit.code, 0);
        assert.match(output.stdout, /factorial\(5\) = 120/);
    } finally {
        await probeHandle.stop().catch(() => undefined);
        await nodeService.stop().catch(() => undefined);
    }
}

async function main(): Promise<void> {
    await run("Node caller to Rust service", testNodeCallerToRustService);
    await run("Rust caller to Node service", testRustCallerToNodeService);
}

void main().catch(error => {
    console.error(error);
    process.exitCode = 1;
});
