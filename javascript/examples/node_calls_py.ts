/**
 * FILE: javascript/examples/node_calls_py.ts
 * PURPOSE: Show a Node caller talking to a service peer that already speaks the v5 router protocol.
 * OWNS: Caller-side connect/handle usage for cross-language RPC.
 * EXPORTS: None
 * DOCS: docs/spec.md, javascript/README.md
 */

import { connect } from "../src/index.js";

async function waitForPeerExists(
    handle: { queryPeerExists(peerId: string): Promise<boolean> },
    peerId: string
): Promise<void> {
    const deadline = Date.now() + 10_000;
    while (Date.now() < deadline) {
        if (await handle.queryPeerExists(peerId)) {
            return;
        }
        await new Promise(resolve => setTimeout(resolve, 50));
    }

    throw new Error(`peer ${peerId} did not appear in the router registry`);
}

function resolveTargetPeerId(): string {
    const targetFromEnv = process.env.MULTIFROST_TARGET_PEER_ID?.trim();
    if (targetFromEnv) {
        return targetFromEnv;
    }

    const index = process.argv.indexOf("--target");
    if (index >= 0 && process.argv[index + 1]?.trim()) {
        return process.argv[index + 1].trim();
    }

    return "math-service";
}

async function main(): Promise<void> {
    const target = resolveTargetPeerId();
    const connection = connect(target);

    const handle = connection.handle();
    await handle.start();

    try {
        await waitForPeerExists(handle, target);
        const add = await handle.call.add(10, 20);
        const product = await handle.call.multiply(7, 8);
        const fact = await handle.call.factorial(5);
        console.log(`target = ${target}`);
        console.log(`add(10, 20) = ${add}`);
        console.log(`multiply(7, 8) = ${product}`);
        console.log(`factorial(5) = ${fact}`);
    } finally {
        await handle.stop();
    }
}

void main().catch(error => {
    console.error(error);
    process.exitCode = 1;
});
