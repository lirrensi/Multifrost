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

async function main(): Promise<void> {
    const connection = connect("math-service");

    const handle = connection.handle();
    await handle.start();

    try {
        await waitForPeerExists(handle, "math-service");
        const fact = await handle.call.factorial(11);
        console.log({ fact });
    } finally {
        await handle.stop();
    }
}

void main().catch(error => {
    console.error(error);
    process.exitCode = 1;
});
