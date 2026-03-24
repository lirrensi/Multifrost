/**
 * FILE: javascript/examples/node_calls_node.ts
 * PURPOSE: Show a Node caller launching a Node service peer and then calling it through the router.
 * OWNS: spawn(...) plus connect()/handle() in the same flow.
 * EXPORTS: None
 * DOCS: docs/spec.md, javascript/README.md
 */

import { fileURLToPath } from "url";
import { connect, spawn } from "../src/index.js";

async function main(): Promise<void> {
    const serviceEntrypoint = fileURLToPath(new URL("./math_worker_service.ts", import.meta.url));
    const executable = `npx tsx ${serviceEntrypoint}`;
    const serviceProcess = await spawn(serviceEntrypoint, executable);

    try {
        const connection = connect("math-service", {
            eagerTargetQuery: "exists",
        });
        const handle = connection.handle();
        await handle.start();

        try {
            const sum = await handle.call.add(10, 20);
            const product = await handle.call.multiply(6, 7);
            console.log({ sum, product });
        } finally {
            await handle.stop();
        }
    } finally {
        await serviceProcess.stop();
    }
}

void main().catch(error => {
    console.error(error);
    process.exitCode = 1;
});
