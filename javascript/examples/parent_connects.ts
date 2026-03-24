/**
 * FILE: javascript/examples/parent_connects.ts
 * PURPOSE: Show multiple Node callers connecting to the same v5 service peer through the router.
 * OWNS: Caller-side connect/handle usage for concurrent remote calls.
 * EXPORTS: None
 * DOCS: docs/spec.md, javascript/README.md
 */

import { connect } from "../src/index.js";

async function callerTask(callerId: number): Promise<void> {
    const connection = connect("math-service", {
        eagerTargetQuery: "exists",
    });

    const handle = connection.handle();
    await handle.start();

    try {
        for (let iteration = 0; iteration < 3; iteration += 1) {
            const left = callerId * 10;
            const result = await handle.call.add(left, iteration);
            console.log(`caller ${callerId}: ${left} + ${iteration} = ${result}`);
        }
    } finally {
        await handle.stop();
    }
}

async function main(): Promise<void> {
    await Promise.all([callerTask(1), callerTask(2), callerTask(3)]);
}

void main().catch(error => {
    console.error(error);
    process.exitCode = 1;
});
